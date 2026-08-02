//! Resolve static `require` / `require_relative` string arguments to files.
//!
//! Navigation and unresolved-path diagnostics only: each project's configured
//! `loadPaths` participate in path search but are never scanned as indexing
//! roots. Dynamic forms (`autoload`, interpolated strings, `load` with
//! non-literals) stay unsupported and fail closed.
//!
//! ## Known gaps / follow-ups
//!
//! - **Stdlib / default-gem requires** are not integration-tested. Gem
//!   `require_paths` FakeEditor coverage exists; `require "json"` / `"uri"`
//!   through published runtime stdlib roots (and Ruby default gems that live
//!   outside Bundler) still need a regression that proves they clear after
//!   dependency indexing. Real workspaces can still show false
//!   `unresolved-require` on those until verified.
//! - **Cold-index coordinator path**: sticky-diagnostic clear is covered by
//!   calling `refresh_unresolved_require_diagnostics_for_workspace` directly,
//!   not by a full gem/stdlib indexing run that publishes roots then refreshes.
//! - **Precedence**: project `loadPaths` vs `lib` is tested; project-local
//!   hit winning over a same-named gem/stdlib feature is not.
//! - **True miss stays after refresh**: clear-on-ready is tested; a still-missing
//!   path remaining red after refresh is not asserted.
//! - **`Kernel.require` / `%q` delimiters**: finder accepts `Kernel`; content
//!   ranges only strip `'`/`"`. No FakeEditor coverage for either.
//! - **Out of scope (intentional)**: `autoload`, `load`, interpolated/dynamic
//!   arguments, non-`.rb` native extensions.

use std::path::{Component, Path, PathBuf};

use log::trace;
use ruby_analysis::core::{DiagnosticFact, DiagnosticSeverity, SourceFileId, TextRange};
use ruby_analysis::engine::AnalysisEngine;
use ruby_prism::{visit_call_node, CallNode, Visit};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

/// Diagnostic code for a static `require` / `require_relative` that cannot be resolved.
pub const UNRESOLVED_REQUIRE_CODE: &str = "unresolved-require";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequireKind {
    Require,
    RequireRelative,
}

impl RequireKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Require => "require",
            Self::RequireRelative => "require_relative",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequireStringTarget {
    pub kind: RequireKind,
    pub argument: String,
    /// Byte offsets of the string node (including quotes) in the source file.
    pub start_byte: usize,
    pub end_byte: usize,
}

impl RequireStringTarget {
    /// Byte range of the string contents, excluding surrounding `'` / `"` quotes.
    ///
    /// Used for Cmd-hover / definition origin underlines and unresolved-require
    /// diagnostics. Exotic delimiters (`%q{}`) fall back to the full node.
    pub fn content_byte_range(&self, source: &str) -> (usize, usize) {
        assert!(
            self.end_byte >= self.start_byte,
            "INVARIANT VIOLATED: require string end_byte ({}) is before start_byte ({}). \
             This is a bug because Prism locations are half-open [start, end). \
             Fix: construct RequireStringTarget only from string_node.location().",
            self.end_byte,
            self.start_byte
        );
        assert!(
            self.end_byte <= source.len(),
            "INVARIANT VIOLATED: require string end_byte ({}) exceeds source length ({}). \
             This is a bug because the target must come from this source buffer. \
             Fix: pass the same content used for parsing.",
            self.end_byte,
            source.len()
        );
        let bytes = source.as_bytes();
        let slice = &bytes[self.start_byte..self.end_byte];
        if slice.len() >= 2 {
            let open = slice[0];
            let close = slice[slice.len() - 1];
            if (open == b'\'' || open == b'"') && open == close {
                return (self.start_byte + 1, self.end_byte - 1);
            }
        }
        (self.start_byte, self.end_byte)
    }
}

/// Find a static require/require_relative string under the cursor.
///
/// Only bare / `Kernel` receivers with a single literal string argument are
/// accepted. Interpolated or dynamic arguments fail closed.
pub fn find_require_string_at_offset(
    content: &str,
    byte_offset: usize,
) -> Option<RequireStringTarget> {
    let parse_result = ruby_prism::parse(content.as_bytes());
    let mut finder = RequireStringFinder {
        byte_offset: Some(byte_offset),
        results: Vec::new(),
    };
    finder.visit(&parse_result.node());
    finder.results.into_iter().next()
}

/// Collect every static `require` / `require_relative` string in a file.
pub fn find_all_require_strings(content: &str) -> Vec<RequireStringTarget> {
    let parse_result = ruby_prism::parse(content.as_bytes());
    let mut finder = RequireStringFinder {
        byte_offset: None,
        results: Vec::new(),
    };
    finder.visit(&parse_result.node());
    finder.results
}

