use crate::{JavaClassName, JavaNameError};
use ruby_fast_lsp_jvm_metadata::{
    ClassFile, ClassKind, JvmType, MemberInfo, MethodDescriptor, Visibility,
};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    InvalidClassName(JavaNameError),
    InvalidDescriptor(String),
}

pub fn generate_ruby_signature(class: &ClassFile) -> Result<String, SignatureError> {
    let name = JavaClassName::parse(&class.name).map_err(SignatureError::InvalidClassName)?;
    let mut output = String::new();
    writeln!(
        output,
        "# Generated from JVM metadata for {}. Method bodies are intentionally unavailable.",
        class.name
    )
    .expect("INVARIANT VIOLATED: writing to String cannot fail");
    writeln!(output, "module Java").expect("INVARIANT VIOLATED: writing to String cannot fail");
    writeln!(output, "  module {}", name.package_proxy_module())
        .expect("INVARIANT VIOLATED: writing to String cannot fail");
    let mut indent = 4usize;
    for outer in &name.classes()[..name.classes().len() - 1] {
        writeln!(output, "{:indent$}class {outer}", "")
            .expect("INVARIANT VIOLATED: writing to String cannot fail");
        indent += 2;
    }
    let declaration = match class.kind() {
        ClassKind::Interface | ClassKind::Annotation | ClassKind::Module => "module",
        ClassKind::Class | ClassKind::Enum | ClassKind::Record => "class",
    };
    let class_name = name.imported_constant();
    let superclass = class
        .super_name
        .as_deref()
        .filter(|_| declaration == "class")
        .and_then(|superclass| JavaClassName::parse(superclass).ok())
        .map(|superclass| format!(" < {}", superclass.ruby_fqn()))
        .unwrap_or_default();
    writeln!(
        output,
        "{:indent$}{declaration} {class_name}{superclass}",
        ""
    )
    .expect("INVARIANT VIOLATED: writing to String cannot fail");
    indent += 2;

    for interface in &class.interfaces {
        if let Ok(interface) = JavaClassName::parse(interface) {
            writeln!(output, "{:indent$}include {}", "", interface.ruby_fqn())
                .expect("INVARIANT VIOLATED: writing to String cannot fail");
        }
    }
    for field in &class.fields {
        write_field(&mut output, field, &name, indent)?;
    }
    let mut current_visibility = Visibility::Public;
    for method in &class.methods {
        if method.name == "<clinit>" {
            continue;
        }
        if !valid_ruby_method_name(java_method_name(method)) {
            continue;
        }
        if method.visibility() != current_visibility {
            current_visibility = method.visibility();
            writeln!(
                output,
                "{:indent$}{}",
                "",
                visibility_keyword(current_visibility)
            )
            .expect("INVARIANT VIOLATED: writing to String cannot fail");
        }
        write_method(&mut output, method, class.kind(), indent)?;
    }

    indent -= 2;
    writeln!(output, "{:indent$}end", "")
        .expect("INVARIANT VIOLATED: writing to String cannot fail");
    for _ in &name.classes()[..name.classes().len() - 1] {
        indent -= 2;
        writeln!(output, "{:indent$}end", "")
            .expect("INVARIANT VIOLATED: writing to String cannot fail");
    }
    writeln!(output, "  end\nend").expect("INVARIANT VIOLATED: writing to String cannot fail");
    Ok(output)
}

