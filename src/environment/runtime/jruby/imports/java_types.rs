//! Static Java type expressions, `to_java` conversions, and JVM-to-Ruby type
//! mapping.

use super::syntax::{collect_ruby_constant_path, dotted_call_name, static_symbol_or_string};
use super::JrubyImportProvider;
use ruby_analysis::core::{FullyQualifiedName, RubyConstant, RubyType, TypeProvenance};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_fast_lsp_jvm_metadata::JvmType;
use ruby_prism::{CallNode, Node};

impl JrubyImportProvider {
    pub(super) fn process_to_java_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.name().as_slice() != b"to_java" {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            if let RubyType::Array(_) = visitor.infer_type_from_value(&receiver) {
                visitor.direct_push_expression_type(
                    &node.as_node(),
                    RubyType::array_of(RubyType::Class(
                        FullyQualifiedName::try_from("Java::JavaLang::Object").expect(
                            "INVARIANT VIOLATED: Java Object proxy FQN is invalid. \
                             This is a bug because it is a canonical JRuby proxy name. \
                             Fix: keep built-in Java proxy identities valid Ruby constants.",
                        ),
                    )),
                    TypeProvenance::Runtime,
                );
            }
            return;
        };
        let arguments = arguments.arguments().iter().collect::<Vec<_>>();
        if arguments.len() != 1 {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "unsupported-jruby-to-java",
                "to_java accepts zero or one static Java target type.".to_string(),
            );
            return;
        }
        let Some(target) = self.static_to_java_type(visitor, &arguments[0]) else {
            return;
        };
        let receiver_type = visitor.infer_type_from_value(&receiver);
        let result = if matches!(receiver_type, RubyType::Array(_)) {
            RubyType::array_of(ruby_type_for_jvm(&target))
        } else {
            ruby_type_for_to_java_scalar(&target)
        };
        visitor.direct_push_expression_type(&node.as_node(), result, TypeProvenance::Runtime);
    }

    pub(super) fn static_java_signature(
        &self,
        visitor: &FactCollector,
        node: &Node<'_>,
    ) -> Option<Vec<JvmType>> {
        let array = node.as_array_node()?;
        array
            .elements()
            .iter()
            .map(|element| self.static_java_type(visitor, &element))
            .collect()
    }

    fn static_java_type(&self, visitor: &FactCollector, node: &Node<'_>) -> Option<JvmType> {
        if let Some(call) = node.as_call_node() {
            if call.name().as_slice() == b"[]"
                && call.arguments().is_none()
                && call.block().is_none()
            {
                return call
                    .receiver()
                    .and_then(|receiver| self.static_java_type(visitor, &receiver))
                    .map(|element| JvmType::Array(Box::new(element)));
            }
            if call.arguments().is_none()
                && call.block().is_none()
                && call.receiver().as_ref().is_some_and(|receiver| {
                    receiver
                        .as_constant_read_node()
                        .is_some_and(|constant| constant.name().as_slice() == b"Java")
                })
            {
                return primitive_java_type(call.name().as_slice());
            }
            if let Some(class_name) =
                dotted_call_name(&call).and_then(|name| self.canonical_catalog_class(&name))
            {
                return Some(JvmType::Object(class_name));
            }
        }
        if let Some(reference) = static_constant_reference(node)
            .and_then(|reference| self.canonical_catalog_class(&reference))
        {
            return Some(JvmType::Object(reference));
        }
        match visitor.infer_type_from_value(node) {
            RubyType::ClassReference(proxy) | RubyType::ModuleReference(proxy) => self
                .canonical_catalog_class(&proxy.to_string())
                .map(JvmType::Object),
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::Literal(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => None,
        }
    }

    fn static_to_java_type(&self, visitor: &FactCollector, node: &Node<'_>) -> Option<JvmType> {
        if let Some(name) = static_symbol_or_string(node) {
            return to_java_symbol_type(&name);
        }
        self.static_java_type(visitor, node)
    }

    fn canonical_catalog_class(&self, name: &str) -> Option<String> {
        self.class_name_for_static_proxy_reference(name)
            .ok()
            .flatten()
    }
}

fn primitive_java_type(name: &[u8]) -> Option<JvmType> {
    match name {
        b"byte" => Some(JvmType::Byte),
        b"char" => Some(JvmType::Char),
        b"double" => Some(JvmType::Double),
        b"float" => Some(JvmType::Float),
        b"int" => Some(JvmType::Int),
        b"long" => Some(JvmType::Long),
        b"short" => Some(JvmType::Short),
        b"boolean" => Some(JvmType::Boolean),
        _ => None,
    }
}