/// Emit diagnostics for static requires whose path cannot be resolved.
///
/// `autoload` / `load` / dynamic arguments are intentionally ignored.
pub fn unresolved_require_diagnostics(
    content: &str,
    file_id: SourceFileId,
    current_file: &Path,
    project_root: &Path,
    load_paths: &[String],
    dependency_roots: &[PathBuf],
    engine: Option<&AnalysisEngine>,
) -> Vec<DiagnosticFact> {
    let mut diagnostics = Vec::new();
    for target in find_all_require_strings(content) {
        if resolve_require_path(
            target.kind,
            &target.argument,
            current_file,
            project_root,
            load_paths,
            dependency_roots,
            engine,
        )
        .is_some()
        {
            continue;
        }
        let (content_start, content_end) = target.content_byte_range(content);
        let start_byte = u32::try_from(content_start).expect(
            "INVARIANT VIOLATED: require diagnostic start offset exceeded u32. \
             This is a bug because TextRange stores u32 offsets. \
             Fix: widen TextRange before indexing files larger than u32::MAX bytes.",
        );
        let end_byte = u32::try_from(content_end).expect(
            "INVARIANT VIOLATED: require diagnostic end offset exceeded u32. \
             This is a bug because TextRange stores u32 offsets. \
             Fix: widen TextRange before indexing files larger than u32::MAX bytes.",
        );
        diagnostics.push(DiagnosticFact::new(
            TextRange::new(file_id, start_byte, end_byte),
            DiagnosticSeverity::Error,
            UNRESOLVED_REQUIRE_CODE,
            format!(
                "Cannot resolve {} \"{}\"",
                target.kind.as_str(),
                target.argument
            ),
        ));
    }
    diagnostics
}

/// Resolve a require string to an existing file path.
///
/// Search order:
/// - `require_relative`: `dirname(current_file)` only
/// - `require`: configured project `loadPaths`, then `<project>/lib`, then
///   project root, then absolute dependency require roots (gem/stdlib
///   `require_paths` discovered for this project)
///
/// Each candidate is tried as-is and with a `.rb` suffix. A hit is any path that
/// exists on disk or is already registered in the project engine.
pub fn resolve_require_path(
    kind: RequireKind,
    argument: &str,
    current_file: &Path,
    project_root: &Path,
    load_paths: &[String],
    dependency_roots: &[PathBuf],
    engine: Option<&AnalysisEngine>,
) -> Option<PathBuf> {
    if argument.is_empty() {
        return None;
    }

    let candidates = match kind {
        RequireKind::RequireRelative => {
            let parent = current_file.parent()?;
            vec![parent.join(argument)]
        }
        RequireKind::Require => {
            let mut roots = Vec::new();
            for configured in load_paths {
                if let Some(root) = validated_project_relative_dir(project_root, configured) {
                    roots.push(root);
                }
            }
            roots.push(project_root.join("lib"));
            roots.push(project_root.to_path_buf());
            for dependency_root in dependency_roots {
                if dependency_root.as_os_str().is_empty() {
                    continue;
                }
                roots.push(dependency_root.clone());
            }
            roots
                .into_iter()
                .map(|root| root.join(argument))
                .collect()
        }
    };

    for candidate in candidates {
        if let Some(resolved) = existing_require_candidate(&candidate, engine) {
            return Some(resolved);
        }
    }
    None
}

/// Build a goto location that selects the entire target file contents.
pub fn location_for_require_target(
    path: &Path,
    engine: Option<&AnalysisEngine>,
) -> Option<Location> {
    let uri = Url::from_file_path(path).ok()?;
    let range = require_target_full_range(path, engine)
        .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
    Some(Location { uri, range })
}

fn require_target_full_range(path: &Path, engine: Option<&AnalysisEngine>) -> Option<Range> {
    if let Some(content) = require_target_content(path, engine) {
        return Some(full_document_range(&content));
    }
    if let Some(engine) = engine {
        if let Some(file_id) = engine.query().file_id(path) {
            if let Some(file) = engine.query().file(file_id) {
                return Some(range_from_engine_file(file));
            }
        }
    }
    None
}

fn require_target_content(path: &Path, engine: Option<&AnalysisEngine>) -> Option<String> {
    if let Some(engine) = engine {
        if let Some(file_id) = engine.query().file_id(path) {
            if let Some(file) = engine.query().file(file_id) {
                if let Some(source) = file.source_text() {
                    return Some(source.to_string());
                }
            }
        }
    }
    std::fs::read_to_string(path).ok()
}

