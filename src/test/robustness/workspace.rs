//! A corpus copied to disk and indexed as a real project, as an editor would.

use std::path::{Path, PathBuf};

use super::corpus::Corpus;
use super::observe::{self, Snapshot};
use crate::loader::scheduling::status::IndexingPhase;
use crate::lsp::lifecycle::indexing::init_workspace_for_run;
use crate::test::harness::FakeEditor;

/// One temporary project directory holding a copy of the corpus.
pub struct ProjectDir {
    _dir: tempfile::TempDir,
    cache: tempfile::TempDir,
    root: PathBuf,
}

impl ProjectDir {
    pub fn write(corpus: &Corpus) -> Self {
        let dir = tempfile::tempdir().expect("create robustness project directory");
        let root =
            dunce::canonicalize(dir.path()).expect("canonicalize robustness project directory");
        for file in &corpus.files {
            let path = root.join(&file.name);
            std::fs::create_dir_all(path.parent().expect("corpus files live under the root"))
                .expect("create corpus subdirectory");
            std::fs::write(&path, &file.content).expect("write corpus file");
        }
        Self {
            _dir: dir,
            cache: tempfile::tempdir().expect("create robustness cache directory"),
            root,
        }
    }

    /// The `FakeEditor` file name for a corpus-relative name.
    pub fn filename(&self, name: &str) -> String {
        editor_name(&self.root.join(name))
    }

    /// Starts an editor on this project, indexes it, then opens `order`.
    pub async fn open(&self, corpus: &Corpus, order: &[&str]) -> Session<'_> {
        let mut editor = FakeEditor::with_cache_root(self.cache.path().to_path_buf()).await;
        let server = editor.server().clone();
        server.set_discovered_runtimes_for_tests(Vec::new());
        let root_uri = tower_lsp::lsp_types::Url::from_directory_path(&self.root)
            .expect("project root is an absolute directory");
        let workspace = server.add_workspace(root_uri.clone());
        let run = workspace.begin_indexing_run();
        init_workspace_for_run(&server, root_uri, run.clone())
            .await
            .expect("index robustness project");
        workspace
            .indexing_status
            .transition(run.generation(), IndexingPhase::Ready, None, None)
            .expect("mark the indexed project ready");
        for &name in order {
            editor
                .open(&self.filename(name), corpus.content(name))
                .await;
        }
        Session {
            project: self,
            editor,
        }
    }
}

/// An editor with the project indexed and some files open.
pub struct Session<'a> {
    project: &'a ProjectDir,
    pub editor: FakeEditor,
}

impl Session<'_> {
    pub fn filename(&self, name: &str) -> String {
        self.project.filename(name)
    }

    pub fn content(&self, name: &str) -> &str {
        self.editor.content(&self.filename(name))
    }

    /// Observations for `names`, keyed and written with project-relative paths
    /// so sessions on different temporary directories compare equal.
    pub async fn snapshot(&self, names: &[&str]) -> Snapshot {
        let files = names
            .iter()
            .map(|name| self.filename(name))
            .collect::<Vec<_>>();
        let files = files.iter().map(String::as_str).collect::<Vec<_>>();
        let prefix = editor_name(&self.project.root);
        observe::snapshot(&self.editor, &files)
            .await
            .into_iter()
            .map(|(key, value)| {
                (
                    key.replace(&prefix, "<root>"),
                    value.replace(&prefix, "<root>"),
                )
            })
            .collect()
    }
}

fn editor_name(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string()
}
