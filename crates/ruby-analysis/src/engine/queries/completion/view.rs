//! The engine view answers completion's semantic questions directly, so a
//! completion request reads them under the caller's one view.

use super::{CompletionSemanticQuery, CompletionVariableKind};
use crate::core::{
    FullyQualifiedName, RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange,
    VariableTypeKind,
};
use crate::engine::lookup::{self, LookupReceiver, MethodRequest, MethodWant};
use crate::engine::View;

impl CompletionSemanticQuery for View<'_> {
    fn constant_type_in_context(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<RubyType> {
        let resolved = self.resolve_constant_in_context(path, current_namespace)?;
        let constant = FullyQualifiedName::constant(resolved.namespace_parts().to_vec());
        View::constant_value_type(self, &constant)
            .or_else(|| View::constant_reference_type(self, resolved.namespace_parts_slice()))
    }

    fn method_return_type_for_receiver(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<RubyType> {
        let request = MethodRequest::new(
            LookupReceiver::Namespace(namespace),
            *method,
            MethodWant::Return,
        );
        lookup::method(self, request).into_return_type()
    }

    fn variable_type_before(
        &self,
        kind: CompletionVariableKind,
        name: &str,
        owner: &FullyQualifiedName,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let kind = match kind {
            CompletionVariableKind::Instance => VariableTypeKind::Instance,
            CompletionVariableKind::Class => VariableTypeKind::Class,
            CompletionVariableKind::Global => VariableTypeKind::Global,
        };
        View::variable_type_before_in_owner(self, kind, name, owner, file_id, byte_offset)
    }

    fn implicit_receiver_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<FullyQualifiedName> {
        View::execution_context_at(self, file_id, byte_offset)
            .map(|context| context.implicit_receiver.clone())
    }

    fn exact_expression_type(
        &self,
        file_id: SourceFileId,
        start_byte: u32,
        end_byte: u32,
    ) -> Option<RubyType> {
        View::exact_expression_type(self, TextRange::new(file_id, start_byte, end_byte))
    }
}