fn range_from_engine_file(file: &ruby_analysis::engine::SourceFile) -> Range {
    if let Some(source) = file.source_text() {
        return full_document_range(source);
    }
    let end_line = u32::try_from(file.line_index.line_offsets().len().saturating_sub(1)).expect(
        "INVARIANT VIOLATED: require target line count exceeded u32. \
         This is a bug because LSP positions require u32 lines. \
         Fix: reject or segment files with more than u32::MAX lines.",
    );
    Range::new(Position::new(0, 0), Position::new(end_line, 0))
}

fn full_document_range(content: &str) -> Range {
    let line = content.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let last_line = content.rsplit('\n').next().unwrap_or("");
    Range::new(
        Position::new(0, 0),
        Position::new(line, last_line.encode_utf16().count() as u32),
    )
}

fn validated_project_relative_dir(project_root: &Path, configured: &str) -> Option<PathBuf> {
    let relative = Path::new(configured);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        trace!(
            "ignoring invalid indexing.loadPaths entry (must be project-relative without traversal): {}",
            configured
        );
        return None;
    }
    Some(project_root.join(relative))
}

fn existing_require_candidate(
    candidate: &Path,
    engine: Option<&AnalysisEngine>,
) -> Option<PathBuf> {
    for path in [candidate.to_path_buf(), with_rb_extension(candidate)] {
        if path.is_file() {
            return Some(path);
        }
        if let Some(engine) = engine {
            if engine.query().file_id(&path).is_some() {
                return Some(path);
            }
        }
    }
    None
}

fn with_rb_extension(path: &Path) -> PathBuf {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rb"))
    {
        return path.to_path_buf();
    }
    let mut with_ext = path.to_path_buf();
    let mut file_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    file_name.push(".rb");
    with_ext.set_file_name(file_name);
    with_ext
}

struct RequireStringFinder {
    /// When set, only keep the target whose string contains this offset.
    byte_offset: Option<usize>,
    results: Vec<RequireStringTarget>,
}

impl Visit<'_> for RequireStringFinder {
    fn visit_call_node(&mut self, node: &CallNode<'_>) {
        if let Some(target) = static_require_target(node) {
            if let Some(offset) = self.byte_offset {
                if offset >= target.start_byte && offset <= target.end_byte {
                    self.results.push(target);
                    return;
                }
            } else {
                self.results.push(target);
            }
        }

        visit_call_node(self, node);
    }
}