fn to_java_symbol_type(name: &str) -> Option<JvmType> {
    match name {
        "byte" => Some(JvmType::Byte),
        "char" => Some(JvmType::Char),
        "double" => Some(JvmType::Double),
        "float" => Some(JvmType::Float),
        "int" | "integer" => Some(JvmType::Int),
        "long" => Some(JvmType::Long),
        "short" => Some(JvmType::Short),
        "boolean" => Some(JvmType::Boolean),
        "string" => Some(JvmType::Object("java/lang/String".to_string())),
        "object" => Some(JvmType::Object("java/lang/Object".to_string())),
        _ => None,
    }
}

fn static_constant_reference(node: &Node<'_>) -> Option<String> {
    if let Some(read) = node.as_constant_read_node() {
        return Some(String::from_utf8_lossy(read.name().as_slice()).to_string());
    }
    let path = node.as_constant_path_node()?;
    let mut parts = Vec::new();
    collect_ruby_constant_path(&path, &mut parts)?;
    Some(parts.join("::"))
}

pub(super) fn display_java_signature(signature: &[JvmType]) -> String {
    signature
        .iter()
        .map(display_java_type)
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_java_type(ty: &JvmType) -> String {
    match ty {
        JvmType::Byte => "byte".to_string(),
        JvmType::Char => "char".to_string(),
        JvmType::Double => "double".to_string(),
        JvmType::Float => "float".to_string(),
        JvmType::Int => "int".to_string(),
        JvmType::Long => "long".to_string(),
        JvmType::Short => "short".to_string(),
        JvmType::Boolean => "boolean".to_string(),
        JvmType::Void => "void".to_string(),
        JvmType::Object(name) => name.replace('/', "."),
        JvmType::Array(element) => format!("{}[]", display_java_type(element)),
    }
}

fn ruby_type_for_to_java_scalar(ty: &JvmType) -> RubyType {
    let proxy = match ty {
        JvmType::Byte => "Java::JavaLang::Byte",
        JvmType::Char => "Java::JavaLang::Character",
        JvmType::Double => "Java::JavaLang::Double",
        JvmType::Float => "Java::JavaLang::Float",
        JvmType::Int => "Java::JavaLang::Integer",
        JvmType::Long => "Java::JavaLang::Long",
        JvmType::Short => "Java::JavaLang::Short",
        JvmType::Boolean => "Java::JavaLang::Boolean",
        JvmType::Void => return RubyType::nil_class(),
        JvmType::Object(name) => {
            return JavaClassName::parse(name)
                .map(|name| {
                    RubyType::Class(
                        FullyQualifiedName::try_from(name.ruby_fqn().as_str()).expect(
                            "INVARIANT VIOLATED: validated Java class produced an invalid JRuby proxy FQN. \
                             This is a bug because JavaClassName owns proxy validation. \
                             Fix: keep Java-to-Ruby proxy conversion single-sourced.",
                        ),
                    )
                })
                .unwrap_or(RubyType::Unknown);
        }
        JvmType::Array(element) => return RubyType::array_of(ruby_type_for_jvm(element)),
    };
    RubyType::Class(FullyQualifiedName::try_from(proxy).expect(
        "INVARIANT VIOLATED: Java primitive wrapper proxy FQN is invalid. \
         This is a bug because wrapper mappings are static canonical JRuby names. \
         Fix: keep primitive wrapper proxy names valid Ruby constants.",
    ))
}

pub(crate) fn ruby_type_for_jvm(ty: &JvmType) -> RubyType {
    match ty {
        JvmType::Byte | JvmType::Char | JvmType::Int | JvmType::Long | JvmType::Short => {
            RubyType::integer()
        }
        JvmType::Double | JvmType::Float => RubyType::float(),
        JvmType::Boolean => RubyType::boolean(),
        JvmType::Void => RubyType::nil_class(),
        JvmType::Object(name) => JavaClassName::parse(name)
            .map(|name| {
                RubyType::Class(FullyQualifiedName::constant(
                    name.ruby_namespace_parts()
                        .into_iter()
                        .map(|part| {
                            RubyConstant::new(&part).expect(
                                "INVARIANT VIOLATED: validated Java proxy part is not a Ruby constant. \
                                 This is a bug because JavaClassName owns proxy validation. \
                                 Fix: keep Java-to-Ruby proxy conversion single-sourced.",
                            )
                        })
                        .collect::<Vec<_>>(),
                ))
            })
            .unwrap_or(RubyType::Unknown),
        JvmType::Array(element) => RubyType::array_of(ruby_type_for_jvm(element)),
    }
}
