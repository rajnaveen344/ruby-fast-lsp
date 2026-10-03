//! An editor whose RSpec facts come from the `extensions/rspec-ruby` package,
//! with fixtures written into projects that lock `rspec-core` 3.x.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::environment::config::RubyFastLspConfig;
use crate::test::harness::FakeEditor;

const GEMFILE: &str = "source 'https://rubygems.org'\n\ngem 'rspec-core'\n";
const GEMFILE_LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    rspec-core (3.13.5)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  rspec-core\n";

fn rspec_package_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby")
}

pub(super) struct RspecEditor {
    editor: FakeEditor,
    projects: Vec<(TempDir, PathBuf)>,
}

impl RspecEditor {
    /// One project that locks `rspec-core` 3.x.
    pub(super) async fn new() -> Self {
        Self::with_projects(&[true]).await
    }

    /// One project per entry; `true` writes a lockfile that locks `rspec-core` 3.x.
    pub(super) async fn with_projects(locked: &[bool]) -> Self {
        let editor = FakeEditor::new().await;
        editor
            .server()
            .extension_registry()
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
        Self { editor, projects }
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

    /// Open a tagged fixture as `spec/inline_spec.rb` in the first project and
    /// run its assertions.
    pub(super) async fn check(&mut self, fixture: &str) {
        let filename = self.path("spec/inline_spec.rb");
        self.editor.open_and_check_fixture(&filename, fixture).await;
    }
}

impl Deref for RspecEditor {
    type Target = FakeEditor;

    fn deref(&self) -> &FakeEditor {
        &self.editor
    }
}

impl DerefMut for RspecEditor {
    fn deref_mut(&mut self) -> &mut FakeEditor {
        &mut self.editor
    }
}
