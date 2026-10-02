//! The persisted gem product identity hashes the semantic producer sources
//! listed in `build.rs`. A workspace crate that those sources call is a
//! producer input too, so its source tree must be listed as well.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn listed_paths(build_script: &str, constant: &str) -> Vec<String> {
    let start = build_script
        .find(&format!("const {constant}: &[&str] = &["))
        .unwrap_or_else(|| panic!("build.rs must declare `{constant}`"));
    let body = &build_script[start..];
    let body = &body[body.find("= &[").expect("constant has an array body") + 4..];
    let body = &body[..body.find("];").expect("constant array is closed")];
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn rust_files(path: &Path, output: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries = fs::read_dir(path)
            .expect("producer tree is readable")
            .map(|entry| entry.expect("producer entry is readable").path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            rust_files(&entry, output);
        }
    } else if path.extension().is_some_and(|extension| extension == "rs") {
        output.push(path.to_path_buf());
    }
}

/// Library name (`a_b`) to source tree (`crates/a-b/src`) for workspace crates.
fn workspace_library_trees(root: &Path) -> BTreeMap<String, String> {
    let mut trees = BTreeMap::new();
    for entry in fs::read_dir(root.join("crates")).expect("crates directory is readable") {
        let directory = entry.expect("crate entry is readable").path();
        let Ok(manifest) = fs::read_to_string(directory.join("Cargo.toml")) else {
            continue;
        };
        if !directory.join("src/lib.rs").is_file() {
            continue;
        }
        let name = manifest
            .lines()
            .find_map(|line| line.trim().strip_prefix("name = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("workspace crate manifest names its package");
        let folder = directory
            .file_name()
            .and_then(|name| name.to_str())
            .expect("crate folder name is UTF-8");
        trees.insert(name.replace('-', "_"), format!("crates/{folder}/src"));
    }
    trees
}

#[test]
fn gem_producer_identity_covers_every_workspace_crate_its_producers_call() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let build_script = fs::read_to_string(root.join("build.rs")).expect("build.rs is readable");
    let trees = listed_paths(&build_script, "GEM_FACT_PRODUCER_TREES");
    let files = listed_paths(&build_script, "GEM_FACT_PRODUCER_FILES");
    let libraries = workspace_library_trees(root);

    let mut sources = Vec::new();
    for listed in trees.iter().chain(&files) {
        rust_files(&root.join(listed), &mut sources);
    }
    let mut missing = BTreeMap::<String, String>::new();
    for source in sources {
        let content = fs::read_to_string(&source).expect("producer source is readable");
        for (library, tree) in &libraries {
            let used = content.contains(&format!("{library}::"))
                || content.contains(&format!("use {library};"));
            if used && !trees.contains(tree) {
                missing.entry(tree.clone()).or_insert_with(|| {
                    source
                        .strip_prefix(root)
                        .unwrap_or(&source)
                        .display()
                        .to_string()
                });
            }
        }
    }
    assert!(
        missing.is_empty(),
        "gem producer sources call workspace crates whose sources are not part of the persisted \
         gem product identity (tree -> first caller): {missing:?}; list each tree in \
         build.rs GEM_FACT_PRODUCER_TREES"
    );
}
