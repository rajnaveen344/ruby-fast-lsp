use crate::core::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeTarget, FullyQualifiedName,
    GraphNodeKind, RubyConstant, RubyType, TextRange, TypeSubject,
};
use crate::indexer::fact_collector::FactCollector;
use ruby_prism::*;

#[derive(Default)]
pub(in crate::indexer::fact_collector) struct ConstantEvidence {
    pub(in crate::indexer::fact_collector) equations: Vec<ConstantTypeEquation>,
    pub(in crate::indexer::fact_collector) callable_bodies:
        Vec<crate::core::ConstantCallableBodyFact>,
}

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn constant_reference_type(
        &self,
        node: &Node,
    ) -> Option<FullyQualifiedName> {
        let reference = crate::indexer::mixin_ref_from_node(node)?;
        if let Some(namespace) = self.direct_resolve_namespace(&reference.parts, reference.absolute)
        {
            return Some(namespace);
        }
        let lexical_context = self.scope_tracker.get_ns_stack();
        let engine = self.semantics.engine.read();
        if let Some(resolved) = crate::engine::AnalysisQuery::new(&engine)
            .resolve_constant_in_context(&reference.parts, &lexical_context)
        {
            return Some(resolved);
        }
        let mut parts = if reference.absolute {
            Vec::new()
        } else {
            lexical_context
        };
        parts.extend(reference.parts);
        Some(FullyQualifiedName::constant(parts))
    }

    pub(in crate::indexer::fact_collector) fn constant_type_dependency(
        &self,
        node: &Node<'_>,
    ) -> Option<ConstantTypeDependency> {
        if let Some(call) = node.as_call_node() {
            if call.name().as_slice() == b"new" {
                let receiver = call.receiver()?;
                let reference = crate::indexer::mixin_ref_from_node(&receiver)?;
                return Some(ConstantTypeDependency::constructor(
                    reference.parts,
                    reference.absolute,
                    self.scope_tracker.get_ns_stack(),
                ));
            }
        }
        let reference = crate::indexer::mixin_ref_from_node(node)?;
        Some(ConstantTypeDependency::new(
            reference.parts,
            reference.absolute,
            self.scope_tracker.get_ns_stack(),
        ))
    }

    pub(in crate::indexer::fact_collector) fn push_constant_type_equation(
        &mut self,
        subject: TypeSubject,
        range: TextRange,
        dependency: ConstantTypeDependency,
    ) {
        let equation = ConstantTypeEquation::dependency(
            ConstantTypeTarget::Fact { subject, range },
            dependency,
        );
        if !self.constants.equations.contains(&equation) {
            self.constants.equations.push(equation);
        }
    }

    pub(in crate::indexer::fact_collector) fn push_constant_local_assignment_equation(
        &mut self,
        name: String,
        range: TextRange,
        dependency: ConstantTypeDependency,
    ) {
        let equation = ConstantTypeEquation::dependency(
            ConstantTypeTarget::LocalAssignment { name, range },
            dependency,
        );
        if !self.constants.equations.contains(&equation) {
            self.constants.equations.push(equation);
        }
    }

    pub(in crate::indexer::fact_collector) fn const_get_reference_type(
        &self,
        call: &CallNode<'_>,
    ) -> Option<RubyType> {
        let parts = self.const_get_target_parts(call)?;
        let constant_fqn = FullyQualifiedName::constant(parts.clone());
        if let Some(value_type) = self.direct_constant_value_type(&constant_fqn) {
            return Some(value_type);
        }

        let namespace_fqn = FullyQualifiedName::namespace(parts.clone());
        if let Some(kind) = self
            .facts
            .direct
            .graph_nodes
            .iter()
            .filter(|fact| fact.fqn == namespace_fqn)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.kind)
        {
            return Some(match kind {
                GraphNodeKind::Class => RubyType::ClassReference(constant_fqn),
                GraphNodeKind::Module => RubyType::ModuleReference(constant_fqn),
            });
        }

        let engine = self.semantics.engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);
        query
            .constant_value_type(&constant_fqn)
            .or_else(|| query.constant_reference_type(&parts))
            .or_else(|| Some(RubyType::ClassReference(constant_fqn)))
    }

    pub(in crate::indexer::fact_collector) fn direct_constant_value_type(
        &self,
        constant_fqn: &FullyQualifiedName,
    ) -> Option<RubyType> {
        let direct = self
            .facts
            .direct
            .types
            .iter()
            .filter(|fact| match &fact.subject {
                TypeSubject::Constant(fqn) => {
                    fqn == constant_fqn && fact.ruby_type != RubyType::Unknown
                }
                TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => false,
            })
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            });
        let subject = TypeSubject::Constant(constant_fqn.clone());
        let stored = self
            .facts
            .types
            .latest_non_unknown_type_with_range(&subject);

        match (direct, stored) {
            (None, None) => None,
            (Some(fact), None) => Some(fact.ruby_type.clone()),
            (None, Some((ruby_type, _))) => Some(ruby_type.clone()),
            (Some(fact), Some((ruby_type, range))) => {
                let direct_key = (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                );
                let stored_key = (range.file_id, range.start_byte, range.end_byte);
                if direct_key > stored_key {
                    Some(fact.ruby_type.clone())
                } else {
                    Some(ruby_type.clone())
                }
            }
        }
    }

    pub(in crate::indexer::fact_collector) fn const_get_target_parts(
        &self,
        call: &CallNode<'_>,
    ) -> Option<Vec<RubyConstant>> {
        if call.name().as_slice() != b"const_get" {
            return None;
        }
        self.const_lookup_target_parts(call)
    }

    pub(in crate::indexer::fact_collector) fn const_lookup_target_parts(
        &self,
        call: &CallNode<'_>,
    ) -> Option<Vec<RubyConstant>> {
        if !matches!(call.name().as_slice(), b"const_get" | b"const_defined?") {
            return None;
        }
        let arguments = call.arguments()?;
        let arg = arguments.arguments().iter().next()?;
        let constant = const_get_arg_constant(&arg)?;
        let mut parts = match call.receiver() {
            Some(receiver) if receiver.as_self_node().is_some() => {
                self.scope_tracker.get_ns_stack()
            }
            Some(receiver) => self.const_lookup_base_parts(&receiver)?,
            None => self.scope_tracker.get_ns_stack(),
        };
        parts.push(constant);
        Some(parts)
    }

    pub(in crate::indexer::fact_collector) fn const_lookup_base_parts(
        &self,
        receiver: &Node<'_>,
    ) -> Option<Vec<RubyConstant>> {
        if let Some(parts) = receiver
            .as_call_node()
            .and_then(|call| self.const_lookup_target_parts(&call))
        {
            return Some(parts);
        }

        let receiver_ref = crate::indexer::mixin_ref_from_node(receiver)?;
        if let Some(fqn) = self.direct_resolve_namespace(&receiver_ref.parts, receiver_ref.absolute)
        {
            return Some(fqn.namespace_parts());
        }

        let context = if receiver_ref.absolute {
            Vec::new()
        } else {
            self.scope_tracker.get_ns_stack()
        };
        let engine = self.semantics.engine.read();
        if let Some(fqn) = crate::engine::AnalysisQuery::new(&engine)
            .resolve_constant_in_context(&receiver_ref.parts, &context)
        {
            return Some(fqn.namespace_parts());
        }

        Some(receiver_ref.parts)
    }
}

pub(in crate::indexer::fact_collector) fn const_get_arg_constant(
    arg: &Node<'_>,
) -> Option<RubyConstant> {
    if let Some(symbol) = arg.as_symbol_node() {
        let name = String::from_utf8_lossy(symbol.unescaped()).to_string();
        return RubyConstant::new(&name).ok();
    }
    if let Some(string) = arg.as_string_node() {
        let name = String::from_utf8_lossy(string.unescaped()).to_string();
        return RubyConstant::new(&name).ok();
    }
    None
}
