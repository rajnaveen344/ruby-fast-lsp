//! Ruby source rendering with exact token/source mapping for modeled sites.

mod calls;
mod methods;
mod namespaces;
mod oracle_support;
mod source_text;
mod type_flow;

use super::graph::{self, CallShape, ConstantRefShape, MethodTarget, NamespaceKind, NamespaceSpec};
use super::project::SyntheticProject;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct ProjectRender {
    pub files: BTreeMap<String, String>,
    pub map: SourceMap,
}

#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    pub files: BTreeSet<String>,
    pub namespaces: HashMap<String, NamespaceDefSite>,
    pub defs: HashMap<MethodTarget, SourcePos>,
    pub constants: HashMap<String, SourcePos>,
    pub calls: Vec<CallSite>,
    pub constant_refs: Vec<ConstantRefSite>,
    pub include_refs: Vec<NamespaceRefSite>,
    pub superclass_refs: Vec<NamespaceRefSite>,
    pub type_asserts: Vec<TypeAssertSite>,
    pub direct_macro_calls: Vec<DirectMacroCallSite>,
}

#[derive(Debug, Clone)]
pub struct CallSite {
    pub caller: MethodTarget,
    pub target: MethodTarget,
    pub shape: CallShape,
    pub pos: SourcePos,
    pub shape_name: &'static str,
    pub definition_support: OracleSupport,
    pub reference_support: OracleSupport,
    pub hover_support: OracleSupport,
}

#[derive(Debug, Clone)]
pub struct ConstantRefSite {
    pub caller: MethodTarget,
    /// Ruby lexical nesting is independent of a dynamically selected method owner.
    pub lexical_scope: String,
    pub target: String,
    pub text: String,
    pub shape: ConstantRefShape,
    pub pos: SourcePos,
}

#[derive(Debug, Clone)]
pub struct NamespaceDefSite {
    pub kind: NamespaceKind,
    pub pos: SourcePos,
}

#[derive(Debug, Clone)]
pub struct NamespaceRefSite {
    pub owner: String,
    pub target: String,
    pub pos: SourcePos,
    pub support: OracleSupport,
}

#[derive(Debug, Clone)]
pub struct TypeAssertSite {
    pub owner: MethodTarget,
    pub expected: String,
    pub pos: SourcePos,
    pub kind: TypeAssertKind,
}

#[derive(Debug, Clone)]
pub struct DirectMacroCallSite {
    pub name: String,
    pub pos: SourcePos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeAssertKind {
    LocalAssignment,
    MethodReturnHint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleSupport {
    Supported,
    KnownGap(&'static str),
}

pub(crate) const UNPROVEN_BLOCK_RECEIVER_GAP: &str =
    "implicit receiver calls inside blocks require a proven execution contract";

impl OracleSupport {
    pub fn is_supported(self) -> bool {
        matches!(self, OracleSupport::Supported)
    }

    pub fn gap_reason(self) -> Option<&'static str> {
        match self {
            OracleSupport::Supported => None,
            OracleSupport::KnownGap(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePos {
    pub file: String,
    pub line: u32,
    pub character: u32,
}

pub fn render_project(project: &SyntheticProject) -> ProjectRender {
    let mut files = BTreeMap::new();
    let mut map = SourceMap::default();
    let inline_concern_namespaces = project
        .namespaces
        .iter()
        .flat_map(|namespace| namespace.concern_class_methods.iter())
        .filter(|class_methods| class_methods.enabled)
        .map(|class_methods| class_methods.fqn.clone())
        .collect::<HashSet<_>>();

    for namespace in &project.namespaces {
        if inline_concern_namespaces.contains(&namespace.fqn) {
            continue;
        }
        let file = namespace_file(namespace);
        let mut renderer = FileRenderer::new(file.clone());
        renderer.render_namespace(namespace, project);
        map.namespaces.extend(renderer.map.namespaces);
        map.defs.extend(renderer.map.defs);
        map.constants.extend(renderer.map.constants);
        map.calls.extend(renderer.map.calls);
        map.constant_refs.extend(renderer.map.constant_refs);
        map.include_refs.extend(renderer.map.include_refs);
        map.superclass_refs.extend(renderer.map.superclass_refs);
        map.type_asserts.extend(renderer.map.type_asserts);
        map.direct_macro_calls
            .extend(renderer.map.direct_macro_calls);
        map.files.insert(file.clone());
        files.insert(file, renderer.code);
    }

    for (file, content) in &project.raw_files {
        assert!(
            !files.contains_key(file),
            "INVARIANT VIOLATED: raw simulation file `{}` collides with a generated namespace file. This is a bug because each simulated file path must have one source. Fix: rename the raw file or namespace.",
            file
        );
        map.files.insert(file.clone());
        files.insert(file.clone(), content.clone());
    }

    ProjectRender { files, map }
}

pub(crate) fn namespace_file(namespace: &NamespaceSpec) -> String {
    namespace
        .file_path
        .clone()
        .unwrap_or_else(|| file_for_namespace(&namespace.fqn))
}

pub(crate) fn file_for_namespace(fqn: &str) -> String {
    let path = fqn
        .split("::")
        .map(underscore)
        .collect::<Vec<_>>()
        .join("/");
    format!("{}.rb", path)
}

fn underscore(input: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if ch.is_uppercase() && idx > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

struct FileRenderer {
    file: String,
    code: String,
    line: u32,
    map: SourceMap,
}

impl FileRenderer {
    fn new(file: String) -> Self {
        Self {
            file,
            code: String::new(),
            line: 0,
            map: SourceMap::default(),
        }
    }

    fn push_line(&mut self, depth: usize, text: &str) {
        self.code.push_str(&"  ".repeat(depth));
        self.code.push_str(text);
        self.code.push('\n');
        self.line += 1;
    }
}
