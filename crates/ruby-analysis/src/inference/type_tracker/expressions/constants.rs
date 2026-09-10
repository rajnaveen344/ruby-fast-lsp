use crate::core::{FullyQualifiedName, RubyConstant, RubyType, UnknownReason};
use crate::engine::AnalysisQuery;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;

impl TypeTracker {
    pub(in crate::inference::type_tracker) fn constant_value_type(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<RubyType> {
        let analysis_engine = self.analysis.engine.as_ref()?;
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let constant = if absolute {
            FullyQualifiedName::constant(parts.to_vec())
        } else {
            let lexical_context = self
                .context
                .class
                .as_ref()
                .map(FullyQualifiedName::namespace_parts)
                .unwrap_or_default();
            let resolved = query.resolve_constant_in_context(parts, &lexical_context)?;
            FullyQualifiedName::constant(resolved.namespace_parts())
        };
        query
            .constant_value_type(&constant)
            .or_else(|| query.constant_reference_type(constant.namespace_parts_slice()))
    }

    pub(in crate::inference::type_tracker) fn constant_callable_body_for_node(
        &self,
        node: &Node<'_>,
    ) -> Option<Result<crate::core::CallableBodySummary, UnknownReason>> {
        let (parts, absolute) = Self::constant_reference(node)?;
        let analysis_engine = self.analysis.engine.as_ref()?;
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let constant = if absolute {
            FullyQualifiedName::constant(parts)
        } else {
            let lexical_context = self
                .context
                .class
                .as_ref()
                .map(FullyQualifiedName::namespace_parts)
                .unwrap_or_default();
            let resolved = query.resolve_constant_in_context(&parts, &lexical_context)?;
            FullyQualifiedName::constant(resolved.namespace_parts())
        };
        query.constant_callable_body(&constant)
    }

    pub(in crate::inference::type_tracker) fn constant_reference(
        node: &Node<'_>,
    ) -> Option<(Vec<RubyConstant>, bool)> {
        if let Some(constant_read) = node.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read.name().as_slice());
            return Some((vec![RubyConstant::new(name.as_ref()).ok()?], false));
        }

        let constant_path = node.as_constant_path_node()?;
        let fqn = Self::resolve_constant_path(&constant_path)?;
        let absolute = Self::constant_path_is_absolute(&constant_path);
        Some((fqn.namespace_parts(), absolute))
    }

    pub(in crate::inference::type_tracker) fn constant_path_is_absolute(
        constant_path: &ConstantPathNode<'_>,
    ) -> bool {
        match constant_path.parent() {
            None => true,
            Some(parent) => parent
                .as_constant_path_node()
                .is_some_and(|parent| Self::constant_path_is_absolute(&parent)),
        }
    }

    /// Resolve a constant path to an FQN (e.g., Foo::Bar::Baz)
    pub(in crate::inference::type_tracker) fn resolve_constant_path(
        const_path: &ConstantPathNode,
    ) -> Option<FullyQualifiedName> {
        let mut parts = Vec::new();

        // Get the child constant name
        if let Some(name_node) = const_path.name() {
            let name = String::from_utf8_lossy(name_node.as_slice()).to_string();
            parts.push(RubyConstant::new(&name).ok()?);
        }

        // Get parent parts recursively
        if let Some(parent) = const_path.parent() {
            if let Some(parent_path) = parent.as_constant_path_node() {
                if let Some(FullyQualifiedName::Constant(parent_parts)) =
                    Self::resolve_constant_path(&parent_path)
                {
                    let mut full_parts = parent_parts;
                    full_parts.extend(parts);
                    return Some(FullyQualifiedName::constant(full_parts));
                }
            } else if let Some(const_read) = parent.as_constant_read_node() {
                let parent_name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
                let mut full_parts = vec![RubyConstant::new(&parent_name).ok()?];
                full_parts.extend(parts);
                return Some(FullyQualifiedName::constant(full_parts));
            }
        } else {
            // No parent means this is a top-level constant
            return Some(FullyQualifiedName::constant(parts));
        }

        None
    }
}
