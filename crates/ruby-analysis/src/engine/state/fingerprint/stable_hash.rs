//! Cross-process stable hashing of semantic facts for fingerprints.

use std::hash::Hasher;

use crate::core::callables::callable_body::CallableBodyExpression;
use crate::core::callables::callable_body::CallableBodyParameterKind;
use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::callables::callable_signature::CallableSignature;
use crate::core::callables::callable_signature::CallableTypeTemplate;
use crate::core::{
    DiagnosticSeverity, ExecutionScopeMode, FullyQualifiedName, GraphEdgeKind, GraphEdgeProvenance,
    GraphNodeKind, MethodAvailability, MethodParamKind, MethodReferenceAccess, NamespaceKind,
    RubyMethod, RubyType, SourceKind, SymbolKind, TextRange, TypeProvenance, TypeSubject,
};

use super::{SemanticExportFingerprint, SemanticResultFingerprint};
use crate::core::MethodVisibility;

pub(super) fn stable_u8(hasher: &mut StableExportHasher, value: u8) {
    hasher.write(&[value]);
}

pub(super) fn stable_bool(hasher: &mut StableExportHasher, value: bool) {
    stable_u8(hasher, u8::from(value));
}

fn stable_u32(hasher: &mut StableExportHasher, value: u32) {
    hasher.write(&value.to_le_bytes());
}

pub(super) fn stable_u64(hasher: &mut StableExportHasher, value: u64) {
    hasher.write(&value.to_le_bytes());
}

pub(super) fn stable_len(hasher: &mut StableExportHasher, value: usize) {
    stable_u64(
        hasher,
        u64::try_from(value).expect(
            "INVARIANT VIOLATED: semantic export collection length exceeded u64. This is a bug because one process cannot hold that many facts. Fix: reject oversized semantic inputs before fingerprinting.",
        ),
    );
}

pub(super) fn stable_string(hasher: &mut StableExportHasher, value: &str) {
    stable_len(hasher, value.len());
    hasher.write(value.as_bytes());
}

pub(super) fn stable_strings(hasher: &mut StableExportHasher, values: &[String]) {
    stable_len(hasher, values.len());
    for value in values {
        stable_string(hasher, value);
    }
}

pub(super) fn stable_optional_string(hasher: &mut StableExportHasher, value: Option<&str>) {
    match value {
        Some(value) => {
            stable_u8(hasher, 1);
            stable_string(hasher, value);
        }
        None => stable_u8(hasher, 0),
    }
}

pub(super) fn stable_method(hasher: &mut StableExportHasher, method: RubyMethod) {
    stable_string(hasher, method.as_str());
}

pub(super) fn stable_optional_method(hasher: &mut StableExportHasher, method: Option<RubyMethod>) {
    match method {
        Some(method) => {
            stable_u8(hasher, 1);
            stable_method(hasher, method);
        }
        None => stable_u8(hasher, 0),
    }
}

pub(super) fn stable_optional_fqn(
    hasher: &mut StableExportHasher,
    fqn: Option<&FullyQualifiedName>,
) {
    match fqn {
        Some(fqn) => {
            stable_u8(hasher, 1);
            stable_fqn(hasher, fqn);
        }
        None => stable_u8(hasher, 0),
    }
}

pub(super) fn stable_range_offsets(hasher: &mut StableExportHasher, range: TextRange) {
    stable_u32(hasher, range.start_byte);
    stable_u32(hasher, range.end_byte);
}

