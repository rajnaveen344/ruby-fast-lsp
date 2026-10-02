//! Seed type facts: constant value lookup, assignment types, and literal types.

use crate::core::{
    FullyQualifiedName, RubyConstant, RubyType, TypeFact, TypeProvenance, TypeSubject,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::{DefNode, Node};

use super::syntax::{constant_parts, constant_parts_and_absolute, constant_path_parts};
use super::AnalysisIndexer;
use crate::indexer::documents::scope_rules::{declaration_candidates, lexical_candidates};
use crate::inference::method::constructor::seed_constructor_type;
use crate::inference::r#type::literal::{infer_array_literal_type, infer_hash_literal_type};

impl AnalysisIndexer {
    fn resolve_constant_value_type_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<RubyType> {
        lexical_candidates(parts, absolute, lexical_context)
            .find_map(|candidate| self.constant_value_type(candidate))
    }

    pub(super) fn resolve_declaration_constant_value_type_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<RubyType> {
        declaration_candidates(parts, absolute, lexical_context)
            .into_iter()
            .find_map(|candidate| self.constant_value_type(candidate))
    }

    fn constant_value_type(&self, parts: Vec<RubyConstant>) -> Option<RubyType> {
        let constant = FullyQualifiedName::constant(parts);
        let subject = TypeSubject::Constant(constant.clone());
        self.facts
            .types
            .iter()
            .rev()
            .find(|fact| fact.subject == subject)
            .map(|fact| fact.ruby_type.clone())
            .or_else(|| self.known_constant_types.get(&constant).cloned())
    }

    pub(super) fn assignment_type(&self, node: &Node<'_>) -> Option<RubyType> {
        // Collection elements need the same lexical value lookup as a direct
        // assignment. Syntax alone cannot distinguish a Symbol-valued constant
        // from a class object, including inside nested or frozen collections.
        if let Some(array) = node.as_array_node() {
            return Some(infer_array_literal_type(&array, |element| {
                self.assignment_type(element).unwrap_or(RubyType::Unknown)
            }));
        }
        if let Some(hash) = node.as_hash_node() {
            return Some(
                infer_hash_literal_type(&hash, |value| {
                    self.assignment_type(value).unwrap_or(RubyType::Unknown)
                })
                .unwrap_or(RubyType::Unknown),
            );
        }
        if let Some(call) = node.as_call_node() {
            if call.name().as_slice() == b"freeze" && call.arguments().is_none() {
                return call
                    .receiver()
                    .and_then(|receiver| self.assignment_type(&receiver));
            }
            if call.name().as_slice() == b"new" {
                let receiver = call.receiver()?;
                let (parts, absolute) = constant_parts_and_absolute(&receiver)?;
                return match self.resolve_constant_value_type_from(
                    &parts,
                    absolute,
                    &self.lexical_stack,
                ) {
                    Some(RubyType::ClassReference(target)) => seed_constructor_type(&target),
                    Some(
                        RubyType::Class(_)
                        | RubyType::Module(_)
                        | RubyType::ModuleReference(_)
                        | RubyType::Literal(_)
                        | RubyType::Array(_)
                        | RubyType::Hash(_, _)
                        | RubyType::Shape(_)
                        | RubyType::Union(_)
                        | RubyType::Unknown,
                    )
                    | None => literal_type(node),
                };
            }
        }

        if let Some((parts, absolute)) = constant_parts_and_absolute(node) {
            return self.resolve_constant_value_type_from(&parts, absolute, &self.lexical_stack);
        }

        literal_type(node)
    }

    pub(super) fn push_type_fact(
        &mut self,
        subject: TypeSubject,
        ruby_type: Option<RubyType>,
        location: ruby_prism::Location<'_>,
    ) {
        let ruby_type = ruby_type.unwrap_or(RubyType::Unknown);
        // Constant equations need an exact file-owned target even before a
        // referenced declaration becomes available. Unknown is a placeholder
        // for that solve, never a guessed class object derived from syntax.
        if ruby_type == RubyType::Unknown && !matches!(subject, TypeSubject::Constant(_)) {
            return;
        }
        self.facts.types.push(TypeFact::new(
            subject,
            ruby_type,
            self.range(&location),
            TypeProvenance::Assignment,
        ));
    }
}

pub(super) fn literal_type(node: &Node<'_>) -> Option<RubyType> {
    if let Some(call) = node.as_call_node() {
        if call.name().as_slice() == b"freeze" && call.arguments().is_none() {
            return call.receiver().and_then(|receiver| literal_type(&receiver));
        }
        if call.name().as_slice() == b"new" {
            let receiver = call.receiver()?;
            let parts = constant_parts(&receiver)?;
            return seed_constructor_type(&FullyQualifiedName::constant(parts));
        }
    }
    if let Some(read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        let constant = RubyConstant::new(&name).ok()?;
        return Some(RubyType::ClassReference(FullyQualifiedName::constant(
            vec![constant],
        )));
    }
    if let Some(path) = node.as_constant_path_node() {
        let parts = constant_path_parts(&path)?;
        return Some(RubyType::ClassReference(FullyQualifiedName::constant(
            parts,
        )));
    }
    if node.as_string_node().is_some() || node.as_interpolated_string_node().is_some() {
        return Some(RubyType::string());
    }
    if node.as_integer_node().is_some() {
        return Some(RubyType::integer());
    }
    if node.as_float_node().is_some() {
        return Some(RubyType::float());
    }
    if node.as_symbol_node().is_some() || node.as_interpolated_symbol_node().is_some() {
        return Some(RubyType::symbol());
    }
    if node.as_true_node().is_some() {
        return Some(RubyType::true_class());
    }
    if node.as_false_node().is_some() {
        return Some(RubyType::false_class());
    }
    if node.as_nil_node().is_some() {
        return Some(RubyType::nil_class());
    }
    if node.as_regular_expression_node().is_some()
        || node.as_interpolated_regular_expression_node().is_some()
    {
        return Some(RubyType::Class(
            FullyQualifiedName::try_from("Regexp").expect_invariant(
                "Regexp is not a valid Ruby constant",
                "it is a language-defined literal type",
                "preserve the canonical constant spelling",
            ),
        ));
    }
    if let Some(array) = node.as_array_node() {
        let element_types = array
            .elements()
            .iter()
            .map(|element| literal_type(&element).unwrap_or(RubyType::Unknown))
            .collect::<Vec<_>>();
        return Some(RubyType::Array(RubyType::canonical_union_members(
            element_types,
        )));
    }
    if let Some(hash) = node.as_hash_node() {
        let mut key_types = Vec::new();
        let mut value_types = Vec::new();
        for element in hash.elements().iter() {
            if let Some(assoc) = element.as_assoc_node() {
                key_types.push(literal_type(&assoc.key()).unwrap_or(RubyType::Unknown));
                value_types.push(literal_type(&assoc.value()).unwrap_or(RubyType::Unknown));
            } else {
                key_types.push(RubyType::Unknown);
                value_types.push(RubyType::Unknown);
            }
        }
        return Some(RubyType::Hash(
            RubyType::canonical_union_members(key_types),
            RubyType::canonical_union_members(value_types),
        ));
    }
    None
}

pub(super) fn method_body_literal_type(node: &DefNode<'_>) -> Option<RubyType> {
    let body = node.body()?;
    if let Some(statements) = body.as_statements_node() {
        let last = statements.body().iter().last()?;
        return literal_type(&last);
    }
    literal_type(&body)
}
