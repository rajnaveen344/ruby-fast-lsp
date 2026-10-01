use super::source_text::{indent_len, leaf_segment_offset};
use super::{
    DirectMacroCallSite, FileRenderer, NamespaceDefSite, NamespaceRefSite, OracleSupport, SourcePos,
};
use crate::simulation::graph::{
    ConstantSpec, MethodDefForm, MethodKind, NamespaceKind, NamespaceSpec,
};
use crate::simulation::project::SyntheticProject;

impl FileRenderer {
    pub(super) fn render_namespace(
        &mut self,
        namespace: &NamespaceSpec,
        project: &SyntheticProject,
    ) {
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

        for route_call in &namespace.route_calls {
            self.render_route_call(namespace, route_call, leaf_depth + 1);
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

    pub(super) fn record_direct_macro_call(&mut self, depth: usize, name: &str) {
        self.map.direct_macro_calls.push(DirectMacroCallSite {
            name: name.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: indent_len(depth) as u32,
            },
        });
    }
}