pub(super) fn stable_fqn(hasher: &mut StableExportHasher, fqn: &FullyQualifiedName) {
    match fqn {
        FullyQualifiedName::Namespace(parts, kind) => {
            stable_u8(hasher, 1);
            stable_len(hasher, parts.len());
            for part in parts {
                stable_string(hasher, part.as_str());
            }
            match kind {
                NamespaceKind::Instance => stable_u8(hasher, 1),
                NamespaceKind::Singleton => stable_u8(hasher, 2),
            }
        }
        FullyQualifiedName::Constant(parts) => {
            stable_u8(hasher, 2);
            stable_len(hasher, parts.len());
            for part in parts {
                stable_string(hasher, part.as_str());
            }
        }
        FullyQualifiedName::Method(parts, method) => {
            stable_u8(hasher, 3);
            stable_len(hasher, parts.len());
            for part in parts {
                stable_string(hasher, part.as_str());
            }
            stable_method(hasher, *method);
        }
        FullyQualifiedName::LocalVariable(name) => {
            stable_u8(hasher, 4);
            stable_string(hasher, name.as_str());
        }
        FullyQualifiedName::InstanceVariable(name) => {
            stable_u8(hasher, 5);
            stable_string(hasher, name.as_str());
        }
        FullyQualifiedName::ClassVariable(name) => {
            stable_u8(hasher, 6);
            stable_string(hasher, name.as_str());
        }
        FullyQualifiedName::GlobalVariable(name) => {
            stable_u8(hasher, 7);
            stable_string(hasher, name.as_str());
        }
    }
}

pub(super) fn stable_symbol_kind(hasher: &mut StableExportHasher, kind: SymbolKind) {
    match kind {
        SymbolKind::Class => stable_u8(hasher, 1),
        SymbolKind::Module => stable_u8(hasher, 2),
        SymbolKind::Method => stable_u8(hasher, 3),
        SymbolKind::Constant => stable_u8(hasher, 4),
        SymbolKind::LocalVariable => stable_u8(hasher, 5),
        SymbolKind::InstanceVariable => stable_u8(hasher, 6),
        SymbolKind::ClassVariable => stable_u8(hasher, 7),
        SymbolKind::GlobalVariable => stable_u8(hasher, 8),
    }
}

pub(super) fn stable_method_param_kind(hasher: &mut StableExportHasher, kind: MethodParamKind) {
    match kind {
        MethodParamKind::Required => stable_u8(hasher, 1),
        MethodParamKind::Optional => stable_u8(hasher, 2),
        MethodParamKind::Rest => stable_u8(hasher, 3),
        MethodParamKind::RequiredKeyword => stable_u8(hasher, 4),
        MethodParamKind::OptionalKeyword => stable_u8(hasher, 5),
        MethodParamKind::KeywordRest => stable_u8(hasher, 6),
        MethodParamKind::Block => stable_u8(hasher, 7),
        MethodParamKind::Forwarding => stable_u8(hasher, 8),
        MethodParamKind::AnonymousRest => stable_u8(hasher, 9),
        MethodParamKind::AnonymousKeywordRest => stable_u8(hasher, 10),
    }
}

pub(super) fn stable_method_visibility(
    hasher: &mut StableExportHasher,
    visibility: MethodVisibility,
) {
    match visibility {
        MethodVisibility::Public => stable_u8(hasher, 1),
        MethodVisibility::Protected => stable_u8(hasher, 2),
        MethodVisibility::Private => stable_u8(hasher, 3),
    }
}

pub(super) fn stable_method_reference_access(
    hasher: &mut StableExportHasher,
    access: MethodReferenceAccess,
) {
    match access {
        MethodReferenceAccess::Normal => stable_u8(hasher, 1),
        MethodReferenceAccess::ExplicitReceiver => stable_u8(hasher, 2),
        MethodReferenceAccess::VisibilityBypass => stable_u8(hasher, 3),
        MethodReferenceAccess::InstanceMethodReflection => stable_u8(hasher, 4),
    }
}

pub(super) fn stable_method_availability(
    hasher: &mut StableExportHasher,
    availability: &MethodAvailability,
) {
    match availability {
        MethodAvailability::Available => stable_u8(hasher, 1),
        MethodAvailability::Unavailable { reason } => {
            stable_u8(hasher, 2);
            stable_string(hasher, reason);
        }
        MethodAvailability::Absent { reason } => {
            stable_u8(hasher, 3);
            stable_string(hasher, reason);
        }
    }
}

