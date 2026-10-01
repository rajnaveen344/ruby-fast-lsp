use super::source_text::{const_get_receiver, indent_len};
use super::{FileRenderer, SourcePos, TypeAssertKind, TypeAssertSite};
use crate::simulation::graph::{
    AliasForm, DelegateForm, DelegateSpec, MethodDefForm, MethodKind, MethodSpec, MethodTarget,
    MethodVisibility, MethodVisibilitySyntax, NamespaceSpec,
};

impl FileRenderer {
    pub(super) fn render_method(
        &mut self,
        namespace: &NamespaceSpec,
        method: &MethodSpec,
        depth: usize,
    ) {
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

    pub(super) fn render_class_eval_method(
        &mut self,
        namespace: &NamespaceSpec,
        method: &MethodSpec,
    ) {
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
            self.render_constant_ref(&namespace.fqn, &caller, constant, depth + 1);
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

    pub(super) fn render_const_get_define_method(
        &mut self,
        namespace: &NamespaceSpec,
        method: &MethodSpec,
    ) {
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
            self.render_constant_ref("", &caller, constant, 1);
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

        // The eval form is emitted after all namespace bodies have closed.
        // Changing self with class_eval does not change Ruby's lexical nesting.
        let lexical_scope = match method.def_form {
            MethodDefForm::ClassEvalBlock | MethodDefForm::ConstGetDefineMethod => "",
            MethodDefForm::Regular
            | MethodDefForm::SingletonClassBlock
            | MethodDefForm::DefineMethod
            | MethodDefForm::ModuleFunctionMode => &namespace.fqn,
        };
        for constant in &method.constant_refs {
            self.render_constant_ref(lexical_scope, &caller, constant, depth + 1);
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

    pub(super) fn render_alias(
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

    pub(super) fn render_delegate(
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

    pub(super) fn render_class_attribute(
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
}
