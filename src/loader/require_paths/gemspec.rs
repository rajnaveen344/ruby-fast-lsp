//! Statically read `require_paths` declared by a project's own gemspecs.
//!
//! A gemspec is project code: Bundler's `gemspec` and gem activation put its
//! `require_paths` on `$LOAD_PATH`, but this reader never evaluates it. Only
//! literal strings count (`'lib'`, `'lib'.freeze`), assigned with
//! `require_paths =` or appended with `require_paths <<`. Anything dynamic is
//! skipped, so an unproven folder never joins the load path.

use std::path::{Path, PathBuf};

use ruby_prism::{visit_call_node, CallNode, Node, Visit};

/// Gemspecs larger than this are not specifications worth reading statically.
const MAX_GEMSPEC_BYTES: u64 = 1024 * 1024;

/// Literal `require_paths` declared by the top-level gemspecs of
/// `project_root`, in gemspec name order and then source order, without
/// duplicates. Unreadable or oversized gemspecs contribute nothing.
pub fn declared_gemspec_require_paths(project_root: &Path) -> Vec<String> {
    let mut paths = Vec::new();
    for gemspec in root_gemspecs(project_root) {
        let Ok(content) = std::fs::read(&gemspec) else {
            continue;
        };
        for path in require_paths_in_source(&content) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// Configured `loadPaths` first, then the gemspec's declared folders, each
/// once. Callers probe `lib` and the project root after these.
pub fn project_load_paths(configured: Vec<String>, declared: &[String]) -> Vec<String> {
    let mut paths = configured;
    for path in declared {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    paths
}

fn root_gemspecs(project_root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(project_root) else {
        return Vec::new();
    };
    let mut gemspecs: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry
                .metadata()
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= MAX_GEMSPEC_BYTES)
        })
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "gemspec"))
        .collect();
    gemspecs.sort();
    gemspecs
}

fn require_paths_in_source(source: &[u8]) -> Vec<String> {
    let parse_result = ruby_prism::parse(source);
    let mut finder = RequirePathsFinder { paths: Vec::new() };
    finder.visit(&parse_result.node());
    finder.paths
}

struct RequirePathsFinder {
    paths: Vec<String>,
}

impl Visit<'_> for RequirePathsFinder {
    fn visit_call_node(&mut self, node: &CallNode<'_>) {
        let declared = match node.name().as_slice() {
            b"require_paths=" if node.receiver().is_some() => single_argument(node),
            b"<<" => node
                .receiver()
                .and_then(|receiver| receiver.as_call_node())
                .filter(|receiver| {
                    receiver.name().as_slice() == b"require_paths"
                        && receiver.receiver().is_some()
                        && receiver.arguments().is_none()
                })
                .and_then(|_| single_argument(node)),
            _ => None,
        };
        if let Some(value) = declared {
            self.paths.extend(literal_paths(&value));
        }
        visit_call_node(self, node);
    }
}

fn single_argument<'pr>(node: &CallNode<'pr>) -> Option<Node<'pr>> {
    let arguments = node.arguments()?;
    let mut arguments = arguments.arguments().iter();
    let first = arguments.next()?;
    arguments.next().is_none().then_some(first)
}

/// A literal string or an array of them; non-literal elements are skipped.
fn literal_paths(value: &Node<'_>) -> Vec<String> {
    match value.as_array_node() {
        Some(array) => array
            .elements()
            .iter()
            .filter_map(|element| literal_string(&element))
            .collect(),
        None => literal_string(value).into_iter().collect(),
    }
}

fn literal_string(node: &Node<'_>) -> Option<String> {
    if let Some(string) = node.as_string_node() {
        return String::from_utf8(string.unescaped().to_vec()).ok();
    }
    let call = node.as_call_node()?;
    if call.name().as_slice() != b"freeze" || call.arguments().is_some() || call.block().is_some() {
        return None;
    }
    literal_string(&call.receiver()?)
}

#[cfg(test)]
mod tests {
    use super::require_paths_in_source;

    #[test]
    fn reads_literal_assignments_and_appends() {
        let source = br#"
Gem::Specification.new do |spec|
  spec.name = "example"
  spec.require_paths = ['lib'.freeze, "lib/jars", dynamic_path]
  spec.require_paths << 'ext/vendor'
  spec.require_paths << "gen#{suffix}"
end
"#;
        assert_eq!(
            require_paths_in_source(source),
            vec!["lib", "lib/jars", "ext/vendor"]
        );
    }

    #[test]
    fn ignores_unrelated_and_receiverless_calls() {
        let source = br#"
require_paths = ['local']
other.require_paths
items << 'not/a/path'
spec.load_paths = ['also/not']
"#;
        assert!(require_paths_in_source(source).is_empty());
    }
}