pub(super) fn stable_type_subject(hasher: &mut StableExportHasher, subject: &TypeSubject) {
    match subject {
        TypeSubject::Constant(fqn) => {
            stable_u8(hasher, 1);
            stable_fqn(hasher, fqn);
        }
        TypeSubject::Local { scope_id, name } => {
            stable_u8(hasher, 2);
            stable_u32(hasher, *scope_id);
            stable_string(hasher, name);
        }
        TypeSubject::InstanceVariable { owner, name } => {
            stable_u8(hasher, 3);
            stable_fqn(hasher, owner);
            stable_string(hasher, name);
        }
        TypeSubject::ClassVariable { owner, name } => {
            stable_u8(hasher, 4);
            stable_fqn(hasher, owner);
            stable_string(hasher, name);
        }
        TypeSubject::GlobalVariable(name) => {
            stable_u8(hasher, 5);
            stable_string(hasher, name);
        }
        TypeSubject::MethodReturn(fqn) => {
            stable_u8(hasher, 6);
            stable_fqn(hasher, fqn);
        }
        TypeSubject::Parameter { method, name } => {
            stable_u8(hasher, 7);
            stable_fqn(hasher, method);
            stable_string(hasher, name);
        }
        TypeSubject::Expression(range) => {
            stable_u8(hasher, 8);
            stable_u32(hasher, range.start_byte);
            stable_u32(hasher, range.end_byte);
        }
    }
}

pub(super) fn stable_ruby_type(hasher: &mut StableExportHasher, ruby_type: &RubyType) {
    match ruby_type {
        RubyType::Class(fqn) => {
            stable_u8(hasher, 1);
            stable_fqn(hasher, fqn);
        }
        RubyType::Module(fqn) => {
            stable_u8(hasher, 2);
            stable_fqn(hasher, fqn);
        }
        RubyType::ClassReference(fqn) => {
            stable_u8(hasher, 3);
            stable_fqn(hasher, fqn);
        }
        RubyType::ModuleReference(fqn) => {
            stable_u8(hasher, 4);
            stable_fqn(hasher, fqn);
        }
        RubyType::Literal(value) => {
            stable_u8(hasher, 9);
            stable_literal_value(hasher, value);
        }
        RubyType::Array(elements) => {
            stable_u8(hasher, 5);
            stable_len(hasher, elements.len());
            for element in elements {
                stable_ruby_type(hasher, element);
            }
        }
        RubyType::Hash(keys, values) => {
            stable_u8(hasher, 6);
            stable_len(hasher, keys.len());
            for key in keys {
                stable_ruby_type(hasher, key);
            }
            stable_len(hasher, values.len());
            for value in values {
                stable_ruby_type(hasher, value);
            }
        }
        RubyType::Shape(shape) => {
            stable_u8(hasher, 10);
            stable_len(hasher, shape.fields().len());
            for field in shape.fields() {
                match field.key() {
                    crate::core::LiteralKey::Symbol(value) => {
                        stable_u8(hasher, 1);
                        stable_string(hasher, value);
                    }
                    crate::core::LiteralKey::String(value) => {
                        stable_u8(hasher, 2);
                        stable_string(hasher, value);
                    }
                }
                stable_u8(
                    hasher,
                    match field.presence() {
                        crate::core::ShapeFieldPresence::Required => 1,
                        crate::core::ShapeFieldPresence::Optional => 2,
                    },
                );
                stable_ruby_type(hasher, field.value());
            }
            match shape.rest() {
                Some(rest) => {
                    stable_u8(hasher, 1);
                    stable_ruby_type(hasher, rest.key());
                    stable_ruby_type(hasher, rest.value());
                }
                None => stable_u8(hasher, 0),
            }
            stable_u8(
                hasher,
                match shape.exactness() {
                    crate::core::ShapeExactness::Exact => 1,
                    crate::core::ShapeExactness::Open => 2,
                },
            );
            stable_u8(
                hasher,
                match shape.stability() {
                    crate::core::ShapeStability::TrackedMutable => 1,
                    crate::core::ShapeStability::Frozen => 2,
                },
            );
        }
        RubyType::Union(types) => {
            stable_u8(hasher, 7);
            stable_len(hasher, types.len());
            for ruby_type in types {
                stable_ruby_type(hasher, ruby_type);
            }
        }
        RubyType::Unknown => stable_u8(hasher, 8),
    }
}

