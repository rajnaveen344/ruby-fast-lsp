#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceKind {
    Class,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodKind {
    Instance,
    Class,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodDefForm {
    Regular,
    SingletonClassBlock,
    ClassEvalBlock,
    DefineMethod,
    ConstGetDefineMethod,
    ModuleFunctionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodVisibility {
    Public,
    Protected,
    Private,
}

impl MethodVisibility {
    pub fn keyword(self) -> &'static str {
        match self {
            MethodVisibility::Public => "public",
            MethodVisibility::Protected => "protected",
            MethodVisibility::Private => "private",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodVisibilitySyntax {
    ScopeKeyword,
    ArgumentList,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MethodTarget {
    pub owner: String,
    pub name: String,
    pub kind: MethodKind,
}

impl MethodTarget {
    pub fn parse(input: &str) -> Self {
        if let Some((owner, name)) = input.split_once('#') {
            return Self {
                owner: owner.to_string(),
                name: name.to_string(),
                kind: MethodKind::Instance,
            };
        }

        if let Some((owner, name)) = input.rsplit_once('.') {
            return Self {
                owner: owner.to_string(),
                name: name.to_string(),
                kind: MethodKind::Class,
            };
        }

        panic!(
            "INVARIANT VIOLATED: method target `{}` is invalid. This is a bug because simulation targets must be `Owner#method` or `Owner.method`. Fix: pass a fully-qualified method target.",
            input
        );
    }

    pub fn signature(&self) -> String {
        let sep = match self.kind {
            MethodKind::Instance => "#",
            MethodKind::Class => ".",
        };
        format!("{}{}{}", self.owner, sep, self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallShape {
    Bare,
    BareInDoBlock,
    BareInBraceBlock,
    BareInLambda,
    BareInProc,
    Super,
    LocalVar {
        name: String,
    },
    Ivar {
        name: String,
    },
    ClassSend,
    MethodObject,
    InstanceMethodObject,
    ClassReceiver {
        receiver_owner: String,
    },
    ConstructorSend,
    StaticSend,
    OneHopChain {
        name: String,
        receiver_owner: String,
        hop_method: String,
    },
    ReceiverLocalVar {
        name: String,
        receiver_owner: String,
    },
    ArrayBlockParam {
        name: String,
    },
    YieldBlockParam {
        name: String,
    },
}

impl CallShape {
    pub fn local(name: impl Into<String>) -> Self {
        Self::LocalVar { name: name.into() }
    }

    pub fn ivar(name: impl Into<String>) -> Self {
        Self::Ivar { name: name.into() }
    }

    pub fn one_hop(
        name: impl Into<String>,
        receiver_owner: impl Into<String>,
        hop_method: impl Into<String>,
    ) -> Self {
        Self::OneHopChain {
            name: name.into(),
            receiver_owner: receiver_owner.into(),
            hop_method: hop_method.into(),
        }
    }

    pub fn receiver_local(name: impl Into<String>, receiver_owner: impl Into<String>) -> Self {
        Self::ReceiverLocalVar {
            name: name.into(),
            receiver_owner: receiver_owner.into(),
        }
    }

    pub fn array_block_param(name: impl Into<String>) -> Self {
        Self::ArrayBlockParam { name: name.into() }
    }

    pub fn yield_block_param(name: impl Into<String>) -> Self {
        Self::YieldBlockParam { name: name.into() }
    }

    pub fn class_receiver(receiver_owner: impl Into<String>) -> Self {
        Self::ClassReceiver {
            receiver_owner: receiver_owner.into(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CallShape::Bare => "bare",
            CallShape::BareInDoBlock => "bare-do-block",
            CallShape::BareInBraceBlock => "bare-brace-block",
            CallShape::BareInLambda => "bare-lambda",
            CallShape::BareInProc => "bare-proc",
            CallShape::Super => "super",
            CallShape::LocalVar { .. } => "local",
            CallShape::Ivar { .. } => "ivar",
            CallShape::ClassSend => "class",
            CallShape::MethodObject => "method-object",
            CallShape::InstanceMethodObject => "instance-method-object",
            CallShape::ClassReceiver { .. } => "class-receiver",
            CallShape::ConstructorSend => "constructor",
            CallShape::StaticSend => "static-send",
            CallShape::OneHopChain { .. } => "one-hop",
            CallShape::ReceiverLocalVar { .. } => "receiver-local",
            CallShape::ArrayBlockParam { .. } => "array-block-param",
            CallShape::YieldBlockParam { .. } => "yield-block-param",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSpec {
    pub target: MethodTarget,
    pub shape: CallShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantRefSpec {
    pub fqn: String,
    pub shape: ConstantRefShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstantRefShape {
    Auto,
    Absolute,
    ConstGet,
    ConstDefined,
    RelativeName { name: String },
    Qualified { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSpec {
    pub name: String,
    pub kind: MethodKind,
    pub def_form: MethodDefForm,
    pub visibility: MethodVisibility,
    pub visibility_syntax: MethodVisibilitySyntax,
    pub enabled: bool,
    pub return_type: Option<String>,
    pub block_type_asserts: bool,
    pub calls: Vec<CallSpec>,
    pub constant_refs: Vec<ConstantRefSpec>,
}

impl MethodSpec {
    pub fn calls(&mut self, target: &str, shape: CallShape) -> &mut Self {
        self.calls.push(CallSpec {
            target: MethodTarget::parse(target),
            shape,
        });
        self
    }

    pub fn ref_const(&mut self, fqn: &str) -> &mut Self {
        self.constant_refs.push(ConstantRefSpec {
            fqn: fqn.to_string(),
            shape: ConstantRefShape::Auto,
        });
        self
    }

    pub fn ref_const_absolute(&mut self, fqn: &str) -> &mut Self {
        self.constant_refs.push(ConstantRefSpec {
            fqn: fqn.to_string(),
            shape: ConstantRefShape::Absolute,
        });
        self
    }

    pub fn ref_const_const_get(&mut self, fqn: &str) -> &mut Self {
        self.constant_refs.push(ConstantRefSpec {
            fqn: fqn.to_string(),
            shape: ConstantRefShape::ConstGet,
        });
        self
    }

    pub fn ref_const_const_defined(&mut self, fqn: &str) -> &mut Self {
        self.constant_refs.push(ConstantRefSpec {
            fqn: fqn.to_string(),
            shape: ConstantRefShape::ConstDefined,
        });
        self
    }

    pub fn ref_const_relative(&mut self, fqn: &str, name: &str) -> &mut Self {
        self.constant_refs.push(ConstantRefSpec {
            fqn: fqn.to_string(),
            shape: ConstantRefShape::RelativeName {
                name: name.to_string(),
            },
        });
        self
    }

    pub fn ref_const_qualified(&mut self, fqn: &str, path: &str) -> &mut Self {
        self.constant_refs.push(ConstantRefSpec {
            fqn: fqn.to_string(),
            shape: ConstantRefShape::Qualified {
                path: path.to_string(),
            },
        });
        self
    }

    pub fn returns(&mut self, fqn: &str) -> &mut Self {
        self.return_type = Some(fqn.to_string());
        self
    }

    pub fn with_block_type_asserts(&mut self) -> &mut Self {
        self.block_type_asserts = true;
        self
    }

    pub fn public(&mut self) -> &mut Self {
        self.visibility = MethodVisibility::Public;
        self
    }

    pub fn protected(&mut self) -> &mut Self {
        self.visibility = MethodVisibility::Protected;
        self
    }

    pub fn private(&mut self) -> &mut Self {
        self.visibility = MethodVisibility::Private;
        self
    }

    pub fn visibility_argument_list(&mut self) -> &mut Self {
        self.visibility_syntax = MethodVisibilitySyntax::ArgumentList;
        self
    }

    pub fn in_singleton_class_block(&mut self) -> &mut Self {
        assert!(
            self.kind == MethodKind::Class,
            "INVARIANT VIOLATED: method `{}` cannot use class << self form because it is not a class method. This is a bug because singleton class blocks define class methods in the simulator. Fix: call this only on class_method().",
            self.name
        );
        self.def_form = MethodDefForm::SingletonClassBlock;
        self
    }

    pub fn in_class_eval_block(&mut self) -> &mut Self {
        self.def_form = MethodDefForm::ClassEvalBlock;
        self
    }

    pub fn as_define_method(&mut self) -> &mut Self {
        self.def_form = MethodDefForm::DefineMethod;
        self
    }

    pub fn as_const_get_define_method(&mut self) -> &mut Self {
        assert!(
            self.kind == MethodKind::Instance,
            "INVARIANT VIOLATED: method `{}` cannot use const_get define_method form because it is not an instance method. This is a bug because Module#const_get(...).send(:define_method, ...) defines instance methods on the resolved class/module. Fix: call this only on method().",
            self.name
        );
        self.def_form = MethodDefForm::ConstGetDefineMethod;
        self
    }

    pub fn in_module_function_mode(&mut self) -> &mut Self {
        assert!(
            self.kind == MethodKind::Instance,
            "INVARIANT VIOLATED: method `{}` cannot use bare module_function mode because it is not an instance method. This is a bug because bare module_function duplicates instance methods as singleton methods. Fix: call this only on method().",
            self.name
        );
        self.def_form = MethodDefForm::ModuleFunctionMode;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantSpec {
    pub name: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeSpec {
    pub fqn: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilityOverrideSpec {
    pub name: String,
    pub visibility: MethodVisibility,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasForm {
    Keyword,
    MethodCall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasSpec {
    pub new_name: String,
    pub old_name: String,
    pub kind: MethodKind,
    pub form: AliasForm,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateSpec {
    pub new_name: String,
    pub receiver_method: String,
    pub kind: MethodKind,
    pub form: DelegateForm,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassAttributeSpec {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegateForm {
    Rails,
    ForwardableSingular,
    ForwardablePlural,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceSpec {
    pub fqn: String,
    pub kind: NamespaceKind,
    pub file_path: Option<String>,
    pub enabled: bool,
    pub superclass: Option<String>,
    pub prepends: Vec<IncludeSpec>,
    pub includes: Vec<IncludeSpec>,
    pub extends: Vec<IncludeSpec>,
    pub extend_self: bool,
    pub singleton_prepends: Vec<IncludeSpec>,
    pub singleton_includes: Vec<IncludeSpec>,
    pub included_hook_extends: Vec<IncludeSpec>,
    pub included_hook_includes: Vec<IncludeSpec>,
    pub included_hook_class_eval_includes: Vec<IncludeSpec>,
    pub concern_class_methods: Vec<IncludeSpec>,
    pub visibility_overrides: Vec<VisibilityOverrideSpec>,
    pub constants: Vec<ConstantSpec>,
    pub methods: Vec<MethodSpec>,
    pub aliases: Vec<AliasSpec>,
    pub delegates: Vec<DelegateSpec>,
    pub class_attributes: Vec<ClassAttributeSpec>,
}

impl NamespaceSpec {
    pub fn new(fqn: impl Into<String>, kind: NamespaceKind) -> Self {
        Self {
            fqn: fqn.into(),
            kind,
            file_path: None,
            enabled: true,
            superclass: None,
            prepends: Vec::new(),
            includes: Vec::new(),
            extends: Vec::new(),
            extend_self: false,
            singleton_prepends: Vec::new(),
            singleton_includes: Vec::new(),
            included_hook_extends: Vec::new(),
            included_hook_includes: Vec::new(),
            included_hook_class_eval_includes: Vec::new(),
            concern_class_methods: Vec::new(),
            visibility_overrides: Vec::new(),
            constants: Vec::new(),
            methods: Vec::new(),
            aliases: Vec::new(),
            delegates: Vec::new(),
            class_attributes: Vec::new(),
        }
    }

    pub fn method_mut(&mut self, target: &MethodTarget) -> Option<&mut MethodSpec> {
        if self.fqn != target.owner {
            return None;
        }

        let method_name = self
            .aliases
            .iter()
            .find(|alias| alias.new_name == target.name && alias.kind == target.kind)
            .map(|alias| alias.old_name.as_str())
            .unwrap_or(target.name.as_str());

        self.methods
            .iter_mut()
            .find(|method| method.name == method_name && method.kind == target.kind)
    }

    pub fn constant_mut(&mut self, constant_fqn: &str) -> Option<&mut ConstantSpec> {
        let prefix = format!("{}::", self.fqn);
        let name = constant_fqn.strip_prefix(&prefix)?;
        self.constants
            .iter_mut()
            .find(|constant| constant.name == name)
    }
}

pub struct NamespaceBuilder<'a> {
    namespace: &'a mut NamespaceSpec,
}

impl<'a> NamespaceBuilder<'a> {
    pub(crate) fn new(namespace: &'a mut NamespaceSpec) -> Self {
        Self { namespace }
    }

    pub fn file_path(&mut self, path: &str) -> &mut Self {
        self.namespace.file_path = Some(path.to_string());
        self
    }

    pub fn superclass(&mut self, fqn: &str) -> &mut Self {
        self.namespace.superclass = Some(fqn.to_string());
        self
    }

    pub fn include(&mut self, fqn: &str) -> &mut Self {
        self.namespace.includes.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn prepend(&mut self, fqn: &str) -> &mut Self {
        self.namespace.prepends.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn extend(&mut self, fqn: &str) -> &mut Self {
        self.namespace.extends.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn extend_self(&mut self) -> &mut Self {
        self.namespace.extend_self = true;
        self
    }

    pub fn singleton_include(&mut self, fqn: &str) -> &mut Self {
        self.namespace.singleton_includes.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn singleton_prepend(&mut self, fqn: &str) -> &mut Self {
        self.namespace.singleton_prepends.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn included_hook_extend(&mut self, fqn: &str) -> &mut Self {
        self.namespace.included_hook_extends.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn included_hook_include(&mut self, fqn: &str) -> &mut Self {
        self.namespace.included_hook_includes.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn included_hook_class_eval_include(&mut self, fqn: &str) -> &mut Self {
        self.namespace
            .included_hook_class_eval_includes
            .push(IncludeSpec {
                fqn: fqn.to_string(),
                enabled: true,
            });
        self
    }

    pub fn concern_class_methods(&mut self, fqn: &str) -> &mut Self {
        self.namespace.concern_class_methods.push(IncludeSpec {
            fqn: fqn.to_string(),
            enabled: true,
        });
        self
    }

    pub fn visibility_override(&mut self, name: &str, visibility: MethodVisibility) -> &mut Self {
        self.namespace
            .visibility_overrides
            .push(VisibilityOverrideSpec {
                name: name.to_string(),
                visibility,
                enabled: true,
            });
        self
    }

    pub fn private_visibility(&mut self, name: &str) -> &mut Self {
        self.visibility_override(name, MethodVisibility::Private)
    }

    pub fn protected_visibility(&mut self, name: &str) -> &mut Self {
        self.visibility_override(name, MethodVisibility::Protected)
    }

    pub fn public_visibility(&mut self, name: &str) -> &mut Self {
        self.visibility_override(name, MethodVisibility::Public)
    }

    pub fn constant(&mut self, name: &str, value: &str) -> &mut Self {
        self.namespace.constants.push(ConstantSpec {
            name: name.to_string(),
            value: value.to_string(),
            enabled: true,
        });
        self
    }

    pub fn method(&mut self, name: &str) -> &mut MethodSpec {
        self.push_method(name, MethodKind::Instance)
    }

    pub fn class_method(&mut self, name: &str) -> &mut MethodSpec {
        self.push_method(name, MethodKind::Class)
    }

    pub fn alias_instance_method(&mut self, new_name: &str, old_name: &str) -> &mut Self {
        self.push_alias(new_name, old_name, MethodKind::Instance, AliasForm::Keyword)
    }

    pub fn alias_method_instance_method(&mut self, new_name: &str, old_name: &str) -> &mut Self {
        self.push_alias(
            new_name,
            old_name,
            MethodKind::Instance,
            AliasForm::MethodCall,
        )
    }

    pub fn alias_class_method(&mut self, new_name: &str, old_name: &str) -> &mut Self {
        self.push_alias(new_name, old_name, MethodKind::Class, AliasForm::Keyword)
    }

    pub fn delegate_instance_method(&mut self, new_name: &str, receiver_method: &str) -> &mut Self {
        self.push_delegate(
            new_name,
            receiver_method,
            MethodKind::Instance,
            DelegateForm::Rails,
        )
    }

    pub fn forwardable_class_method(&mut self, new_name: &str, receiver_method: &str) -> &mut Self {
        self.push_delegate(
            new_name,
            receiver_method,
            MethodKind::Class,
            DelegateForm::ForwardablePlural,
        )
    }

    pub fn class_attribute(&mut self, name: &str) -> &mut Self {
        self.namespace.class_attributes.push(ClassAttributeSpec {
            name: name.to_string(),
            enabled: true,
        });
        self
    }

    fn push_alias(
        &mut self,
        new_name: &str,
        old_name: &str,
        kind: MethodKind,
        form: AliasForm,
    ) -> &mut Self {
        self.namespace.aliases.push(AliasSpec {
            new_name: new_name.to_string(),
            old_name: old_name.to_string(),
            kind,
            form,
            enabled: true,
        });
        self
    }

    fn push_delegate(
        &mut self,
        new_name: &str,
        receiver_method: &str,
        kind: MethodKind,
        form: DelegateForm,
    ) -> &mut Self {
        self.namespace.delegates.push(DelegateSpec {
            new_name: new_name.to_string(),
            receiver_method: receiver_method.to_string(),
            kind,
            form,
            enabled: true,
        });
        self
    }

    fn push_method(&mut self, name: &str, kind: MethodKind) -> &mut MethodSpec {
        self.namespace.methods.push(MethodSpec {
            name: name.to_string(),
            kind,
            def_form: MethodDefForm::Regular,
            visibility: MethodVisibility::Public,
            visibility_syntax: MethodVisibilitySyntax::ScopeKeyword,
            enabled: true,
            return_type: None,
            block_type_asserts: false,
            calls: Vec::new(),
            constant_refs: Vec::new(),
        });
        self.namespace
            .methods
            .last_mut()
            .expect("INVARIANT VIOLATED: just-pushed method is missing. This is a bug because Vec::push must create a last element. Fix: inspect NamespaceBuilder::push_method.")
    }
}
