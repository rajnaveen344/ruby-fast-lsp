//! Loads the Ruby sources a robustness check runs against.

use std::path::{Path, PathBuf};

/// Files larger than this are skipped so one generated file cannot dominate a run.
const MAX_FILE_BYTES: usize = 64 * 1024;
/// An external corpus is truncated to this many files, in path order.
const MAX_EXTERNAL_FILES: usize = 200;

/// One Ruby file, named by its path relative to the corpus root.
pub struct CorpusFile {
    pub name: String,
    pub content: String,
}

pub struct Corpus {
    pub files: Vec<CorpusFile>,
    /// True for the reviewed built-in corpus, which must be diagnostic-free.
    pub is_builtin: bool,
}

impl Corpus {
    /// The corpus selected by `ROBUSTNESS_CORPUS`, or the built-in one.
    pub fn selected() -> Self {
        match std::env::var_os("ROBUSTNESS_CORPUS") {
            Some(root) => Self::load(Path::new(&root), MAX_EXTERNAL_FILES, false),
            None => Self::builtin(),
        }
    }

    pub fn builtin() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/test/fixtures/robustness");
        Self::load(&root, usize::MAX, true)
    }

    fn load(root: &Path, max_files: usize, is_builtin: bool) -> Self {
        let mut paths = Vec::new();
        collect_ruby_files(root, &mut paths);
        paths.sort();
        let files = paths
            .into_iter()
            .filter_map(|path| {
                let content = std::fs::read_to_string(&path).ok()?;
                if content.len() > MAX_FILE_BYTES {
                    return None;
                }
                let name = path
                    .strip_prefix(root)
                    .expect("corpus walk only yields paths under its root")
                    .to_string_lossy()
                    .replace('\\', "/");
                Some(CorpusFile { name, content })
            })
            .take(max_files)
            .collect::<Vec<_>>();
        assert!(
            !files.is_empty(),
            "robustness corpus {} has no readable Ruby files",
            root.display()
        );
        Self { files, is_builtin }
    }

    pub fn names(&self) -> Vec<&str> {
        self.files.iter().map(|file| file.name.as_str()).collect()
    }

    pub fn content(&self, name: &str) -> &str {
        &self
            .files
            .iter()
            .find(|file| file.name == name)
            .expect("corpus lookups use names taken from the same corpus")
            .content
    }
}

fn collect_ruby_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ruby_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rb") {
            out.push(path);
        }
    }
}
