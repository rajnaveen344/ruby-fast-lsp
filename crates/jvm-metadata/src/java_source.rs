use crate::{parse_method_descriptor, ClassFile, JvmType, MemberInfo};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceByteRange {
    pub start: u32,
    pub end: u32,
}

impl SourceByteRange {
    pub fn new(start: usize, end: usize) -> Self {
        assert!(
            start <= end,
            "INVARIANT VIOLATED: Java source range start exceeds its end. \
             This is a bug because tree-sitter nodes always expose normalized byte ranges. \
             Fix: preserve node start/end ordering when converting source locations."
        );
        Self {
            start: u32::try_from(start).expect(
                "INVARIANT VIOLATED: Java source byte offset exceeded u32. \
                 This is a bug because bounded source parsing must reject files before range conversion. \
                 Fix: enforce JavaSourceLimits::max_source_bytes before constructing ranges.",
            ),
            end: u32::try_from(end).expect(
                "INVARIANT VIOLATED: Java source byte offset exceeded u32. \
                 This is a bug because bounded source parsing must reject files before range conversion. \
                 Fix: enforce JavaSourceLimits::max_source_bytes before constructing ranges.",
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaSourceMemberLocation {
    pub name: String,
    pub descriptor: String,
    pub declaration_range: SourceByteRange,
    pub name_range: SourceByteRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaSourceClassLocation {
    pub internal_name: String,
    pub declaration_range: SourceByteRange,
    pub name_range: SourceByteRange,
    pub methods: Vec<JavaSourceMemberLocation>,
    pub fields: Vec<JavaSourceMemberLocation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JavaSourceLimits {
    pub max_source_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
}

impl Default for JavaSourceLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_nodes: 1_000_000,
            max_depth: 512,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaSourceError {
    LimitExceeded(&'static str),
    ParseFailed,
}

pub fn locate_java_source_declarations(
    class: &ClassFile,
    source: &str,
    limits: JavaSourceLimits,
) -> Result<Option<JavaSourceClassLocation>, JavaSourceError> {
    if source.len() > limits.max_source_bytes || source.len() > u32::MAX as usize {
        return Err(JavaSourceError::LimitExceeded("Java source bytes"));
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect(
            "INVARIANT VIOLATED: bundled tree-sitter Java grammar failed to load. \
             This is a build/configuration bug because the grammar and parser versions are pinned together. \
             Fix: keep tree-sitter and tree-sitter-java ABI-compatible.",
        );
    let tree = parser
        .parse(source, None)
        .ok_or(JavaSourceError::ParseFailed)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(JavaSourceError::ParseFailed);
    }
    enforce_tree_limits(root, limits)?;

    let (expected_package, expected_classes) = split_internal_class_name(&class.name)?;
    let package = source_package(root, source);
    if package.as_deref() != Some(expected_package.as_str()) {
        return Ok(None);
    }
    let Some(class_node) = find_class_node(root, source, &expected_classes, &mut Vec::new()) else {
        return Ok(None);
    };
    let name_node = class_node.child_by_field_name("name").expect(
        "INVARIANT VIOLATED: recognized Java type declaration has no name field. \
         This is a tree-sitter grammar contract violation. Fix: update Java source mapping for the pinned grammar.",
    );
    let method_nodes = direct_children_of_kinds(
        class_node.child_by_field_name("body").expect(
            "INVARIANT VIOLATED: recognized Java type declaration has no body field. \
             This is a tree-sitter grammar contract violation. Fix: update Java source mapping for the pinned grammar.",
        ),
        &[
            "method_declaration",
            "constructor_declaration",
            "compact_constructor_declaration",
        ],
    );
    let field_nodes = direct_children_of_kinds(
        class_node.child_by_field_name("body").expect(
            "INVARIANT VIOLATED: recognized Java type declaration has no body field. \
             This is a tree-sitter grammar contract violation. Fix: update Java source mapping for the pinned grammar.",
        ),
        &["field_declaration", "enum_constant"],
    );

    let methods = class
        .methods
        .iter()
        .filter_map(|method| locate_method(method, &method_nodes, source))
        .collect::<Vec<_>>();
    let fields = class
        .fields
        .iter()
        .filter_map(|field| locate_field(field, &field_nodes, source))
        .collect::<Vec<_>>();
    Ok(Some(JavaSourceClassLocation {
        internal_name: class.name.clone(),
        declaration_range: node_range(class_node),
        name_range: node_range(name_node),
        methods,
        fields,
    }))
}

fn enforce_tree_limits(root: Node<'_>, limits: JavaSourceLimits) -> Result<(), JavaSourceError> {
    let mut stack = vec![(root, 0usize)];
    let mut nodes = 0usize;
    while let Some((node, depth)) = stack.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or(JavaSourceError::LimitExceeded("Java source nodes"))?;
        if nodes > limits.max_nodes {
            return Err(JavaSourceError::LimitExceeded("Java source nodes"));
        }
        if depth > limits.max_depth {
            return Err(JavaSourceError::LimitExceeded("Java source depth"));
        }
        let mut cursor = node.walk();
        let children = node.children(&mut cursor).collect::<Vec<_>>();
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    Ok(())
}

fn split_internal_class_name(name: &str) -> Result<(String, Vec<String>), JavaSourceError> {
    let (package, classes) = match name.rsplit_once('/') {
        Some((package, classes)) => (package.replace('/', "."), classes),
        None => (String::new(), name),
    };
    let classes = classes.split('$').map(str::to_string).collect::<Vec<_>>();
    if classes.is_empty()
        || classes
            .iter()
            .any(|name| name.is_empty() || name.chars().all(|character| character.is_ascii_digit()))
    {
        return Err(JavaSourceError::ParseFailed);
    }
    Ok((package, classes))
}

fn source_package(root: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "package_declaration" {
            continue;
        }
        let text = child.utf8_text(source.as_bytes()).ok()?;
        return text
            .strip_prefix("package")
            .and_then(|text| text.strip_suffix(';'))
            .map(str::trim)
            .map(str::to_string);
    }
    Some(String::new())
}

fn find_class_node<'tree>(
    node: Node<'tree>,
    source: &str,
    expected: &[String],
    stack: &mut Vec<String>,
) -> Option<Node<'tree>> {
    let is_type = is_type_declaration(node.kind());
    if is_type {
        let name = node.child_by_field_name("name")?;
        let name = name.utf8_text(source.as_bytes()).ok()?.to_string();
        stack.push(name);
        if stack == expected {
            return Some(node);
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(found) = find_class_node(child, source, expected, stack) {
            return Some(found);
        }
    }
    if is_type {
        stack.pop().expect(
            "INVARIANT VIOLATED: Java type traversal stack underflowed. \
             This is a bug because every recognized type pushes exactly once before recursion. \
             Fix: keep find_class_node push/pop paths balanced.",
        );
    }
    None
}

fn is_type_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

fn direct_children_of_kinds<'tree>(node: Node<'tree>, kinds: &[&str]) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| kinds.contains(&child.kind()))
        .collect()
}

fn locate_method(
    method: &MemberInfo,
    nodes: &[Node<'_>],
    source: &str,
) -> Option<JavaSourceMemberLocation> {
    if method.name == "<clinit>" {
        return None;
    }
    let descriptor = parse_method_descriptor(&method.descriptor).ok()?;
    let expected_name = if method.name == "<init>" {
        None
    } else {
        Some(method.name.as_str())
    };
    let mut candidates = nodes
        .iter()
        .filter_map(|node| {
            let is_constructor = matches!(
                node.kind(),
                "constructor_declaration" | "compact_constructor_declaration"
            );
            if is_constructor != (method.name == "<init>") {
                return None;
            }
            let name = node.child_by_field_name("name")?;
            let name_text = name.utf8_text(source.as_bytes()).ok()?;
            if expected_name.is_some_and(|expected| expected != name_text) {
                return None;
            }
            if node_is_static(*node, source) != method.is_static() {
                return None;
            }
            let parameter_types = source_parameter_types(*node, source)?;
            if parameter_types.len() != descriptor.parameters.len() {
                return None;
            }
            Some((*node, name, parameter_types))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    if let Some(line) = method.first_line {
        let line = usize::from(line);
        let line_matches = candidates
            .iter()
            .filter(|(node, _, _)| {
                node.start_position().row + 1 <= line && line <= node.end_position().row + 1
            })
            .cloned()
            .collect::<Vec<_>>();
        if line_matches.len() == 1 {
            candidates = line_matches;
        }
    }
    if candidates.len() > 1 {
        let type_matches = candidates
            .iter()
            .filter(|(_, _, source_types)| {
                source_types_match_descriptor(source_types, &descriptor.parameters)
            })
            .cloned()
            .collect::<Vec<_>>();
        if type_matches.len() == 1 {
            candidates = type_matches;
        }
    }
    if candidates.len() != 1 {
        return None;
    }
    let (declaration, name, _) = candidates[0].clone();
    Some(JavaSourceMemberLocation {
        name: method.name.clone(),
        descriptor: method.descriptor.clone(),
        declaration_range: node_range(declaration),
        name_range: node_range(name),
    })
}

fn locate_field(
    field: &MemberInfo,
    nodes: &[Node<'_>],
    source: &str,
) -> Option<JavaSourceMemberLocation> {
    let mut matches = Vec::new();
    for node in nodes {
        if node.kind() == "enum_constant" {
            let Some(name) = node.child_by_field_name("name") else {
                continue;
            };
            if name.utf8_text(source.as_bytes()).ok()? == field.name {
                matches.push((*node, name));
            }
            continue;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() != "variable_declarator" {
                continue;
            }
            let Some(name) = child.child_by_field_name("name") else {
                continue;
            };
            if name.utf8_text(source.as_bytes()).ok()? == field.name {
                matches.push((*node, name));
            }
        }
    }
    if matches.len() != 1 {
        return None;
    }
    Some(JavaSourceMemberLocation {
        name: field.name.clone(),
        descriptor: field.descriptor.clone(),
        declaration_range: node_range(matches[0].0),
        name_range: node_range(matches[0].1),
    })
}

fn node_is_static(node: Node<'_>, source: &str) -> bool {
    let mut cursor = node.walk();
    let is_static = node.named_children(&mut cursor).any(|child| {
        child.kind() == "modifiers"
            && child
                .utf8_text(source.as_bytes())
                .is_ok_and(|text| text.split_whitespace().any(|word| word == "static"))
    });
    is_static
}

fn source_parameter_types(node: Node<'_>, source: &str) -> Option<Vec<String>> {
    if node.kind() == "compact_constructor_declaration" {
        return Some(Vec::new());
    }
    let parameters = node.child_by_field_name("parameters")?;
    let mut result = Vec::new();
    let mut cursor = parameters.walk();
    for parameter in parameters.named_children(&mut cursor) {
        if !matches!(
            parameter.kind(),
            "formal_parameter" | "spread_parameter" | "receiver_parameter"
        ) {
            continue;
        }
        let ty = parameter
            .child_by_field_name("type")
            .or_else(|| first_parameter_type_child(parameter))?;
        let mut text = ty.utf8_text(source.as_bytes()).ok()?.to_string();
        if parameter.kind() == "spread_parameter" {
            text.push_str("[]");
        }
        result.push(normalize_source_type(&text));
    }
    Some(result)
}

fn first_parameter_type_child(parameter: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = parameter.walk();
    let result = parameter.named_children(&mut cursor).find(|child| {
        matches!(
            child.kind(),
            "annotated_type"
                | "array_type"
                | "boolean_type"
                | "floating_point_type"
                | "generic_type"
                | "integral_type"
                | "scoped_type_identifier"
                | "type_identifier"
                | "void_type"
                | "wildcard"
        )
    });
    result
}

fn normalize_source_type(source: &str) -> String {
    let mut output = String::new();
    let mut generic_depth = 0usize;
    for character in source.chars() {
        match character {
            '<' => generic_depth += 1,
            '>' => generic_depth = generic_depth.saturating_sub(1),
            character if generic_depth > 0 || character.is_whitespace() => {}
            character => output.push(character),
        }
    }
    output
}

fn source_types_match_descriptor(source: &[String], descriptor: &[JvmType]) -> bool {
    source.len() == descriptor.len()
        && source
            .iter()
            .zip(descriptor)
            .all(|(source, descriptor)| source_type_matches(source, descriptor))
}

fn source_type_matches(source: &str, descriptor: &JvmType) -> bool {
    let expected = match descriptor {
        JvmType::Byte => "byte".to_string(),
        JvmType::Char => "char".to_string(),
        JvmType::Double => "double".to_string(),
        JvmType::Float => "float".to_string(),
        JvmType::Int => "int".to_string(),
        JvmType::Long => "long".to_string(),
        JvmType::Short => "short".to_string(),
        JvmType::Boolean => "boolean".to_string(),
        JvmType::Void => return false,
        JvmType::Object(name) => name.rsplit(['/', '$']).next().unwrap_or(name).to_string(),
        JvmType::Array(element) => format!("{}[]", descriptor_simple_name(element)),
    };
    source == expected
        || source
            .strip_suffix(&expected)
            .is_some_and(|prefix| prefix.ends_with('.') || prefix.ends_with('$'))
}

fn descriptor_simple_name(descriptor: &JvmType) -> String {
    match descriptor {
        JvmType::Byte => "byte".to_string(),
        JvmType::Char => "char".to_string(),
        JvmType::Double => "double".to_string(),
        JvmType::Float => "float".to_string(),
        JvmType::Int => "int".to_string(),
        JvmType::Long => "long".to_string(),
        JvmType::Short => "short".to_string(),
        JvmType::Boolean => "boolean".to_string(),
        JvmType::Void => "void".to_string(),
        JvmType::Object(name) => name.rsplit(['/', '$']).next().unwrap_or(name).to_string(),
        JvmType::Array(element) => format!("{}[]", descriptor_simple_name(element)),
    }
}

fn node_range(node: Node<'_>) -> SourceByteRange {
    SourceByteRange::new(node.start_byte(), node.end_byte())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_class, ClassLimits};

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

    fn text<'a>(source: &'a str, range: SourceByteRange) -> &'a str {
        &source[usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()]
    }

    #[test]
    fn maps_exact_class_constructor_methods_and_fields_from_source() {
        let class = parse_class(
            &decode_hex(include_str!("../fixtures/rich_fixture.class.hex")),
            ClassLimits::default(),
        )
        .expect("checked rich fixture must parse");
        let source = include_str!("../fixtures/sources/RichFixture.java");
        let location = locate_java_source_declarations(&class, source, JavaSourceLimits::default())
            .expect("checked Java source must parse")
            .expect("matching Java source must contain the class");
        assert_eq!(location.internal_name, "fixtures/RichFixture");
        assert_eq!(text(source, location.name_range), "RichFixture");
        assert!(
            text(source, location.declaration_range).contains("public class RichFixture"),
            "class declaration range must contain the exact source declaration"
        );

        for (name, descriptor, expected_text) in [
            ("<init>", "(Ljava/lang/Number;)V", "RichFixture"),
            (
                "combine",
                "(Ljava/lang/String;[I)Ljava/util/List;",
                "combine",
            ),
            ("run", "()V", "run"),
        ] {
            let member = location
                .methods
                .iter()
                .find(|member| member.name == name && member.descriptor == descriptor)
                .unwrap_or_else(|| panic!("missing exact method {name}{descriptor}"));
            assert_eq!(text(source, member.name_range), expected_text);
        }
        for (name, descriptor) in [("CONSTANT", "I"), ("value", "Ljava/lang/Number;")] {
            let field = location
                .fields
                .iter()
                .find(|field| field.name == name && field.descriptor == descriptor)
                .unwrap_or_else(|| panic!("missing exact field {name}:{descriptor}"));
            assert_eq!(text(source, field.name_range), name);
        }
    }

    #[test]
    fn distinguishes_overloads_by_erased_parameter_types() {
        let class = parse_class(
            &decode_hex(include_str!("../fixtures/overloads.class.hex")),
            ClassLimits::default(),
        )
        .expect("checked overload fixture must parse");
        let source = include_str!("../fixtures/sources/fixtures/Overloads.java");
        let location = locate_java_source_declarations(&class, source, JavaSourceLimits::default())
            .expect("checked Java source must parse")
            .expect("matching Java source must contain the class");
        let overloads = location
            .methods
            .iter()
            .filter(|member| member.name == "value")
            .collect::<Vec<_>>();
        assert_eq!(overloads.len(), 2);
        assert_eq!(
            overloads
                .iter()
                .map(|member| member.descriptor.as_str())
                .collect::<Vec<_>>(),
            vec![
                "(I)Ljava/lang/String;",
                "(Ljava/lang/String;)Ljava/lang/String;"
            ]
        );
        assert_ne!(
            overloads[0].declaration_range,
            overloads[1].declaration_range
        );
    }

    #[test]
    fn fails_closed_for_wrong_source_syntax_errors_and_limits() {
        let class = parse_class(
            &decode_hex(include_str!("../fixtures/overloads.class.hex")),
            ClassLimits::default(),
        )
        .expect("checked overload fixture must parse");
        assert_eq!(
            locate_java_source_declarations(
                &class,
                "package other; class Other {}",
                JavaSourceLimits::default()
            ),
            Ok(None)
        );
        assert_eq!(
            locate_java_source_declarations(
                &class,
                "package fixtures; class Overloads {",
                JavaSourceLimits::default()
            ),
            Err(JavaSourceError::ParseFailed)
        );
        assert_eq!(
            locate_java_source_declarations(
                &class,
                include_str!("../fixtures/sources/fixtures/Overloads.java"),
                JavaSourceLimits {
                    max_source_bytes: 8,
                    ..JavaSourceLimits::default()
                }
            ),
            Err(JavaSourceError::LimitExceeded("Java source bytes"))
        );
    }
}