fn static_require_target(node: &CallNode<'_>) -> Option<RequireStringTarget> {
    let kind = match node.name().as_slice() {
        b"require" => RequireKind::Require,
        b"require_relative" => RequireKind::RequireRelative,
        _ => return None,
    };

    if let Some(receiver) = node.receiver() {
        let is_kernel = receiver
            .as_constant_read_node()
            .is_some_and(|constant| constant.name().as_slice() == b"Kernel");
        if !is_kernel {
            return None;
        }
    }

    let arguments = node.arguments()?;
    let mut args = arguments.arguments().iter();
    let first = args.next()?;
    if args.next().is_some() {
        return None;
    }

    let string = first.as_string_node()?;
    let location = string.location();
    let argument = String::from_utf8_lossy(string.unescaped()).into_owned();
    Some(RequireStringTarget {
        kind,
        argument,
        start_byte: location.start_offset(),
        end_byte: location.end_offset(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_analysis::core::SourceKind;
    use ruby_analysis::engine::{AnalysisEngine, SourceFileInput};

    #[test]
    fn finds_static_require_string_under_cursor() {
        let source = "require \"foo/bar\"\n";
        let offset = source.find("foo").unwrap();
        let target = find_require_string_at_offset(source, offset).unwrap();
        assert_eq!(target.kind, RequireKind::Require);
        assert_eq!(target.argument, "foo/bar");
    }

    #[test]
    fn content_byte_range_excludes_quotes() {
        let source = "require 'platform/helpers/json'\n";
        let target = find_require_string_at_offset(source, source.find("json").unwrap()).unwrap();
        let (start, end) = target.content_byte_range(source);
        assert_eq!(&source[start..end], "platform/helpers/json");
        assert_eq!(&source[target.start_byte..target.end_byte], "'platform/helpers/json'");
    }

    #[test]
    fn rejects_interpolated_require_string() {
        let source = "require \"a#{b}\"\n";
        let offset = source.find('a').unwrap();
        assert!(find_require_string_at_offset(source, offset).is_none());
    }

    #[test]
    fn collects_all_static_requires_in_file() {
        let source = "require \"a\"\nrequire_relative \"./b\"\nautoload :C, \"c\"\n";
        let targets = find_all_require_strings(source);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].argument, "a");
        assert_eq!(targets[1].argument, "./b");
        assert_eq!(targets[1].kind, RequireKind::RequireRelative);
    }

    #[test]
    fn unresolved_require_emits_diagnostic_on_string() {
        let source = "require \"missing\"\n";
        let file_id = SourceFileId(1);
        let diagnostics = unresolved_require_diagnostics(
            source,
            file_id,
            Path::new("/project/main.rb"),
            Path::new("/project"),
            &[],
            &[],
            None,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, UNRESOLVED_REQUIRE_CODE);
        assert_eq!(diagnostics[0].range.file_id, file_id);
        assert!(diagnostics[0].message.contains("missing"));
        let start = diagnostics[0].range.start_byte as usize;
        let end = diagnostics[0].range.end_byte as usize;
        assert_eq!(&source[start..end], "missing");
    }

    #[test]
    fn resolved_require_emits_no_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("lib").join("foo.rb");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "# foo\n").unwrap();
        let source = "require \"foo\"\n";
        let diagnostics = unresolved_require_diagnostics(
            source,
            SourceFileId(1),
            &dir.path().join("main.rb"),
            dir.path(),
            &[],
            &[],
            None,
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn require_relative_resolves_beside_current_file() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("app").join("main.rb");
        let target = dir.path().join("app").join("foo.rb");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&current, "require_relative \"./foo\"\n").unwrap();
        std::fs::write(&target, "# foo\n").unwrap();

        let resolved = resolve_require_path(
            RequireKind::RequireRelative,
            "./foo",
            &current,
            dir.path(),
            &[],
            &[],
            None,
        )
        .unwrap();
        assert_eq!(resolved, target);
    }

    #[test]
    fn require_prefers_configured_load_path_before_lib() {
        let dir = tempfile::tempdir().unwrap();
        let custom = dir.path().join("custom").join("foo.rb");
        let lib = dir.path().join("lib").join("foo.rb");
        std::fs::create_dir_all(custom.parent().unwrap()).unwrap();
        std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
        std::fs::write(&custom, "# custom\n").unwrap();
        std::fs::write(&lib, "# lib\n").unwrap();

        let resolved = resolve_require_path(
            RequireKind::Require,
            "foo",
            &dir.path().join("main.rb"),
            dir.path(),
            &["custom".to_string()],
            &[],
            None,
        )
        .unwrap();
        assert_eq!(resolved, custom);
    }

    #[test]
    fn require_can_resolve_engine_registered_virtual_files() {
        let mut engine = AnalysisEngine::new();
        let path = PathBuf::from("/project/lib/foo.rb");
        engine.register_file(SourceFileInput {
            path: path.clone(),
            content: "# foo\n".to_string(),
            kind: SourceKind::Project,
        });

        let resolved = resolve_require_path(
            RequireKind::Require,
            "foo",
            Path::new("/project/main.rb"),
            Path::new("/project"),
            &[],
            &[],
            Some(&engine),
        )
        .unwrap();
        assert_eq!(resolved, path);
    }

    #[test]
    fn require_resolves_through_dependency_require_roots() {
        let dir = tempfile::tempdir().unwrap();
        let gem_lib = dir.path().join("gems/demo-1.0.0/lib");
        let target = gem_lib.join("platform/helpers/json.rb");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "# gem json\n").unwrap();

        let resolved = resolve_require_path(
            RequireKind::Require,
            "platform/helpers/json",
            &dir.path().join("main.rb"),
            dir.path(),
            &[],
            &[gem_lib],
            None,
        )
        .unwrap();
        assert_eq!(resolved, target);
    }

    #[test]
    fn project_lib_still_wins_before_dependency_roots() {
        let dir = tempfile::tempdir().unwrap();
        let gem_lib = dir.path().join("gems/demo-1.0.0/lib");
        let gem_target = gem_lib.join("foo.rb");
        let project_target = dir.path().join("lib/foo.rb");
        std::fs::create_dir_all(gem_target.parent().unwrap()).unwrap();
        std::fs::create_dir_all(project_target.parent().unwrap()).unwrap();
        std::fs::write(&gem_target, "# gem\n").unwrap();
        std::fs::write(&project_target, "# project\n").unwrap();

        let resolved = resolve_require_path(
            RequireKind::Require,
            "foo",
            &dir.path().join("main.rb"),
            dir.path(),
            &[],
            &[gem_lib],
            None,
        )
        .unwrap();
        assert_eq!(resolved, project_target);
    }

    #[test]
    fn location_spans_entire_target_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo.rb");
        let content = "# line1\nclass Foo\nend\n";
        std::fs::write(&path, content).unwrap();

        let location = location_for_require_target(&path, None).unwrap();
        assert_eq!(location.range.start, Position::new(0, 0));
        assert_eq!(location.range.end, Position::new(3, 0));
    }
}
