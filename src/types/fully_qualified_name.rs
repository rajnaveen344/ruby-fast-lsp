use ustr::Ustr;

use crate::analyzer_prism::Identifier;

pub use ruby_analysis_core::FullyQualifiedName;
pub use ruby_analysis_core::NamespaceKind;

impl From<Identifier> for FullyQualifiedName {
    fn from(value: Identifier) -> Self {
        match value {
            Identifier::RubyConstant { namespace: _, iden } => FullyQualifiedName::Constant(iden),
            Identifier::RubyMethod {
                namespace, iden, ..
            } => FullyQualifiedName::Method(namespace, iden),
            Identifier::RubyLocalVariable { name, .. } => {
                FullyQualifiedName::LocalVariable(Ustr::from(&name))
            }
            Identifier::RubyInstanceVariable { name, .. } => {
                FullyQualifiedName::InstanceVariable(Ustr::from(&name))
            }
            Identifier::RubyClassVariable { name, .. } => {
                FullyQualifiedName::ClassVariable(Ustr::from(&name))
            }
            Identifier::RubyGlobalVariable { name, .. } => {
                FullyQualifiedName::GlobalVariable(Ustr::from(&name))
            }
            Identifier::YardType { type_name, .. } => {
                let namespace = type_name
                    .split("::")
                    .filter_map(|part| {
                        crate::types::ruby_namespace::RubyConstant::try_from(part.trim()).ok()
                    })
                    .collect();
                FullyQualifiedName::Constant(namespace)
            }
        }
    }
}