pub(super) fn stable_callable_signatures(
    hasher: &mut StableExportHasher,
    signatures: &[CallableSignature],
) {
    stable_len(hasher, signatures.len());
    for signature in signatures {
        stable_strings(hasher, &signature.receiver_type_parameters);
        stable_strings(hasher, &signature.type_parameters);
        stable_len(hasher, signature.parameters.len());
        for parameter in &signature.parameters {
            stable_method_param_kind(hasher, parameter.kind);
            stable_callable_template(hasher, &parameter.ruby_type);
        }
        stable_len(hasher, signature.block.parameters.len());
        for parameter in &signature.block.parameters {
            stable_callable_template(hasher, parameter);
        }
        stable_callable_template(hasher, &signature.block.return_type);
        stable_u8(hasher, u8::from(signature.block.required));
        stable_callable_template(hasher, &signature.return_type);
    }
}

pub(super) fn stable_callable_body_summary(
    hasher: &mut StableExportHasher,
    summary: &CallableBodySummary,
) {
    stable_bool(hasher, summary.strict_arity);
    stable_len(hasher, summary.parameters.len());
    for parameter in &summary.parameters {
        stable_string(hasher, &parameter.name);
        stable_u8(
            hasher,
            match parameter.kind {
                CallableBodyParameterKind::Required => 1,
                CallableBodyParameterKind::Optional => 2,
                CallableBodyParameterKind::Rest => 3,
            },
        );
        match &parameter.default {
            Some(default) => {
                stable_u8(hasher, 1);
                stable_callable_body_expression(hasher, default);
            }
            None => stable_u8(hasher, 0),
        }
    }
    stable_strings(hasher, &summary.captures);
    stable_callable_body_expression(hasher, &summary.result);
    stable_u8(hasher, summary.node_count);
}

fn stable_callable_body_expression(
    hasher: &mut StableExportHasher,
    expression: &CallableBodyExpression,
) {
    match expression {
        CallableBodyExpression::Literal(ruby_type) => {
            stable_u8(hasher, 1);
            stable_ruby_type(hasher, ruby_type);
        }
        CallableBodyExpression::Parameter(index) => {
            stable_u8(hasher, 2);
            stable_len(hasher, *index);
        }
        CallableBodyExpression::Capture(name) => {
            stable_u8(hasher, 3);
            stable_string(hasher, name);
        }
        CallableBodyExpression::Array(values) => {
            stable_u8(hasher, 4);
            stable_len(hasher, values.len());
            for value in values {
                stable_callable_body_expression(hasher, value);
            }
        }
        CallableBodyExpression::Shape(fields) => {
            stable_u8(hasher, 5);
            stable_len(hasher, fields.len());
            for (key, value) in fields {
                stable_literal_key(hasher, key);
                stable_callable_body_expression(hasher, value);
            }
        }
        CallableBodyExpression::Call {
            receiver,
            method,
            arguments,
            literal_argument_keys,
        } => {
            stable_u8(hasher, 6);
            stable_callable_body_expression(hasher, receiver);
            stable_method(hasher, *method);
            stable_len(hasher, arguments.len());
            for argument in arguments {
                stable_callable_body_expression(hasher, argument);
            }
            stable_len(hasher, literal_argument_keys.len());
            for key in literal_argument_keys {
                match key {
                    Some(key) => {
                        stable_u8(hasher, 1);
                        stable_literal_key(hasher, key);
                    }
                    None => stable_u8(hasher, 0),
                }
            }
        }
        CallableBodyExpression::ExhaustiveUnion(values) => {
            stable_u8(hasher, 7);
            stable_len(hasher, values.len());
            for value in values {
                stable_callable_body_expression(hasher, value);
            }
        }
    }
}

