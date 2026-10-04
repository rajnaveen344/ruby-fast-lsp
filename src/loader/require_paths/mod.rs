//! Resolve static `require` / `require_relative` string arguments to files.
//!
//! Navigation and unresolved-path diagnostics only: each project's configured
//! `loadPaths` participate in path search but are never scanned as indexing
//! roots. Dynamic forms (`autoload`, interpolated strings, `load` with
//! non-literals) stay unsupported and fail closed.
//!
//! ## Known gaps / follow-ups
//!
//! - **Cold runtime roots**: the full coordinator regression
//!   `cold_runtime_require_roots_refresh_open_diagnostics_and_preserve_project_precedence`
//!   uses an exact fixture runtime probe with stdlib and separate default-gem
//!   load paths. It verifies `json` / `uri` resolution and diagnostic refresh
//!   without edits, a retained true miss, and project-local precedence over a
//!   same-named runtime feature. This does not substitute for installed-runtime
//!   acceptance: default gems absent from that runtime's reported load path
//!   are not discovered by this mechanism.
//! - **Precedence**: project `loadPaths` vs `lib` and project-local vs stdlib
//!   are tested; the equivalent collision with a locked gem needs coverage.
//! - **True miss stays after refresh**: covered by
//!   `unresolved_require_stays_after_refresh_when_still_missing` and the
//!   stored-fact reresolve tests in this module.
//! - **Feature index**: published gem/stdlib roots are walked once into
//!   `RequireFeatureIndex`. Goto, hover, collection, and refresh share that
//!   map. Project `loadPaths` / `lib` / root still `stat`. Native `.so` stays
//!   out of scope.
//! - **`Kernel.require` / `%q` delimiters**: finder accepts `Kernel`; content
//!   ranges only strip `'`/`"`. No FakeEditor coverage for either.
//! - **Out of scope (intentional)**: `autoload`, `load`, interpolated/dynamic
//!   arguments, non-`.rb` native extensions.

mod gemspec;

pub use gemspec::{declared_gemspec_require_paths, project_load_paths};

use crate::invariant::ExpectInvariant;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use log::trace;
use ruby_analysis::core::{DiagnosticFact, DiagnosticSeverity, SourceFileId, TextRange};
use ruby_analysis::engine::{View, UNRESOLVED_REQUIRE_CODE};
use ruby_prism::{visit_call_node, CallNode, Node, Visit};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

/// One-shot map from a `require` feature string to the first matching file.
///
/// Built from published gem/stdlib require roots. With an engine, snapshot
/// indexed `.rb` paths once and scan prefix ranges. Without an engine, walk
/// those roots on disk (tests / FakeEditor). Lookup is O(1). Project
/// `loadPaths` / `lib` / root are still probed with `is_file` first.
#[derive(Debug, Clone, Default)]
pub struct RequireFeatureIndex {
    /// Values share the engine's path allocation when built from a view.
    by_feature: HashMap<Box<str>, Arc<Path>>,
}

impl RequireFeatureIndex {
    pub fn empty() -> Self {
        Self::default()
    }

    /// When `view` is present, snapshot its `.rb` paths once and assign each
    /// file to the first published root that is a prefix (sorted range scan).
    /// Without an engine (tests and FakeEditor root injection), walk on-disk
    /// `.rb` files. First insert wins, matching sequential `$LOAD_PATH` search.
    pub fn build(roots: &[PathBuf], view: Option<&View<'_>>) -> Self {
        if let Some(view) = view {
            let mut paths: Vec<&Arc<Path>> = view
                .files()
                .filter(|file| {
                    file.path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("rb"))
                })
                .map(|file| &file.path)
                .collect();
            paths.sort_unstable();
            return Self::from_sorted_indexed_paths(roots, &paths);
        }
        let mut index = Self::empty();
        for root in roots {
            if root.as_os_str().is_empty() {
                continue;
            }
            index.add_disk_root(root);
        }
        index
    }

    fn from_sorted_indexed_paths(roots: &[PathBuf], sorted: &[&Arc<Path>]) -> Self {
        let mut index = Self::empty();
        for root in roots {
            if root.as_os_str().is_empty() {
                continue;
            }
            let start = sorted.partition_point(|path| ***path < *root.as_path());
            for path in &sorted[start..] {
                if !path.starts_with(root) {
                    break;
                }
                let Ok(relative) = path.strip_prefix(root) else {
                    continue;
                };
                index.insert_relative(relative, Arc::clone(path));
            }
        }
        index
    }

    pub fn feature_count(&self) -> usize {
        self.by_feature.len()
    }

    pub fn lookup(&self, argument: &str) -> Option<&Path> {
        self.by_feature.get(argument).map(|path| &**path)
    }

    fn add_disk_root(&mut self, root: &Path) {
        self.walk_rb_files(root, root, 0);
    }

    fn walk_rb_files(&mut self, dir: &Path, root: &Path, depth: u32) {
        if depth > 64 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.'))
                {
                    continue;
                }
                self.walk_rb_files(&path, root, depth + 1);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rb"))
            {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            self.insert_relative(relative, Arc::from(path.as_path()));
        }
    }

    fn insert_relative(&mut self, relative: &Path, absolute: Arc<Path>) {
        let Some((with_ext, bare)) = require_feature_keys(relative) else {
            return;
        };
        self.by_feature
            .entry(with_ext.into_boxed_str())
            .or_insert_with(|| Arc::clone(&absolute));
        if let Some(bare) = bare {
            self.by_feature
                .entry(bare.into_boxed_str())
                .or_insert(absolute);
        }
    }
}

