//! An editor whose RSpec facts come from one chosen implementation, with
//! fixtures written into projects that lock `rspec-core` 3.x.
//!
//! Every observation is also recorded, with paths relative to the owning
//! project, so the parity test can compare implementations exactly.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use tower_lsp::lsp_types::{Diagnostic, Location, Url, WorkspaceEdit};

use crate::environment::config::RubyFastLspConfig;
use crate::test::harness::parse_fixture;
use crate::test::harness::FakeEditor;

/// The RSpec implementation that produces facts for a scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rspec {
    /// The in-process native crate, which runs when no package claims a call.
    NativeFallback,
    /// The `extensions/rspec-ruby` Wasm package.
    Package,
}

const GEMFILE: &str = "source 'https://rubygems.org'\n\ngem 'rspec-core'\n";
const GEMFILE_LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    rspec-core (3.13.5)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  rspec-core\n";

fn rspec_package_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby")
}

pub(super) struct RspecEditor {
    editor: FakeEditor,
    projects: Vec<(TempDir, PathBuf)>,
    transcript: Vec<String>,
}

impl RspecEditor {
    /// One project that locks `rspec-core` 3.x.
    pub(super) async fn new(rspec: Rspec) -> Self {
        Self::with_projects(rspec, &[true]).await
    }

    /// One project per entry; `true` writes a lockfile that locks `rspec-core` 3.x.
    pub(super) async fn with_projects(rspec: Rspec, locked: &[bool]) -> Self {
        let editor = FakeEditor::new().await;
        if rspec == Rspec::Package {
            editor
                .server()
                .extensions
                .registry()
                .configure_from_config(&RubyFastLspConfig {
                    extension_packages: vec![rspec_package_dir().to_string_lossy().into_owned()],
                    ..RubyFastLspConfig::default()
                });
            let statuses = editor.server().extension_status_reports();
            assert!(
                statuses
                    .iter()
                    .any(|status| status.id == "rspec-ruby" && status.status == "loaded"),
                "the RSpec package must load: {statuses:?}"
            );
        }
        let mut projects = Vec::new();
        for locked in locked {
            let dir = TempDir::new().expect("RSpec project directory must exist");
            let root = dir
                .path()
                .canonicalize()
                .expect("RSpec project directory must canonicalize");
            std::fs::write(root.join("Gemfile"), GEMFILE).expect("Gemfile must be written");
            if *locked {
                std::fs::write(root.join("Gemfile.lock"), GEMFILE_LOCK)
                    .expect("Gemfile.lock must be written");
            }
            editor.add_workspace(&root.to_string_lossy());
            projects.push((dir, root));
        }
        Self {
            editor,
            projects,
            transcript: Vec::new(),
        }
    }

    /// The absolute name of `relative` inside project `project`.
    pub(super) fn path_in(&self, project: usize, relative: &str) -> String {
        self.projects[project]
            .1
            .join(relative)
            .to_string_lossy()
            .into_owned()
    }

    /// The absolute name of `relative` inside the first project.
    pub(super) fn path(&self, relative: &str) -> String {
        self.path_in(0, relative)
    }

    pub(super) async fn open(&mut self, filename: &str, content: &str) {
        self.editor.open(filename, content).await;
    }

    pub(super) async fn set(&mut self, filename: &str, content: &str) {
        self.editor.set(filename, content).await;
    }

    /// Open a tagged fixture as `spec/inline_spec.rb` in the first project and
    /// run its assertions; record the definition at its cursor and its
    /// diagnostics.
    pub(super) async fn check(&mut self, fixture: &str) {
        let filename = self.path("spec/inline_spec.rb");
        self.editor.open_and_check_fixture(&filename, fixture).await;
        if let Some(cursor) = parse_fixture(fixture).cursor {
            self.goto_def_at(&filename, cursor.line, cursor.character)
                .await;
        }
        self.diagnostics(&filename).await;
    }

    pub(super) async fn goto_def_at(
        &mut self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<Location> {
        let locations = self.editor.goto_def_at(filename, line, character).await;
        let observed = self.locations(&locations);
        let file = self.relative(filename);
        self.record(format!(
            "definition {file} {line}:{character} -> {observed:?}"
        ));
        locations
    }

    pub(super) async fn references_at(
        &mut self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<Location> {
        let locations = self.editor.references_at(filename, line, character).await;
        let observed = self.locations(&locations);
        let file = self.relative(filename);
        self.record(format!(
            "references {file} {line}:{character} -> {observed:?}"
        ));
        locations
    }

    pub(super) async fn diagnostics(&mut self, filename: &str) -> Vec<Diagnostic> {
        let diagnostics = self.editor.diagnostics(filename).await;
        let mut observed = diagnostics
            .iter()
            .map(|diagnostic| {
                format!(
                    "{:?} {:?} {}:{}-{}:{} {}",
                    diagnostic.severity,
                    diagnostic.code,
                    diagnostic.range.start.line,
                    diagnostic.range.start.character,
                    diagnostic.range.end.line,
                    diagnostic.range.end.character,
                    diagnostic.message
                )
            })
            .collect::<Vec<_>>();
        observed.sort();
        let file = self.relative(filename);
        self.record(format!("diagnostics {file} -> {observed:?}"));
        diagnostics
    }

    pub(super) async fn rename_at(
        &mut self,
        filename: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Option<WorkspaceEdit> {
        let edit = self
            .editor
            .rename_at(filename, line, character, new_name)
            .await;
        let observed = edit.as_ref().map(|edit| {
            let mut changes = edit
                .changes
                .iter()
                .flatten()
                .flat_map(|(uri, edits)| {
                    let file = self.uri_relative(uri);
                    edits.iter().map(move |edit| {
                        format!(
                            "{file}:{}:{}-{}:{} {}",
                            edit.range.start.line,
                            edit.range.start.character,
                            edit.range.end.line,
                            edit.range.end.character,
                            edit.new_text
                        )
                    })
                })
                .collect::<Vec<_>>();
            changes.sort();
            changes
        });
        let file = self.relative(filename);
        self.record(format!("rename {file} {line}:{character} -> {observed:?}"));
        edit
    }

    /// Everything this editor observed, in request order.
    pub(super) fn into_transcript(self) -> Vec<String> {
        self.transcript
    }

    fn record(&mut self, observation: String) {
        self.transcript.push(observation);
    }

    fn locations(&self, locations: &[Location]) -> Vec<String> {
        let mut observed = locations
            .iter()
            .map(|location| {
                format!(
                    "{}:{}:{}-{}:{}",
                    self.uri_relative(&location.uri),
                    location.range.start.line,
                    location.range.start.character,
                    location.range.end.line,
                    location.range.end.character
                )
            })
            .collect::<Vec<_>>();
        observed.sort();
        observed
    }

    fn uri_relative(&self, uri: &Url) -> String {
        match uri.to_file_path() {
            Ok(path) => self.relative(&path.to_string_lossy()),
            Err(()) => uri.to_string(),
        }
    }

    /// Replace a project root with its index so transcripts do not depend on
    /// temporary directory names.
    fn relative(&self, filename: &str) -> String {
        for (index, (_, root)) in self.projects.iter().enumerate() {
            if let Ok(relative) = Path::new(filename).strip_prefix(root) {
                return format!("project{index}/{}", relative.to_string_lossy());
            }
        }
        filename.to_string()
    }
}