fn stable_literal_key(hasher: &mut StableExportHasher, key: &crate::core::LiteralKey) {
    match key {
        crate::core::LiteralKey::Symbol(value) => {
            stable_u8(hasher, 1);
            stable_string(hasher, value);
        }
        crate::core::LiteralKey::String(value) => {
            stable_u8(hasher, 2);
            stable_string(hasher, value);
        }
    }
}

fn stable_callable_template(hasher: &mut StableExportHasher, template: &CallableTypeTemplate) {
    match template {
        CallableTypeTemplate::Concrete(ruby_type) => {
            stable_u8(hasher, 1);
            stable_ruby_type(hasher, ruby_type);
        }
        CallableTypeTemplate::Receiver => stable_u8(hasher, 2),
        CallableTypeTemplate::Variable(name) => {
            stable_u8(hasher, 3);
            stable_string(hasher, name);
        }
        CallableTypeTemplate::Array(element) => {
            stable_u8(hasher, 4);
            stable_callable_template(hasher, element);
        }
        CallableTypeTemplate::Hash(key, value) => {
            stable_u8(hasher, 5);
            stable_callable_template(hasher, key);
            stable_callable_template(hasher, value);
        }
        CallableTypeTemplate::Union(members) => {
            stable_u8(hasher, 6);
            stable_len(hasher, members.len());
            for member in members {
                stable_callable_template(hasher, member);
            }
        }
        CallableTypeTemplate::Unconstrained => stable_u8(hasher, 7),
    }
}

pub(super) fn stable_forwarded_block_call(
    hasher: &mut StableExportHasher,
    forwarded: Option<&crate::core::callables::callable_signature::ForwardedBlockCall>,
) {
    match forwarded {
        Some(forwarded) => {
            stable_u8(hasher, 1);
            stable_string(hasher, &forwarded.receiver_parameter);
            stable_method(hasher, forwarded.method);
        }
        None => stable_u8(hasher, 0),
    }
}

pub(super) fn stable_direct_yield_call(
    hasher: &mut StableExportHasher,
    direct: Option<&crate::core::callables::callable_signature::DirectYieldCall>,
) {
    match direct {
        Some(direct) => {
            stable_u8(hasher, 1);
            stable_strings(hasher, &direct.parameter_names);
        }
        None => stable_u8(hasher, 0),
    }
}

fn stable_literal_value(hasher: &mut StableExportHasher, value: &crate::core::LiteralValue) {
    match value {
        crate::core::LiteralValue::Symbol(value) => {
            stable_u8(hasher, 1);
            stable_string(hasher, value);
        }
        crate::core::LiteralValue::String(value) => {
            stable_u8(hasher, 2);
            stable_string(hasher, value);
        }
    }
}

pub(super) fn stable_type_provenance(hasher: &mut StableExportHasher, provenance: TypeProvenance) {
    match provenance {
        TypeProvenance::Literal => stable_u8(hasher, 1),
        TypeProvenance::Assignment => stable_u8(hasher, 2),
        TypeProvenance::Flow => stable_u8(hasher, 3),
        TypeProvenance::Rbs => stable_u8(hasher, 4),
        TypeProvenance::Yard => stable_u8(hasher, 5),
        TypeProvenance::Runtime => stable_u8(hasher, 6),
        TypeProvenance::Extension => stable_u8(hasher, 7),
        TypeProvenance::Inferred => stable_u8(hasher, 8),
    }
}

pub(super) fn stable_graph_node_kind(hasher: &mut StableExportHasher, kind: GraphNodeKind) {
    match kind {
        GraphNodeKind::Class => stable_u8(hasher, 1),
        GraphNodeKind::Module => stable_u8(hasher, 2),
    }
}

pub(super) fn stable_graph_edge_kind(hasher: &mut StableExportHasher, kind: GraphEdgeKind) {
    match kind {
        GraphEdgeKind::Superclass => stable_u8(hasher, 1),
        GraphEdgeKind::Include => stable_u8(hasher, 2),
        GraphEdgeKind::Prepend => stable_u8(hasher, 3),
        GraphEdgeKind::Extend => stable_u8(hasher, 4),
        GraphEdgeKind::ExecutionContextApplication => stable_u8(hasher, 5),
    }
}