fn require_feature_keys(relative: &Path) -> Option<(String, Option<String>)> {
    let mut key = String::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str()?;
                if !key.is_empty() {
                    key.push('/');
                }
                key.push_str(part);
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return None;
            }
        }
    }
    if key.is_empty() {
        return None;
    }
    let bare = key
        .strip_suffix(".rb")
        .or_else(|| key.strip_suffix(".RB"))
        .map(ToOwned::to_owned);
    Some((key, bare))
}

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
        invariant!(
            self.end_byte >= self.start_byte,
            what = "require string end_byte ({}) is before start_byte ({})",
            why = "prism locations are half-open [start, end)",
            fix = "construct RequireStringTarget only from string_node.location()",
            self.end_byte,
            self.start_byte,
        );
        invariant!(
            self.end_byte <= source.len(),
            what = "require string end_byte ({}) exceeds source length ({})",
            why = "the target must come from this source buffer",
            fix = "pass the same content used for parsing",
            self.end_byte,
            source.len(),
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

/// Collect every static `require` / `require_relative` string under an
/// already parsed root, so callers that hold a parse never parse again.
pub fn static_require_targets(root: &Node<'_>) -> Vec<RequireStringTarget> {
    let mut finder = RequireStringFinder {
        byte_offset: None,
        results: Vec::new(),
    };
    finder.visit(root);
    finder.results
}

/// Emit diagnostics for static requires whose path cannot be resolved.
///
/// `autoload` / `load` / dynamic arguments are intentionally ignored.
pub fn unresolved_require_diagnostics(
    content: &str,
    root: &Node<'_>,
    file_id: SourceFileId,
    current_file: &Path,
    project_root: &Path,
    load_paths: &[String],
    feature_index: &RequireFeatureIndex,
    view: Option<&View<'_>>,
) -> Vec<DiagnosticFact> {
    let mut diagnostics = Vec::new();
    for target in static_require_targets(root) {
        if resolve_require_path(
            target.kind,
            &target.argument,
            current_file,
            project_root,
            load_paths,
            feature_index,
            view,
        )
        .is_some()
        {
            continue;
        }
        diagnostics.push(require_diagnostic_for_target(content, file_id, &target));
    }
    diagnostics
}

/// Complete static require candidates for delayed open-document refresh.
/// Resolve these against the current engine at commit time, including targets
/// that existed during collection but may have been removed before commit.
pub fn require_diagnostic_candidates(
    content: &str,
    root: &Node<'_>,
    file_id: SourceFileId,
) -> Vec<DiagnosticFact> {
    static_require_targets(root)
        .iter()
        .map(|target| require_diagnostic_for_target(content, file_id, target))
        .collect()
}

fn require_diagnostic_for_target(
    content: &str,
    file_id: SourceFileId,
    target: &RequireStringTarget,
) -> DiagnosticFact {
    let (content_start, content_end) = target.content_byte_range(content);
    let start_byte = u32::try_from(content_start).expect_invariant(
        "require diagnostic start offset exceeded u32",
        "TextRange stores u32 offsets",
        "widen TextRange before indexing files larger than u32::MAX bytes",
    );
    let end_byte = u32::try_from(content_end).expect_invariant(
        "require diagnostic end offset exceeded u32",
        "TextRange stores u32 offsets",
        "widen TextRange before indexing files larger than u32::MAX bytes",
    );
    DiagnosticFact::new(
        TextRange::new(file_id, start_byte, end_byte),
        DiagnosticSeverity::Error,
        UNRESOLVED_REQUIRE_CODE,
        unresolved_require_message(target.kind, &target.argument),
    )
}

/// Re-check stored `unresolved-require` facts after dependency roots change.
///
/// Closed project files were already parsed during collection. ASCII sources
/// are not retained in the engine, so refresh must not Prism-parse them again
/// or re-read them just to recover the require string. Kind and argument live
/// in the diagnostic message; keep the fact only when the path is still missing.
/// Adding gem/stdlib roots can only clear diagnostics, not invent new ones.
pub fn reresolve_unresolved_require_diagnostics(
    current_file: &Path,
    project_root: &Path,
    load_paths: &[String],
    feature_index: &RequireFeatureIndex,
    view: Option<&View<'_>>,
    existing: &[DiagnosticFact],
) -> Vec<DiagnosticFact> {
    existing
        .iter()
        .filter(|fact| fact.code == UNRESOLVED_REQUIRE_CODE)
        .filter(|fact| {
            let (kind, argument) = require_target_from_unresolved_message(&fact.message);
            resolve_require_path(
                kind,
                argument,
                current_file,
                project_root,
                load_paths,
                feature_index,
                view,
            )
            .is_none()
        })
        .cloned()
        .collect()
}

fn unresolved_require_message(kind: RequireKind, argument: &str) -> String {
    format!("Cannot resolve {} \"{}\"", kind.as_str(), argument)
}

fn require_target_from_unresolved_message(message: &str) -> (RequireKind, &str) {
    let (kind, rest) = if let Some(rest) =
        message.strip_prefix("Cannot resolve require_relative \"")
    {
        (RequireKind::RequireRelative, rest)
    } else if let Some(rest) = message.strip_prefix("Cannot resolve require \"") {
        (RequireKind::Require, rest)
    } else {
        unreachable_invariant!(
                what = "diagnostic message `{message}` is not an unresolved-require diagnostic",
                why = "require refresh sees only unresolved_require_diagnostics facts",
                fix = "filter by code `{UNRESOLVED_REQUIRE_CODE}`; keep unresolved_require_message's format",
                message = message,
                UNRESOLVED_REQUIRE_CODE = UNRESOLVED_REQUIRE_CODE,
            );
    };
    let argument = rest.strip_suffix('"').unwrap_or_else(|| {
        unreachable_invariant!(
            what = "unresolved-require diagnostic message `{message}` is missing its closing quote",
            why = "unresolved_require_message always quote-wraps the require argument",
            fix = "keep message construction and parsing in this module",
            message = message,
        )
    });
    (kind, argument)
}

/// Resolve a require string to an existing file path.
///
/// Search order:
/// - `require_relative`: `dirname(current_file)` only
/// - `require`: `load_paths` (configured project `loadPaths`, then the
///   gemspec's declared `require_paths`), then `<project>/lib`, then
///   project root (each probed as-is and with `.rb`), then the published
///   gem/stdlib feature index
///
/// Project probes hit any path that exists on disk or is already registered in
/// the project engine. Dependency hits come from `RequireFeatureIndex` only.
pub fn resolve_require_path(
    kind: RequireKind,
    argument: &str,
    current_file: &Path,
    project_root: &Path,
    load_paths: &[String],
    feature_index: &RequireFeatureIndex,
    view: Option<&View<'_>>,
) -> Option<PathBuf> {
    if argument.is_empty() {
        return None;
    }

    match kind {
        RequireKind::RequireRelative => {
            let parent = current_file.parent()?;
            existing_require_candidate(&parent.join(argument), view)
        }
        RequireKind::Require => {
            for configured in load_paths {
                if let Some(root) = validated_project_relative_dir(project_root, configured) {
                    if let Some(resolved) = existing_require_candidate(&root.join(argument), view) {
                        return Some(resolved);
                    }
                }
            }
            if let Some(resolved) =
                existing_require_candidate(&project_root.join("lib").join(argument), view)
            {
                return Some(resolved);
            }
            if let Some(resolved) = existing_require_candidate(&project_root.join(argument), view) {
                return Some(resolved);
            }
            feature_index.lookup(argument).map(Path::to_path_buf)
        }
    }
}

/// Build a goto location that selects the entire target file contents.
pub fn location_for_require_target(path: &Path, view: Option<&View<'_>>) -> Option<Location> {
    let uri = Url::from_file_path(path).ok()?;
    let range = require_target_full_range(path, view)
        .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
    Some(Location { uri, range })
}

fn require_target_full_range(path: &Path, view: Option<&View<'_>>) -> Option<Range> {
    if let Some(file) = view.and_then(|view| view.file(view.file_id(path)?)) {
        let (line, character) = file.end_position();
        return Some(Range::new(
            Position::new(0, 0),
            Position::new(line, character),
        ));
    }
    std::fs::read_to_string(path)
        .ok()
        .map(|content| full_document_range(&content))
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

fn existing_require_candidate(candidate: &Path, view: Option<&View<'_>>) -> Option<PathBuf> {
    for path in [candidate.to_path_buf(), with_rb_extension(candidate)] {
        if path.is_file() {
            return Some(path);
        }
        if let Some(view) = view {
            if view.file_id(&path).is_some() {
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
mod tests;
