use super::graph::{
    AliasForm, CallShape, ConstantRefShape, ConstantSpec, DelegateForm, DelegateSpec,
    MethodDefForm, MethodKind, MethodSpec, MethodTarget, MethodVisibility, MethodVisibilitySyntax,
    NamespaceKind, NamespaceSpec,
};
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

    fn render_namespace(&mut self, namespace: &NamespaceSpec, project: &SyntheticProject) {
        if !namespace.enabled {
            return;
        }

        let parts = namespace.fqn.split("::").collect::<Vec<_>>();
        assert!(
            !parts.is_empty(),
            "INVARIANT VIOLATED: namespace `{}` has no path parts. This is a bug because Ruby namespaces need at least one constant. Fix: pass a non-empty FQN.",
            namespace.fqn
        );

        for (depth, part) in parts.iter().take(parts.len() - 1).enumerate() {
            self.push_line(depth, &format!("module {}", part));
        }

        let leaf_depth = parts.len() - 1;
        let leaf = parts
            .last()
            .expect("INVARIANT VIOLATED: namespace leaf missing. This is a bug because parts was asserted non-empty. Fix: inspect FileRenderer::render_namespace.");
        match namespace.kind {
            NamespaceKind::Class => {
                let superclass = namespace
                    .superclass
                    .as_ref()
                    .map(|fqn| format!(" < {}", fqn))
                    .unwrap_or_default();
                self.map.namespaces.insert(
                    namespace.fqn.clone(),
                    NamespaceDefSite {
                        kind: namespace.kind,
                        pos: SourcePos {
                            file: self.file.clone(),
                            line: self.line,
                            character: (indent_len(leaf_depth) + "class ".len()) as u32,
                        },
                    },
                );
                if let Some(superclass) = &namespace.superclass {
                    self.map.superclass_refs.push(NamespaceRefSite {
                        owner: namespace.fqn.clone(),
                        target: superclass.clone(),
                        pos: SourcePos {
                            file: self.file.clone(),
                            line: self.line,
                            character: (indent_len(leaf_depth)
                                + "class ".len()
                                + leaf.len()
                                + " < ".len()
                                + leaf_segment_offset(superclass))
                                as u32,
                        },
                        support: OracleSupport::Supported,
                    });
                }
                self.push_line(leaf_depth, &format!("class {}{}", leaf, superclass));
            }
            NamespaceKind::Module => {
                self.map.namespaces.insert(
                    namespace.fqn.clone(),
                    NamespaceDefSite {
                        kind: namespace.kind,
                        pos: SourcePos {
                            file: self.file.clone(),
                            line: self.line,
                            character: (indent_len(leaf_depth) + "module ".len()) as u32,
                        },
                    },
                );
                self.push_line(leaf_depth, &format!("module {}", leaf));
            }
        }

        for prepend in namespace.prepends.iter().filter(|prepend| prepend.enabled) {
            self.record_direct_macro_call(leaf_depth + 1, "prepend");
            self.map.include_refs.push(NamespaceRefSite {
                owner: namespace.fqn.clone(),
                target: prepend.fqn.clone(),
                pos: SourcePos {
                    file: self.file.clone(),
                    line: self.line,
                    character: (indent_len(leaf_depth + 1)
                        + "prepend ".len()
                        + leaf_segment_offset(&prepend.fqn)) as u32,
                },
                support: OracleSupport::Supported,
            });
            self.push_line(leaf_depth + 1, &format!("prepend {}", prepend.fqn));
        }

        for include in namespace.includes.iter().filter(|include| include.enabled) {
            self.record_direct_macro_call(leaf_depth + 1, "include");
            self.map.include_refs.push(NamespaceRefSite {
                owner: namespace.fqn.clone(),
                target: include.fqn.clone(),
                pos: SourcePos {
                    file: self.file.clone(),
                    line: self.line,
                    character: (indent_len(leaf_depth + 1)
                        + "include ".len()
                        + leaf_segment_offset(&include.fqn)) as u32,
                },
                support: OracleSupport::Supported,
            });
            self.push_line(leaf_depth + 1, &format!("include {}", include.fqn));
        }

        for extend in namespace.extends.iter().filter(|extend| extend.enabled) {
            self.record_direct_macro_call(leaf_depth + 1, "extend");
            self.map.include_refs.push(NamespaceRefSite {
                owner: namespace.fqn.clone(),
                target: extend.fqn.clone(),
                pos: SourcePos {
                    file: self.file.clone(),
                    line: self.line,
                    character: (indent_len(leaf_depth + 1)
                        + "extend ".len()
                        + leaf_segment_offset(&extend.fqn)) as u32,
                },
                support: OracleSupport::Supported,
            });
            self.push_line(leaf_depth + 1, &format!("extend {}", extend.fqn));
        }

        if namespace.extend_self {
            self.record_direct_macro_call(leaf_depth + 1, "extend");
            self.push_line(leaf_depth + 1, "extend self");
        }

        if namespace
            .concern_class_methods
            .iter()
            .any(|class_methods| class_methods.enabled)
        {
            self.push_line(leaf_depth + 1, "extend ActiveSupport::Concern");
        }

        if namespace
            .included_hook_extends
            .iter()
            .any(|extend| extend.enabled)
            || namespace
                .included_hook_includes
                .iter()
                .any(|include| include.enabled)
            || namespace
                .included_hook_class_eval_includes
                .iter()
                .any(|include| include.enabled)
        {
            self.push_line(leaf_depth + 1, "def self.included(base)");
            for extend in namespace
                .included_hook_extends
                .iter()
                .filter(|extend| extend.enabled)
            {
                self.map.include_refs.push(NamespaceRefSite {
                    owner: namespace.fqn.clone(),
                    target: extend.fqn.clone(),
                    pos: SourcePos {
                        file: self.file.clone(),
                        line: self.line,
                        character: (indent_len(leaf_depth + 2)
                            + "base.extend(".len()
                            + leaf_segment_offset(&extend.fqn))
                            as u32,
                    },
                    support: OracleSupport::Supported,
                });
                self.push_line(leaf_depth + 2, &format!("base.extend({})", extend.fqn));
            }
            for include in namespace
                .included_hook_includes
                .iter()
                .filter(|include| include.enabled)
            {
                self.map.include_refs.push(NamespaceRefSite {
                    owner: namespace.fqn.clone(),
                    target: include.fqn.clone(),
                    pos: SourcePos {
                        file: self.file.clone(),
                        line: self.line,
                        character: (indent_len(leaf_depth + 2)
                            + "base.send :include, ".len()
                            + leaf_segment_offset(&include.fqn))
                            as u32,
                    },
                    support: OracleSupport::Supported,
                });
                self.push_line(
                    leaf_depth + 2,
                    &format!("base.send :include, {}", include.fqn),
                );
            }
            if namespace
                .included_hook_class_eval_includes
                .iter()
                .any(|include| include.enabled)
            {
                self.push_line(leaf_depth + 2, "base.class_eval do");
                for include in namespace
                    .included_hook_class_eval_includes
                    .iter()
                    .filter(|include| include.enabled)
                {
                    self.record_direct_macro_call(leaf_depth + 3, "include");
                    self.map.include_refs.push(NamespaceRefSite {
                        owner: namespace.fqn.clone(),
                        target: include.fqn.clone(),
                        pos: SourcePos {
                            file: self.file.clone(),
                            line: self.line,
                            character: (indent_len(leaf_depth + 3)
                                + "include ".len()
                                + leaf_segment_offset(&include.fqn))
                                as u32,
                        },
                        support: OracleSupport::Supported,
                    });
                    self.push_line(leaf_depth + 3, &format!("include {}", include.fqn));
                }
                self.push_line(leaf_depth + 2, "end");
            }
            self.push_line(leaf_depth + 1, "end");
        }

        if namespace
            .singleton_prepends
            .iter()
            .any(|prepend| prepend.enabled)
            || namespace
                .singleton_includes
                .iter()
                .any(|include| include.enabled)
        {
            self.push_line(leaf_depth + 1, "class << self");
            for prepend in namespace
                .singleton_prepends
                .iter()
                .filter(|prepend| prepend.enabled)
            {
                self.record_direct_macro_call(leaf_depth + 2, "prepend");
                self.map.include_refs.push(NamespaceRefSite {
                    owner: namespace.fqn.clone(),
                    target: prepend.fqn.clone(),
                    pos: SourcePos {
                        file: self.file.clone(),
                        line: self.line,
                        character: (indent_len(leaf_depth + 2)
                            + "prepend ".len()
                            + leaf_segment_offset(&prepend.fqn))
                            as u32,
                    },
                    support: OracleSupport::Supported,
                });
                self.push_line(leaf_depth + 2, &format!("prepend {}", prepend.fqn));
            }
            for include in namespace
                .singleton_includes
                .iter()
                .filter(|include| include.enabled)
            {
                self.record_direct_macro_call(leaf_depth + 2, "include");
                self.map.include_refs.push(NamespaceRefSite {
                    owner: namespace.fqn.clone(),
                    target: include.fqn.clone(),
                    pos: SourcePos {
                        file: self.file.clone(),
                        line: self.line,
                        character: (indent_len(leaf_depth + 2)
                            + "include ".len()
                            + leaf_segment_offset(&include.fqn))
                            as u32,
                    },
                    support: OracleSupport::Supported,
                });
                self.push_line(leaf_depth + 2, &format!("include {}", include.fqn));
            }
            self.push_line(leaf_depth + 1, "end");
        }

        if namespace.prepends.iter().any(|prepend| prepend.enabled)
            || namespace.includes.iter().any(|include| include.enabled)
            || namespace.extends.iter().any(|extend| extend.enabled)
            || namespace.extend_self
            || namespace
                .included_hook_extends
                .iter()
                .any(|extend| extend.enabled)
            || namespace
                .included_hook_includes
                .iter()
                .any(|include| include.enabled)
            || namespace
                .included_hook_class_eval_includes
                .iter()
                .any(|include| include.enabled)
            || namespace
                .concern_class_methods
                .iter()
                .any(|class_methods| class_methods.enabled)
            || namespace
                .visibility_overrides
                .iter()
                .any(|visibility_override| visibility_override.enabled)
            || namespace
                .singleton_prepends
                .iter()
                .any(|prepend| prepend.enabled)
            || namespace
                .singleton_includes
                .iter()
                .any(|include| include.enabled)
        {
            self.push_line(leaf_depth + 1, "");
        }

        for constant in namespace
            .constants
            .iter()
            .filter(|constant| constant.enabled)
        {
            self.render_constant(namespace, constant, leaf_depth + 1);
        }

        if namespace.constants.iter().any(|constant| constant.enabled)
            && namespace.methods.iter().any(|method| method.enabled)
        {
            self.push_line(leaf_depth + 1, "");
        }

        for method in namespace.methods.iter().filter(|method| {
            method.enabled
                && !matches!(
                    method.def_form,
                    MethodDefForm::ClassEvalBlock | MethodDefForm::ConstGetDefineMethod
                )
        }) {
            self.render_method(namespace, method, leaf_depth + 1);
        }

        for visibility_override in namespace
            .visibility_overrides
            .iter()
            .filter(|visibility_override| visibility_override.enabled)
        {
            self.record_direct_macro_call(leaf_depth + 1, visibility_override.visibility.keyword());
            self.push_line(
                leaf_depth + 1,
                &format!(
                    "{} :{}",
                    visibility_override.visibility.keyword(),
                    visibility_override.name
                ),
            );
        }

        if namespace
            .visibility_overrides
            .iter()
            .any(|visibility_override| visibility_override.enabled)
        {
            self.push_line(leaf_depth + 1, "");
        }

        for class_methods in namespace
            .concern_class_methods
            .iter()
            .filter(|class_methods| class_methods.enabled)
        {
            let Some(class_methods_namespace) = project
                .namespaces
                .iter()
                .find(|candidate| candidate.enabled && candidate.fqn == class_methods.fqn)
            else {
                continue;
            };
            self.push_line(leaf_depth + 1, "");
            self.push_line(leaf_depth + 1, "class_methods do");
            for method in class_methods_namespace.methods.iter().filter(|method| {
                method.enabled
                    && method.kind == MethodKind::Instance
                    && !matches!(
                        method.def_form,
                        MethodDefForm::ClassEvalBlock | MethodDefForm::ConstGetDefineMethod
                    )
            }) {
                self.render_method(class_methods_namespace, method, leaf_depth + 2);
            }
            self.push_line(leaf_depth + 1, "end");
        }

        for alias in namespace.aliases.iter().filter(|alias| alias.enabled) {
            self.render_alias(namespace, alias, leaf_depth + 1);
        }

        for delegate in namespace
            .delegates
            .iter()
            .filter(|delegate| delegate.enabled)
        {
            self.render_delegate(namespace, delegate, leaf_depth + 1);
        }

        for attribute in namespace
            .class_attributes
            .iter()
            .filter(|attribute| attribute.enabled)
        {
            self.render_class_attribute(namespace, attribute, leaf_depth + 1);
        }

        for depth in (0..=leaf_depth).rev() {
            self.push_line(depth, "end");
        }

        for method in namespace
            .methods
            .iter()
            .filter(|method| method.enabled && method.def_form == MethodDefForm::ClassEvalBlock)
        {
            self.render_class_eval_method(namespace, method);
        }
        for method in namespace.methods.iter().filter(|method| {
            method.enabled && method.def_form == MethodDefForm::ConstGetDefineMethod
        }) {
            self.render_const_get_define_method(namespace, method);
        }
    }

    fn render_constant(
        &mut self,
        namespace: &NamespaceSpec,
        constant: &ConstantSpec,
        depth: usize,
    ) {
        let fqn = format!("{}::{}", namespace.fqn, constant.name);
        self.map.constants.insert(
            fqn,
            SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: indent_len(depth) as u32,
            },
        );
        self.push_line(depth, &format!("{} = {}", constant.name, constant.value));
    }

    fn record_direct_macro_call(&mut self, depth: usize, name: &str) {
        self.map.direct_macro_calls.push(DirectMacroCallSite {
            name: name.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: indent_len(depth) as u32,
            },
        });
    }

    fn render_method(&mut self, namespace: &NamespaceSpec, method: &MethodSpec, depth: usize) {
        let scoped_visibility = method.visibility != MethodVisibility::Public;
        if scoped_visibility && method.visibility_syntax == MethodVisibilitySyntax::ScopeKeyword {
            assert!(
                method.kind == MethodKind::Instance
                    && matches!(
                        method.def_form,
                        MethodDefForm::Regular | MethodDefForm::DefineMethod
                    ),
                "INVARIANT VIOLATED: simulator method `{}` in `{}` requested non-public visibility for unsupported def form/kind. This is a bug because Ruby visibility syntax differs for singleton/dynamic methods. Fix: only mark regular instance methods private/protected until the simulator models those forms.",
                method.name,
                namespace.fqn
            );
            self.record_direct_macro_call(depth, method.visibility.keyword());
            self.push_line(depth, method.visibility.keyword());
        }

        if method.block_type_asserts {
            self.render_block_type_helper(method, depth);
        }

        match (method.kind, method.def_form) {
            (MethodKind::Instance, MethodDefForm::DefineMethod) => {
                self.render_define_method_body(namespace, method, depth);
            }
            (MethodKind::Class, MethodDefForm::DefineMethod) => {
                self.push_line(depth, "class << self");
                self.render_define_method_body(namespace, method, depth + 1);
                self.push_line(depth, "end");
                self.push_line(depth, "");
            }
            (MethodKind::Instance | MethodKind::Class, MethodDefForm::ClassEvalBlock) => {
                panic!(
                    "INVARIANT VIOLATED: method `{}` in `{}` requested class_eval rendering through render_method. This is a bug because class_eval methods must be rendered after the class/module closes. Fix: call render_class_eval_method.",
                    method.name, namespace.fqn
                );
            }
            (MethodKind::Instance | MethodKind::Class, MethodDefForm::ConstGetDefineMethod) => {
                panic!(
                    "INVARIANT VIOLATED: method `{}` in `{}` requested const_get define_method rendering through render_method. This is a bug because const_get define_method methods must be rendered after the class/module closes. Fix: call render_const_get_define_method.",
                    method.name, namespace.fqn
                );
            }
            (MethodKind::Instance, MethodDefForm::ModuleFunctionMode) => {
                assert!(
                    namespace.kind == super::graph::NamespaceKind::Module,
                    "INVARIANT VIOLATED: method `{}` in `{}` requested module_function mode outside a module. This is a bug because bare module_function is only meaningful for modules in the simulator. Fix: call in_module_function_mode only on module methods.",
                    method.name,
                    namespace.fqn
                );
                self.record_direct_macro_call(depth, "module_function");
                self.push_line(depth, "module_function");
                self.push_line(depth, "");
                self.render_method_body(namespace, method, depth, "def ");
            }
            (MethodKind::Class, MethodDefForm::ModuleFunctionMode) => {
                panic!(
                    "INVARIANT VIOLATED: class method `{}` in `{}` requested module_function mode. This is a bug because bare module_function duplicates instance methods as singleton methods. Fix: use method(), not class_method().",
                    method.name, namespace.fqn
                );
            }
            (MethodKind::Class, MethodDefForm::SingletonClassBlock) => {
                self.push_line(depth, "class << self");
                self.render_method_body(namespace, method, depth + 1, "def ");
                self.push_line(depth, "end");
                self.push_line(depth, "");
            }
            (MethodKind::Instance, MethodDefForm::SingletonClassBlock) => {
                panic!(
                    "INVARIANT VIOLATED: instance method `{}` in `{}` requested class << self rendering. This is a bug because class << self produces singleton methods. Fix: only set SingletonClassBlock on class methods.",
                    method.name, namespace.fqn
                );
            }
            (MethodKind::Instance, MethodDefForm::Regular) => {
                self.render_method_body(namespace, method, depth, "def ");
            }
            (MethodKind::Class, MethodDefForm::Regular) => {
                self.render_method_body(namespace, method, depth, "def self.");
            }
        }

        if scoped_visibility && method.visibility_syntax == MethodVisibilitySyntax::ArgumentList {
            assert!(
                method.kind == MethodKind::Instance
                    && matches!(
                        method.def_form,
                        MethodDefForm::Regular | MethodDefForm::DefineMethod
                    ),
                "INVARIANT VIOLATED: simulator method `{}` in `{}` requested argument-list visibility for unsupported def form/kind. This is a bug because Ruby visibility argument syntax differs for singleton/dynamic methods. Fix: only mark regular/define instance methods private/protected until the simulator models those forms.",
                method.name,
                namespace.fqn
            );
            self.record_direct_macro_call(depth, method.visibility.keyword());
            self.push_line(
                depth,
                &format!("{} :{}", method.visibility.keyword(), method.name),
            );
            self.push_line(depth, "");
        }

        if scoped_visibility && method.visibility_syntax == MethodVisibilitySyntax::ScopeKeyword {
            self.record_direct_macro_call(depth, MethodVisibility::Public.keyword());
            self.push_line(depth, MethodVisibility::Public.keyword());
            self.push_line(depth, "");
        }
    }

    fn render_class_eval_method(&mut self, namespace: &NamespaceSpec, method: &MethodSpec) {
        self.push_line(0, "");
        self.push_line(0, &format!("{}.class_eval do", namespace.fqn));
        let prefix = match method.kind {
            MethodKind::Instance => "def ",
            MethodKind::Class => "def self.",
        };
        self.render_method_body(namespace, method, 1, prefix);
        self.push_line(0, "end");
    }

    fn render_define_method_body(
        &mut self,
        namespace: &NamespaceSpec,
        method: &MethodSpec,
        depth: usize,
    ) {
        if let Some(return_type) = &method.return_type {
            self.push_line(depth, &format!("# @return [{}]", return_type));
        }

        let def_line = format!("define_method(:{}) do", method.name);
        let char_offset = indent_len(depth) + "define_method(:".len();
        self.record_direct_macro_call(depth, "define_method");
        self.map.defs.insert(
            MethodTarget {
                owner: namespace.fqn.clone(),
                name: method.name.clone(),
                kind: method.kind,
            },
            SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: char_offset as u32,
            },
        );
        self.push_line(depth, &def_line);

        let caller = MethodTarget {
            owner: namespace.fqn.clone(),
            name: method.name.clone(),
            kind: method.kind,
        };
        for constant in &method.constant_refs {
            self.render_constant_ref(namespace, &caller, constant, depth + 1);
        }
        for call in &method.calls {
            self.render_call(&caller, &call.target, &call.shape, depth + 1);
        }

        if let Some(return_type) = &method.return_type {
            self.render_return_assignment(
                &caller,
                return_type,
                depth + 1,
                method.block_type_asserts,
            );
        } else if method.calls.is_empty() && method.constant_refs.is_empty() {
            self.push_line(depth + 1, "nil");
        }

        self.push_line(depth, "end");
        self.push_line(depth, "");
    }

    fn render_const_get_define_method(&mut self, namespace: &NamespaceSpec, method: &MethodSpec) {
        self.push_line(0, "");
        if let Some(return_type) = &method.return_type {
            self.push_line(0, &format!("# @return [{}]", return_type));
        }
        let (receiver, leaf) = const_get_receiver(&namespace.fqn);
        let def_line = format!(
            "{receiver}.const_get(:{leaf}).send(:define_method, :{}) do",
            method.name
        );
        let char_offset = format!("{receiver}.const_get(:{leaf}).send(:define_method, :").len();
        self.map.defs.insert(
            MethodTarget {
                owner: namespace.fqn.clone(),
                name: method.name.clone(),
                kind: method.kind,
            },
            SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: char_offset as u32,
            },
        );
        self.push_line(0, &def_line);

        let caller = MethodTarget {
            owner: namespace.fqn.clone(),
            name: method.name.clone(),
            kind: method.kind,
        };
        for constant in &method.constant_refs {
            self.render_constant_ref(namespace, &caller, constant, 1);
        }
        for call in &method.calls {
            self.render_call(&caller, &call.target, &call.shape, 1);
        }

        if let Some(return_type) = &method.return_type {
            self.render_return_assignment(&caller, return_type, 1, method.block_type_asserts);
        } else if method.calls.is_empty() && method.constant_refs.is_empty() {
            self.push_line(1, "nil");
        }

        self.push_line(0, "end");
    }

    fn render_method_body(
        &mut self,
        namespace: &NamespaceSpec,
        method: &MethodSpec,
        depth: usize,
        prefix: &str,
    ) {
        if let Some(return_type) = &method.return_type {
            self.push_line(depth, &format!("# @return [{}]", return_type));
        }

        let def_line = format!("{}{}", prefix, method.name);
        let char_offset = indent_len(depth) + prefix.len();
        self.map.defs.insert(
            MethodTarget {
                owner: namespace.fqn.clone(),
                name: method.name.clone(),
                kind: method.kind,
            },
            SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: char_offset as u32,
            },
        );
        let caller = MethodTarget {
            owner: namespace.fqn.clone(),
            name: method.name.clone(),
            kind: method.kind,
        };
        if let Some(return_type) = &method.return_type {
            self.map.type_asserts.push(TypeAssertSite {
                owner: caller.clone(),
                expected: return_type.clone(),
                pos: SourcePos {
                    file: self.file.clone(),
                    line: self.line,
                    character: (char_offset + method.name.len()) as u32,
                },
                kind: TypeAssertKind::MethodReturnHint,
            });
        }
        self.push_line(depth, &def_line);

        for constant in &method.constant_refs {
            self.render_constant_ref(namespace, &caller, constant, depth + 1);
        }
        for call in &method.calls {
            self.render_call(&caller, &call.target, &call.shape, depth + 1);
        }

        if let Some(return_type) = &method.return_type {
            self.render_return_assignment(
                &caller,
                return_type,
                depth + 1,
                method.block_type_asserts,
            );
        } else if method.calls.is_empty() && method.constant_refs.is_empty() {
            self.push_line(depth + 1, "nil");
        }

        self.push_line(depth, "end");
        self.push_line(depth, "");
    }

    fn render_alias(
        &mut self,
        namespace: &NamespaceSpec,
        alias: &super::graph::AliasSpec,
        depth: usize,
    ) {
        let target = MethodTarget {
            owner: namespace.fqn.clone(),
            name: alias.new_name.clone(),
            kind: alias.kind,
        };
        let (line, character) = match alias.form {
            AliasForm::Keyword => (
                format!("alias {} {}", alias.new_name, alias.old_name),
                indent_len(depth) + "alias ".len(),
            ),
            AliasForm::MethodCall => (
                format!("alias_method :{}, :{}", alias.new_name, alias.old_name),
                indent_len(depth) + "alias_method :".len(),
            ),
        };
        if alias.form == AliasForm::MethodCall {
            self.record_direct_macro_call(depth, "alias_method");
        }
        self.map.defs.insert(
            target,
            SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: character as u32,
            },
        );
        self.push_line(depth, &line);
        self.push_line(depth, "");
    }

    fn render_delegate(
        &mut self,
        namespace: &NamespaceSpec,
        delegate: &DelegateSpec,
        depth: usize,
    ) {
        let target = MethodTarget {
            owner: namespace.fqn.clone(),
            name: delegate.new_name.clone(),
            kind: delegate.kind,
        };
        match (delegate.kind, delegate.form) {
            (MethodKind::Instance, DelegateForm::Rails) => {
                self.record_direct_macro_call(depth, "delegate");
                self.map.defs.insert(
                    target,
                    SourcePos {
                        file: self.file.clone(),
                        line: self.line,
                        character: (indent_len(depth) + "delegate :".len()) as u32,
                    },
                );
                self.push_line(
                    depth,
                    &format!(
                        "delegate :{}, to: :{}",
                        delegate.new_name, delegate.receiver_method
                    ),
                );
                self.push_line(depth, "");
            }
            (MethodKind::Class, DelegateForm::ForwardableSingular)
            | (MethodKind::Class, DelegateForm::ForwardablePlural) => {
                self.push_line(depth, "class << self");
                let inner_depth = depth + 1;
                let (line, character) = match delegate.form {
                    DelegateForm::ForwardableSingular => (
                        format!(
                            "def_delegator :{}, :{}",
                            delegate.receiver_method, delegate.new_name
                        ),
                        indent_len(inner_depth)
                            + "def_delegator :".len()
                            + delegate.receiver_method.len()
                            + ", :".len(),
                    ),
                    DelegateForm::ForwardablePlural => (
                        format!(
                            "def_delegators :{}, :{}",
                            delegate.receiver_method, delegate.new_name
                        ),
                        indent_len(inner_depth)
                            + "def_delegators :".len()
                            + delegate.receiver_method.len()
                            + ", :".len(),
                    ),
                    DelegateForm::Rails => panic!(
                        "INVARIANT VIOLATED: Rails delegate rendered inside Forwardable branch. This is a bug because delegate form dispatch must be exhaustive. Fix: keep render_delegate form match aligned."
                    ),
                };
                self.record_direct_macro_call(
                    inner_depth,
                    match delegate.form {
                        DelegateForm::ForwardableSingular => "def_delegator",
                        DelegateForm::ForwardablePlural => "def_delegators",
                        DelegateForm::Rails => panic!(
                            "INVARIANT VIOLATED: Rails delegate reached Forwardable macro recording. This is a bug because delegate form dispatch must be exhaustive. Fix: keep render_delegate form match aligned."
                        ),
                    },
                );
                self.map.defs.insert(
                    target,
                    SourcePos {
                        file: self.file.clone(),
                        line: self.line,
                        character: character as u32,
                    },
                );
                self.push_line(inner_depth, &line);
                self.push_line(depth, "end");
                self.push_line(depth, "");
            }
            (MethodKind::Instance, DelegateForm::ForwardableSingular)
            | (MethodKind::Instance, DelegateForm::ForwardablePlural)
            | (MethodKind::Class, DelegateForm::Rails) => {
                panic!(
                    "INVARIANT VIOLATED: delegate `{}` in `{}` requested unsupported form/kind combination. This is a bug because simulator delegates must render valid Ruby. Fix: add a supported renderer or adjust project builder.",
                    delegate.new_name,
                    namespace.fqn
                );
            }
        }
    }

    fn render_class_attribute(
        &mut self,
        namespace: &NamespaceSpec,
        attribute: &super::graph::ClassAttributeSpec,
        depth: usize,
    ) {
        let pos = SourcePos {
            file: self.file.clone(),
            line: self.line,
            character: (indent_len(depth) + "class_attribute :".len()) as u32,
        };
        self.record_direct_macro_call(depth, "class_attribute");
        self.map.defs.insert(
            MethodTarget {
                owner: namespace.fqn.clone(),
                name: attribute.name.clone(),
                kind: MethodKind::Class,
            },
            pos.clone(),
        );
        self.map.defs.insert(
            MethodTarget {
                owner: namespace.fqn.clone(),
                name: attribute.name.clone(),
                kind: MethodKind::Instance,
            },
            pos,
        );
        self.push_line(depth, &format!("class_attribute :{}", attribute.name));
        self.push_line(depth, "");
    }

    fn render_return_assignment(
        &mut self,
        caller: &MethodTarget,
        return_type: &str,
        depth: usize,
        block_type_asserts: bool,
    ) {
        let slug = method_slug(&caller.name);
        let expression = return_expression(return_type);
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_if_{}", slug),
            &format!("if true then {} else {} end", expression, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_unless_{}", slug),
            &format!("unless false then {} else {} end", expression, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_case_{}", slug),
            &format!(
                "case :sim when :sim then {} else {} end",
                expression, expression
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_rescue_{}", slug),
            &format!(
                "begin {}; rescue StandardError; {}; ensure nil; end",
                expression, expression
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_rescue_modifier_{}", slug),
            &format!("{} rescue {}", expression, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_return_{}", slug),
            &expression,
        );

        if block_type_asserts {
            self.render_block_type_assignments(caller, return_type, depth, &slug, &expression);
        }
    }

    fn render_block_type_assignments(
        &mut self,
        caller: &MethodTarget,
        return_type: &str,
        depth: usize,
        slug: &str,
        expression: &str,
    ) {
        let array_local = format!("__sim_array_value_{}", slug);
        self.push_line(
            depth,
            &format!("[{}].each do |{}|", expression, array_local),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_array_copy_{}", slug),
            &array_local,
        );
        self.push_line(depth, "end");

        self.push_line(depth, &format!("[{}].each do", expression));
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_array_numbered_copy_{}", slug),
            "_1",
        );
        self.push_line(depth, "end");

        let pattern_local = format!("__sim_pattern_value_{}", slug);
        self.push_line(depth, &format!("case {{value: {}}}", expression));
        self.push_line(depth, &format!("in {{value: {}}}", pattern_local));
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_pattern_copy_{}", slug),
            &pattern_local,
        );
        self.push_line(depth, "end");

        self.push_line(
            depth,
            &format!("__sim_lambda_builder_{} = -> {{ {} }}", slug, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_lambda_result_{}", slug),
            &format!("__sim_lambda_builder_{}.call", slug),
        );

        self.push_line(
            depth,
            &format!(
                "__sim_proc_builder_{} = Proc.new {{ {} }}",
                slug, expression
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_proc_result_{}", slug),
            &format!("__sim_proc_builder_{}.call", slug),
        );

        let helper = format!("__sim_with_value_{}", slug);

        let local = format!("__sim_yield_value_{}", slug);
        let result = format!("__sim_yield_result_{}", slug);
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + result.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(depth, &format!("{} = {} do |{}|", result, helper, local));
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_block_copy_{}", slug),
            &local,
        );
        self.push_line(depth + 1, &local);
        self.push_line(depth, "end");

        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_yield_numbered_result_{}", slug),
            &format!("{} {{ _1 }}", helper),
        );

        let forward_helper = format!("__sim_forward_value_{}", slug);
        let forward_local = format!("__sim_forward_yield_value_{}", slug);
        let forward_result = format!("__sim_forward_yield_result_{}", slug);
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + forward_result.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(
            depth,
            &format!(
                "{} = {} do |{}|",
                forward_result, forward_helper, forward_local
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_forward_block_copy_{}", slug),
            &forward_local,
        );
        self.push_line(depth + 1, &forward_local);
        self.push_line(depth, "end");

        let dot_forward_helper = format!("__sim_dot_forward_value_{}", slug);
        let dot_forward_local = format!("__sim_dot_forward_yield_value_{}", slug);
        let dot_forward_result = format!("__sim_dot_forward_yield_result_{}", slug);
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + dot_forward_result.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(
            depth,
            &format!(
                "{} = {} do |{}|",
                dot_forward_result, dot_forward_helper, dot_forward_local
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_dot_forward_block_copy_{}", slug),
            &dot_forward_local,
        );
        self.push_line(depth + 1, &dot_forward_local);
        self.push_line(depth, "end");
    }

    fn render_block_type_helper(&mut self, method: &MethodSpec, depth: usize) {
        let return_type = method.return_type.as_ref().unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: simulator method `{}` requested block type assertions without return type. This is a bug because generated block helpers need a typed yield expression. Fix: call returns(...) before with_block_type_asserts().",
                method.name
            )
        });
        assert!(
            method.kind == MethodKind::Instance && method.def_form == MethodDefForm::Regular,
            "INVARIANT VIOLATED: simulator method `{}` requested block type assertions for unsupported method form. This is a bug because helper generation currently models regular instance methods only. Fix: extend render_block_type_helper for this method form before enabling it.",
            method.name
        );

        let slug = method_slug(&method.name);
        self.push_line(depth, &format!("def __sim_with_value_{}", slug));
        self.push_line(
            depth + 1,
            &format!("yield {}", return_expression(return_type)),
        );
        self.push_line(depth, "end");
        self.push_line(depth, &format!("def __sim_forward_value_{}(&)", slug));
        self.push_line(depth + 1, &format!("__sim_with_value_{}(&)", slug));
        self.push_line(depth, "end");
        self.push_line(depth, &format!("def __sim_dot_forward_value_{}(...)", slug));
        self.push_line(depth + 1, &format!("__sim_with_value_{}(...)", slug));
        self.push_line(depth, "end");
        self.push_line(depth, "");
    }

    fn render_typed_local_assignment(
        &mut self,
        caller: &MethodTarget,
        return_type: &str,
        depth: usize,
        local: &str,
        expression: &str,
    ) {
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + local.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(depth, &format!("{} = {}", local, expression));
    }

    fn render_constant_ref(
        &mut self,
        namespace: &NamespaceSpec,
        caller: &MethodTarget,
        constant: &super::graph::ConstantRefSpec,
        depth: usize,
    ) {
        let text = constant_ref_text(namespace, constant);
        self.map.constant_refs.push(ConstantRefSite {
            caller: caller.clone(),
            target: constant.fqn.clone(),
            text: text.clone(),
            shape: constant.shape.clone(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + constant_ref_cursor_offset(&constant.shape, &text))
                    as u32,
            },
        });
        self.push_line(depth, &text);
    }

    fn render_call(
        &mut self,
        caller: &MethodTarget,
        target: &MethodTarget,
        shape: &CallShape,
        depth: usize,
    ) {
        match shape {
            CallShape::Bare => {
                self.record_call(caller, target, shape, depth, "");
                self.push_line(depth, &target.name);
            }
            CallShape::BareInDoBlock => {
                self.push_line(depth, "[1].each do |_v|");
                self.record_call(caller, target, shape, depth + 1, "");
                self.push_line(depth + 1, &target.name);
                self.push_line(depth, "end");
            }
            CallShape::BareInBraceBlock => {
                let receiver = "[1].map { |_v| ";
                self.record_call(caller, target, shape, depth, receiver);
                self.push_line(depth, &format!("{}{} }}", receiver, target.name));
            }
            CallShape::BareInLambda => {
                let receiver = "lambda_probe = -> { ";
                self.record_call(caller, target, shape, depth, receiver);
                self.push_line(depth, &format!("{}{} }}", receiver, target.name));
            }
            CallShape::BareInProc => {
                let receiver = "Proc.new { ";
                self.record_call(caller, target, shape, depth, receiver);
                self.push_line(depth, &format!("{}{} }}", receiver, target.name));
            }
            CallShape::Super => {
                self.record_call(caller, target, shape, depth, "");
                self.push_line(depth, "super");
            }
            CallShape::LocalVar { name } => {
                self.push_line(depth, &format!("{} = {}.new", name, target.owner));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::Ivar { name } => {
                let ivar = format!("@{}", name);
                self.push_line(depth, &format!("{} = {}.new", ivar, target.owner));
                let receiver = format!("{}.", ivar);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ClassSend => {
                let receiver = format!("{}.", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::MethodObject => {
                let receiver = match target.kind {
                    MethodKind::Class => format!("{}.method(:", target.owner),
                    MethodKind::Instance => "method(:".to_string(),
                };
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{})", receiver, target.name));
            }
            CallShape::InstanceMethodObject => {
                let receiver = format!("{}.instance_method(:", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{})", receiver, target.name));
            }
            CallShape::ClassReceiver { receiver_owner } => {
                let receiver = format!("{}.", receiver_owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ConstructorSend => {
                let receiver = format!("{}.new.", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::StaticSend => {
                let receiver = format!("{}.new.send(:", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{})", receiver, target.name));
            }
            CallShape::OneHopChain {
                name,
                receiver_owner,
                hop_method,
            } => {
                self.push_line(depth, &format!("{} = {}.new", name, receiver_owner));
                let receiver = format!("{}.{}.", name, hop_method);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ReceiverLocalVar {
                name,
                receiver_owner,
            } => {
                self.push_line(depth, &format!("{} = {}.new", name, receiver_owner));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ArrayBlockParam { name } => {
                self.push_line(depth, &format!("[{}.new].each do |{}|", target.owner, name));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth + 1, &receiver);
                self.push_line(depth + 1, &format!("{}{}", receiver, target.name));
                self.push_line(depth, "end");
            }
            CallShape::YieldBlockParam { name } => {
                let helper = yield_helper_name(target);
                self.push_line(depth, &format!("def {}", helper));
                self.push_line(depth + 1, &format!("yield {}.new", target.owner));
                self.push_line(depth, "end");
                self.push_line(depth, &format!("{} do |{}|", helper, name));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth + 1, &receiver);
                self.push_line(depth + 1, &format!("{}{}", receiver, target.name));
                self.push_line(depth, "end");
            }
        }
    }

    fn record_call(
        &mut self,
        caller: &MethodTarget,
        target: &MethodTarget,
        shape: &CallShape,
        depth: usize,
        receiver: &str,
    ) {
        self.map.calls.push(CallSite {
            caller: caller.clone(),
            target: target.clone(),
            shape: shape.clone(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + receiver.len()) as u32,
            },
            shape_name: shape.label(),
            definition_support: method_definition_support(shape),
            reference_support: method_reference_support(shape),
            hover_support: method_hover_support(shape),
        });

        assert!(
            !target.name.is_empty(),
            "INVARIANT VIOLATED: generated call has empty method name. This is a bug because call positions must point at a Ruby identifier. Fix: validate MethodTarget parsing."
        );
    }

    fn push_line(&mut self, depth: usize, text: &str) {
        self.code.push_str(&"  ".repeat(depth));
        self.code.push_str(text);
        self.code.push('\n');
        self.line += 1;
    }
}

fn constant_ref_text(
    namespace: &NamespaceSpec,
    constant: &super::graph::ConstantRefSpec,
) -> String {
    match &constant.shape {
        ConstantRefShape::Auto => {
            let prefix = format!("{}::", namespace.fqn);
            if let Some(local) = constant.fqn.strip_prefix(&prefix) {
                return local.to_string();
            }
            constant.fqn.clone()
        }
        ConstantRefShape::Absolute => format!("::{}", constant.fqn),
        ConstantRefShape::ConstGet => {
            let (receiver, leaf) = const_get_receiver(&constant.fqn);
            format!("{receiver}.const_get(:{leaf})")
        }
        ConstantRefShape::ConstDefined => {
            let (receiver, leaf) = const_get_receiver(&constant.fqn);
            format!("{receiver}.const_defined?(:{leaf})")
        }
        ConstantRefShape::RelativeName { name } => name.clone(),
        ConstantRefShape::Qualified { path } => path.clone(),
    }
}

fn constant_ref_cursor_offset(shape: &ConstantRefShape, text: &str) -> usize {
    match shape {
        ConstantRefShape::ConstGet | ConstantRefShape::ConstDefined => text
            .rfind(':')
            .map(|idx| idx + ':'.len_utf8())
            .unwrap_or_else(|| leaf_segment_offset(text)),
        ConstantRefShape::Auto
        | ConstantRefShape::Absolute
        | ConstantRefShape::RelativeName { .. }
        | ConstantRefShape::Qualified { .. } => leaf_segment_offset(text),
    }
}

fn leaf_segment_offset(fqn: &str) -> usize {
    fqn.rfind("::").map(|idx| idx + "::".len()).unwrap_or(0)
}

fn const_get_receiver(fqn: &str) -> (String, String) {
    let Some(index) = fqn.rfind("::") else {
        return ("Object".to_string(), fqn.to_string());
    };
    (
        fqn[..index].to_string(),
        fqn[index + "::".len()..].to_string(),
    )
}

fn method_definition_support(shape: &CallShape) -> OracleSupport {
    match shape {
        CallShape::Bare
        | CallShape::BareInDoBlock
        | CallShape::BareInBraceBlock
        | CallShape::BareInLambda
        | CallShape::BareInProc
        | CallShape::Super
        | CallShape::LocalVar { .. }
        | CallShape::Ivar { .. }
        | CallShape::ClassSend
        | CallShape::MethodObject
        | CallShape::InstanceMethodObject
        | CallShape::ClassReceiver { .. }
        | CallShape::ConstructorSend
        | CallShape::StaticSend
        | CallShape::OneHopChain { .. }
        | CallShape::ReceiverLocalVar { .. }
        | CallShape::ArrayBlockParam { .. }
        | CallShape::YieldBlockParam { .. } => OracleSupport::Supported,
    }
}

fn method_reference_support(shape: &CallShape) -> OracleSupport {
    match shape {
        CallShape::Bare
        | CallShape::BareInDoBlock
        | CallShape::BareInBraceBlock
        | CallShape::BareInLambda
        | CallShape::BareInProc
        | CallShape::Super
        | CallShape::LocalVar { .. }
        | CallShape::Ivar { .. }
        | CallShape::ClassSend
        | CallShape::MethodObject
        | CallShape::InstanceMethodObject
        | CallShape::ClassReceiver { .. }
        | CallShape::ConstructorSend
        | CallShape::StaticSend
        | CallShape::OneHopChain { .. }
        | CallShape::ReceiverLocalVar { .. }
        | CallShape::ArrayBlockParam { .. }
        | CallShape::YieldBlockParam { .. } => OracleSupport::Supported,
    }
}

fn method_hover_support(shape: &CallShape) -> OracleSupport {
    match shape {
        CallShape::Bare
        | CallShape::BareInDoBlock
        | CallShape::BareInBraceBlock
        | CallShape::BareInLambda
        | CallShape::BareInProc
        | CallShape::Super
        | CallShape::LocalVar { .. }
        | CallShape::Ivar { .. }
        | CallShape::ClassSend
        | CallShape::MethodObject
        | CallShape::InstanceMethodObject
        | CallShape::ClassReceiver { .. }
        | CallShape::ConstructorSend
        | CallShape::StaticSend
        | CallShape::OneHopChain { .. }
        | CallShape::ReceiverLocalVar { .. }
        | CallShape::ArrayBlockParam { .. }
        | CallShape::YieldBlockParam { .. } => OracleSupport::Supported,
    }
}

fn yield_helper_name(target: &MethodTarget) -> String {
    let owner = target
        .owner
        .split("::")
        .map(underscore)
        .collect::<Vec<_>>()
        .join("_");
    format!("__sim_yield_{}_{}", owner, method_slug(&target.name))
}

fn method_slug(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn return_expression(return_type: &str) -> String {
    match return_type {
        "String" => "\"value\"".to_string(),
        "Integer" => "1".to_string(),
        "Float" => "1.0".to_string(),
        "Symbol" => ":value".to_string(),
        "NilClass" => "nil".to_string(),
        class_name => format!("{}.new", class_name),
    }
}

fn indent_len(depth: usize) -> usize {
    depth * 2
}