pub(super) fn stable_graph_edge_provenance(
    hasher: &mut StableExportHasher,
    provenance: GraphEdgeProvenance,
) {
    match provenance {
        GraphEdgeProvenance::Explicit => stable_u8(hasher, 1),
        GraphEdgeProvenance::ImplicitObject => stable_u8(hasher, 2),
    }
}

pub(super) fn stable_source_kind(hasher: &mut StableExportHasher, kind: SourceKind) {
    match kind {
        SourceKind::Project => stable_u8(hasher, 1),
        SourceKind::Excluded => stable_u8(hasher, 2),
        SourceKind::Signature => stable_u8(hasher, 3),
        SourceKind::External => stable_u8(hasher, 4),
        SourceKind::Stub => stable_u8(hasher, 5),
        SourceKind::Stdlib => stable_u8(hasher, 6),
        SourceKind::Gem => stable_u8(hasher, 7),
    }
}

pub(super) fn stable_diagnostic_severity(
    hasher: &mut StableExportHasher,
    severity: DiagnosticSeverity,
) {
    match severity {
        DiagnosticSeverity::Error => stable_u8(hasher, 1),
        DiagnosticSeverity::Warning => stable_u8(hasher, 2),
        DiagnosticSeverity::Information => stable_u8(hasher, 3),
        DiagnosticSeverity::Hint => stable_u8(hasher, 4),
    }
}

pub(super) fn stable_execution_scope_mode(
    hasher: &mut StableExportHasher,
    mode: ExecutionScopeMode,
) {
    match mode {
        ExecutionScopeMode::Preserve => stable_u8(hasher, 1),
    }
}

pub(super) fn export_hash(
    mut hash_fields: impl FnMut(&mut StableExportHasher),
) -> SemanticExportFingerprint {
    let mut hasher = StableExportHasher::new(0xcbf2_9ce4_8422_2325, 0x8422_2325_cbf2_9ce4);
    hash_fields(&mut hasher);
    let (high, low) = hasher.finish_lanes();
    SemanticExportFingerprint { high, low }
}

pub(super) fn result_hash(
    mut hash_fields: impl FnMut(&mut StableExportHasher),
) -> SemanticResultFingerprint {
    let mut hasher = StableExportHasher::new(0x517c_c1b7_2722_0a95, 0x2722_0a95_517c_c1b7);
    hash_fields(&mut hasher);
    let (high, low) = hasher.finish_lanes();
    SemanticResultFingerprint { high, low }
}

#[cfg(test)]
mod stable_export_hasher_tests {
    use super::*;
    use std::cell::Cell;

    fn legacy_lane(seed: u64, bytes: &[u8]) -> u64 {
        bytes.iter().fold(seed, |state, byte| {
            (state ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[test]
    fn export_hash_visits_fields_once_and_preserves_both_legacy_lanes() {
        let visits = Cell::new(0usize);
        let fingerprint = export_hash(|hasher| {
            visits.set(visits.get() + 1);
            hasher.write(b"semantic-export");
        });

        assert_eq!(visits.get(), 1);
        assert_eq!(
            fingerprint.high,
            legacy_lane(0xcbf2_9ce4_8422_2325, b"semantic-export")
        );
        assert_eq!(
            fingerprint.low,
            legacy_lane(0x8422_2325_cbf2_9ce4, b"semantic-export")
        );
    }
}

pub(super) struct StableExportHasher {
    high: u64,
    low: u64,
}

impl StableExportHasher {
    fn new(high_seed: u64, low_seed: u64) -> Self {
        Self {
            high: high_seed,
            low: low_seed,
        }
    }

    fn finish_lanes(&self) -> (u64, u64) {
        (self.high, self.low)
    }
}

impl Hasher for StableExportHasher {
    fn finish(&self) -> u64 {
        self.high
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.high ^= u64::from(*byte);
            self.high = self.high.wrapping_mul(0x0000_0100_0000_01b3);
            self.low ^= u64::from(*byte);
            self.low = self.low.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}