fn write_field(
    output: &mut String,
    field: &MemberInfo,
    owner: &JavaClassName,
    indent: usize,
) -> Result<(), SignatureError> {
    let ty = ruby_fast_lsp_jvm_metadata::parse_field_descriptor(&field.descriptor)
        .map_err(|_| SignatureError::InvalidDescriptor(field.descriptor.clone()))?;
    let ruby_type = ruby_type_for_jvm_type(&ty);
    if field.is_static() && (field.is_final() || field.is_enum_constant()) {
        if valid_ruby_constant_name(&field.name) {
            writeln!(
                output,
                "{:indent$}# @ruby_fast_lsp_navigation declaration-only: JVM fields and enum constants have no executable implementation body.",
                ""
            )
            .expect("INVARIANT VIOLATED: writing to String cannot fail");
            writeln!(output, "{:indent$}# @type [{ruby_type}]", "")
                .expect("INVARIANT VIOLATED: writing to String cannot fail");
            writeln!(output, "{:indent$}{} = nil", "", field.name)
                .expect("INVARIANT VIOLATED: writing to String cannot fail");
        }
        return Ok(());
    }
    if valid_ruby_method_name(&field.name) {
        let receiver = if field.is_static() { "self." } else { "" };
        writeln!(
            output,
            "{:indent$}# @ruby_fast_lsp_navigation declaration-only: generated field access has no Java method body.",
            ""
        )
        .expect("INVARIANT VIOLATED: writing to String cannot fail");
        writeln!(output, "{:indent$}# @return [{ruby_type}]", "")
            .expect("INVARIANT VIOLATED: writing to String cannot fail");
        writeln!(output, "{:indent$}def {receiver}{}; end", "", field.name)
            .expect("INVARIANT VIOLATED: writing to String cannot fail");
        writeln!(output, "{:indent$}# @param value [{ruby_type}]", "")
            .expect("INVARIANT VIOLATED: writing to String cannot fail");
        writeln!(
            output,
            "{:indent$}def {receiver}{}=(value); end",
            "", field.name
        )
        .expect("INVARIANT VIOLATED: writing to String cannot fail");
    }
    let _ = owner;
    Ok(())
}

fn write_method(
    output: &mut String,
    method: &MemberInfo,
    class_kind: ClassKind,
    indent: usize,
) -> Result<(), SignatureError> {
    let descriptor = ruby_fast_lsp_jvm_metadata::parse_method_descriptor(&method.descriptor)
        .map_err(|_| SignatureError::InvalidDescriptor(method.descriptor.clone()))?;
    write_method_docs(output, method, &descriptor, indent);
    writeln!(
        output,
        "{:indent$}# @ruby_fast_lsp_navigation declaration-only: {}.",
        "",
        method_navigation_fallback_reason(method, class_kind)
    )
    .expect("INVARIANT VIOLATED: writing to String cannot fail");
    let name = java_method_name(method);
    let receiver = if method.name == "<init>" || method.is_static() {
        "self."
    } else {
        ""
    };
    let parameters = method
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let name = ruby_parameter_name(&parameter.name, index);
            if method.is_varargs() && index + 1 == method.parameters.len() {
                format!("*{name}")
            } else {
                name
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        output,
        "{:indent$}def {receiver}{name}({parameters}); end",
        ""
    )
    .expect("INVARIANT VIOLATED: writing to String cannot fail");
    Ok(())
}

fn method_navigation_fallback_reason(method: &MemberInfo, class_kind: ClassKind) -> &'static str {
    if method.is_abstract() {
        "abstract JVM method has no implementation body"
    } else if method.is_native() {
        "native JVM method has no classfile implementation body"
    } else if matches!(class_kind, ClassKind::Interface | ClassKind::Annotation) {
        "interface or annotation method has no mappable implementation body"
    } else {
        "no verified exact-source or bounded-decompiler member range was available"
    }
}

fn write_method_docs(
    output: &mut String,
    method: &MemberInfo,
    descriptor: &MethodDescriptor,
    indent: usize,
) {
    for (index, parameter) in descriptor.parameters.iter().enumerate() {
        let name = method
            .parameters
            .get(index)
            .map(|parameter| ruby_parameter_name(&parameter.name, index))
            .unwrap_or_else(|| format!("arg{index}"));
        writeln!(
            output,
            "{:indent$}# @param {name} [{}]",
            "",
            ruby_type_for_jvm_type(parameter)
        )
        .expect("INVARIANT VIOLATED: writing to String cannot fail");
    }
    writeln!(
        output,
        "{:indent$}# @return [{}]",
        "",
        ruby_type_for_jvm_type(&descriptor.returns)
    )
    .expect("INVARIANT VIOLATED: writing to String cannot fail");
    if !method.exceptions.is_empty() {
        writeln!(
            output,
            "{:indent$}# @raise [{}]",
            "",
            method
                .exceptions
                .iter()
                .filter_map(|exception| JavaClassName::parse(exception).ok())
                .map(|exception| exception.ruby_fqn())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .expect("INVARIANT VIOLATED: writing to String cannot fail");
    }
}

pub fn ruby_type_for_jvm_type(ty: &JvmType) -> String {
    match ty {
        JvmType::Byte | JvmType::Char | JvmType::Int | JvmType::Long | JvmType::Short => {
            "Integer".to_string()
        }
        JvmType::Double | JvmType::Float => "Float".to_string(),
        JvmType::Boolean => "Boolean".to_string(),
        JvmType::Void => "nil".to_string(),
        JvmType::Object(name) => JavaClassName::parse(name)
            .map(|name| name.ruby_fqn())
            .unwrap_or_else(|_| "Object".to_string()),
        JvmType::Array(element) => format!("Array<{}>", ruby_type_for_jvm_type(element)),
    }
}

fn java_method_name(method: &MemberInfo) -> &str {
    if method.name == "<init>" {
        "new"
    } else {
        &method.name
    }
}

fn visibility_keyword(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private | Visibility::Package => "private",
    }
}

