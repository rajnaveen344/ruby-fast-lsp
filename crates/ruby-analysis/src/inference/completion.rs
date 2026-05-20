use crate::core::{FullyQualifiedName, RubyConstant};
use crate::inference::RubyType;

pub fn infer_constructor_assignment_type(content: &str, var_name: &str) -> Option<RubyType> {
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(var_name) else {
            continue;
        };

        let next_char = rest.chars().next();
        if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
            continue;
        }

        let rest = rest.trim();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rhs = rest.trim();
        if !(rhs.ends_with(".new") || rhs.contains(".new(") || rhs.contains(".new ")) {
            continue;
        }

        let new_pos = rhs.find(".new")?;
        let class_part = rhs[..new_pos].trim();
        if !class_part
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            continue;
        }

        let parts: Vec<_> = class_part
            .split("::")
            .filter_map(|s| RubyConstant::new(s.trim()).ok())
            .collect();

        if !parts.is_empty() {
            return Some(RubyType::Class(FullyQualifiedName::Constant(parts)));
        }
    }

    None
}

pub fn infer_literal_type_from_expression(text: &str) -> Option<RubyType> {
    let trimmed = text.trim();

    if trimmed.ends_with('"') || trimmed.ends_with('\'') {
        return Some(RubyType::string());
    }

    if trimmed.ends_with(']') && trimmed.starts_with('[') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let element_types = infer_array_element_types(inner);
        return Some(RubyType::Array(element_types));
    }

    if trimmed.ends_with('}') {
        return Some(RubyType::Hash(
            vec![RubyType::Unknown],
            vec![RubyType::Unknown],
        ));
    }

    if let Some(rest) = trimmed.rsplit_once(|c: char| c.is_whitespace() || c == '(' || c == ',') {
        if rest.1.starts_with(':') {
            return Some(RubyType::symbol());
        }
    } else if trimmed.starts_with(':') {
        return Some(RubyType::symbol());
    }

    None
}

pub fn infer_literal_type(text: &str) -> Option<RubyType> {
    if text.starts_with('"') || text.starts_with('\'') {
        return Some(RubyType::string());
    }

    if text.starts_with(':') {
        return Some(RubyType::symbol());
    }

    if text.starts_with('[') {
        return Some(RubyType::Array(vec![RubyType::Unknown]));
    }

    if text.starts_with('{') {
        return Some(RubyType::Hash(
            vec![RubyType::Unknown],
            vec![RubyType::Unknown],
        ));
    }

    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return Some(RubyType::integer());
    }

    if text.contains('.')
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
    {
        return Some(RubyType::float());
    }

    None
}

pub fn is_variable_name(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }

    let first_char = text.chars().next().expect(
        "INVARIANT VIOLATED: non-empty variable text has no first char. \
         This is a bug because Rust str chars must yield at least one char for non-empty valid UTF-8. \
         Fix: check caller input encoding.",
    );
    if !first_char.is_lowercase() && first_char != '_' {
        return false;
    }

    text.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn infer_array_element_types(inner: &str) -> Vec<RubyType> {
    let mut types = Vec::new();
    for element in inner.split(',') {
        let el = element.trim();
        if el.is_empty() {
            continue;
        }
        let ty = if el.starts_with('"') || el.starts_with('\'') {
            RubyType::string()
        } else if el.starts_with(':') {
            RubyType::symbol()
        } else if el.parse::<i64>().is_ok() {
            RubyType::integer()
        } else if el.parse::<f64>().is_ok() {
            RubyType::float()
        } else if el == "true" || el == "false" {
            RubyType::true_class()
        } else if el == "nil" {
            RubyType::nil_class()
        } else {
            RubyType::Unknown
        };
        if ty != RubyType::Unknown && !types.contains(&ty) {
            types.push(ty);
        }
    }
    if types.is_empty() {
        vec![RubyType::Unknown]
    } else {
        types
    }
}