fn valid_ruby_method_name(name: &str) -> bool {
    let name = name
        .strip_suffix('=')
        .or_else(|| name.strip_suffix('?'))
        .or_else(|| name.strip_suffix('!'))
        .unwrap_or(name);
    valid_ruby_local_name(name)
}

fn valid_ruby_local_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_lowercase() || first == '_')
        && chars.all(|character| character.is_alphanumeric() || character == '_')
        && !is_ruby_keyword(name)
}

pub fn ruby_parameter_name(name: &str, index: usize) -> String {
    if valid_ruby_local_name(name) {
        name.to_string()
    } else {
        format!("arg{index}")
    }
}

fn is_ruby_keyword(name: &str) -> bool {
    matches!(
        name,
        "BEGIN"
            | "END"
            | "__ENCODING__"
            | "__FILE__"
            | "__LINE__"
            | "alias"
            | "and"
            | "begin"
            | "break"
            | "case"
            | "class"
            | "def"
            | "defined?"
            | "do"
            | "else"
            | "elsif"
            | "end"
            | "ensure"
            | "false"
            | "for"
            | "if"
            | "in"
            | "module"
            | "next"
            | "nil"
            | "not"
            | "or"
            | "redo"
            | "rescue"
            | "retry"
            | "return"
            | "self"
            | "super"
            | "then"
            | "true"
            | "undef"
            | "unless"
            | "until"
            | "when"
            | "while"
            | "yield"
    )
}

fn valid_ruby_constant_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_uppercase())
        && name
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_fast_lsp_jvm_metadata::{parse_class, ClassLimits};

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(
                    std::str::from_utf8(pair).expect("fixture hex must be ASCII"),
                    16,
                )
                .expect("fixture byte must be valid hex")
            })
            .collect()
    }

    #[test]
    fn generates_deterministic_ruby_declarations_from_metadata_only() {
        let class = parse_class(
            &decode_hex(include_str!(
                "../../jvm-metadata/fixtures/overloads.class.hex"
            )),
            ClassLimits::default(),
        )
        .expect("checked overload fixture must parse");
        let source = generate_ruby_signature(&class)
            .expect("checked overload metadata must generate a Ruby signature");
        assert!(source.contains("class Overloads < Java::JavaLang::Object"));
        assert_eq!(source.matches("def value(").count(), 2);
        assert!(source.contains("protected\n"));
        assert!(source.contains("def self.nativeValue(input); end"));
        assert!(source
            .contains("declaration-only: native JVM method has no classfile implementation body."));
        assert!(source.contains(
            "declaration-only: no verified exact-source or bounded-decompiler member range was available."
        ));
        assert!(!source.contains("java_import"));
    }

    #[test]
    fn generated_parameter_names_are_valid_ruby_locals() {
        let mut class = parse_class(
            &decode_hex(include_str!(
                "../../jvm-metadata/fixtures/overloads.class.hex"
            )),
            ClassLimits::default(),
        )
        .expect("checked overload fixture must parse");
        let parameterized_methods = class
            .methods
            .iter()
            .enumerate()
            .filter_map(|(index, method)| (!method.parameters.is_empty()).then_some(index))
            .collect::<Vec<_>>();
        assert!(
            parameterized_methods.len() >= 2,
            "checked overload fixture must contain two parameterized methods"
        );
        class.methods[parameterized_methods[0]].parameters[0].name = "module".to_string();
        class.methods[parameterized_methods[1]].parameters[0].name = "ID".to_string();

        let source = generate_ruby_signature(&class)
            .expect("Java parameter metadata must generate valid Ruby");
        assert!(!source.contains("(module"));
        assert!(!source.contains("@param module"));
        assert!(!source.contains("(ID"));
        assert!(!source.contains("@param ID"));
        assert!(source.contains("(arg0"));
        assert!(source.contains("@param arg0"));
    }
}
