use ruby_analysis::core::{FullyQualifiedName, RubyType as AnalysisRubyType};
use ruby_fast_lsp_extension_api::IndexPatch;

pub(crate) fn analysis_ruby_type_from_extension(
    ruby_type: Option<&ruby_fast_lsp_extension_api::RubyType>,
) -> Result<Option<AnalysisRubyType>, String> {
    let Some(ruby_type) = ruby_type else {
        return Ok(None);
    };
    if ruby_type == &ruby_fast_lsp_extension_api::RubyType::Unknown {
        return Ok(None);
    }
    let mut node_count = 0;
    let converted = analysis_ruby_type_from_extension_inner(ruby_type, 0, &mut node_count)?;
    if converted == AnalysisRubyType::Unknown {
        Ok(None)
    } else {
        Ok(Some(converted))
    }
}

fn analysis_ruby_type_from_extension_inner(
    ruby_type: &ruby_fast_lsp_extension_api::RubyType,
    depth: usize,
    node_count: &mut usize,
) -> Result<AnalysisRubyType, String> {
    const MAX_TYPE_DEPTH: usize = 8;
    const MAX_TYPE_NODES: usize = 64;
    if depth > MAX_TYPE_DEPTH {
        return Err(format!(
            "extension Ruby type nesting exceeds maximum depth {MAX_TYPE_DEPTH}"
        ));
    }
    *node_count += 1;
    if *node_count > MAX_TYPE_NODES {
        return Err(format!(
            "extension Ruby type exceeds maximum node count {MAX_TYPE_NODES}"
        ));
    }

    match ruby_type {
        ruby_fast_lsp_extension_api::RubyType::Named(name) => {
            let fqn = FullyQualifiedName::try_from(name.as_str())
                .map_err(|err| format!("invalid named Ruby type `{name}`: {err}"))?;
            Ok(AnalysisRubyType::Class(fqn))
        }
        ruby_fast_lsp_extension_api::RubyType::Array(element_types) => {
            let elements =
                convert_extension_type_list(element_types, depth + 1, node_count, "array element")?;
            Ok(AnalysisRubyType::Array(elements))
        }
        ruby_fast_lsp_extension_api::RubyType::Hash { keys, values } => {
            let keys = convert_extension_type_list(keys, depth + 1, node_count, "hash key")?;
            let values = convert_extension_type_list(values, depth + 1, node_count, "hash value")?;
            Ok(AnalysisRubyType::Hash(keys, values))
        }
        ruby_fast_lsp_extension_api::RubyType::Union(types) => {
            let types = convert_extension_type_list(types, depth + 1, node_count, "union")?;
            Ok(AnalysisRubyType::union(types))
        }
        ruby_fast_lsp_extension_api::RubyType::Unknown => Ok(AnalysisRubyType::Unknown),
    }
}

fn convert_extension_type_list(
    types: &[ruby_fast_lsp_extension_api::RubyType],
    depth: usize,
    node_count: &mut usize,
    label: &str,
) -> Result<Vec<AnalysisRubyType>, String> {
    if types.is_empty() {
        return Err(format!("extension {label} type list must not be empty"));
    }
    let mut converted = types
        .iter()
        .map(|ruby_type| analysis_ruby_type_from_extension_inner(ruby_type, depth, node_count))
        .collect::<Result<Vec<_>, _>>()?;
    converted.sort_by_key(|ruby_type| format!("{ruby_type:?}"));
    converted.dedup();
    Ok(converted)
}

pub(in crate::environment::extensions) fn extension_ruby_types_semantically_equal(
    left: Option<&ruby_fast_lsp_extension_api::RubyType>,
    right: Option<&ruby_fast_lsp_extension_api::RubyType>,
) -> bool {
    analysis_ruby_type_from_extension(left).expect(
        "INVARIANT VIOLATED: invalid left extension Ruby type reached conflict resolution. This is a bug because patch payloads must be validated before deterministic merging. Fix: keep validation before resolve_index_patch_conflicts.",
    ) == analysis_ruby_type_from_extension(right).expect(
        "INVARIANT VIOLATED: invalid right extension Ruby type reached conflict resolution. This is a bug because patch payloads must be validated before deterministic merging. Fix: keep validation before resolve_index_patch_conflicts.",
    )
}

pub(super) fn index_patch_payload_eq(left: &IndexPatch, right: &IndexPatch) -> bool {
    match (left, right) {
        (IndexPatch::DefineNamespace(left), IndexPatch::DefineNamespace(right)) => {
            left.namespace == right.namespace
                && left.kind == right.kind
                && left.location == right.location
        }
        (IndexPatch::DefineConstant(left), IndexPatch::DefineConstant(right)) => {
            left.namespace == right.namespace
                && left.name == right.name
                && left.location == right.location
                && extension_ruby_types_semantically_equal(
                    left.ruby_type.as_ref(),
                    right.ruby_type.as_ref(),
                )
        }
        (IndexPatch::AddReference(left), IndexPatch::AddReference(right)) => {
            left.target == right.target && left.location == right.location
        }
        (IndexPatch::DefineMethod(left), IndexPatch::DefineMethod(right)) => {
            left.name == right.name
                && left.namespace == right.namespace
                && left.owner_target == right.owner_target
                && left.owner_kind == right.owner_kind
                && left.visibility == right.visibility
                && left.location == right.location
                && left.params == right.params
                && left.return_type_source == right.return_type_source
                && extension_ruby_types_semantically_equal(
                    left.return_type.as_ref(),
                    right.return_type.as_ref(),
                )
        }
        (IndexPatch::SetSuperclass(left), IndexPatch::SetSuperclass(right)) => {
            left.namespace == right.namespace
                && left.superclass == right.superclass
                && left.absolute == right.absolute
                && left.location == right.location
        }
        (IndexPatch::ApplyMixin(left), IndexPatch::ApplyMixin(right)) => {
            left.namespace == right.namespace
                && left.owner_target == right.owner_target
                && left.target_kind == right.target_kind
                && left.mixin == right.mixin
                && left.absolute == right.absolute
                && left.kind == right.kind
                && left.location == right.location
        }
        (IndexPatch::ConnectExecutionContext(left), IndexPatch::ConnectExecutionContext(right)) => {
            left.template == right.template
                && left.application == right.application
                && left.location == right.location
        }
        (IndexPatch::DefineNamespace(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::AddReference(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::AddReference(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::AddReference(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::AddReference(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::AddReference(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::AddReference(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::AddReference(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::AddReference(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::AddReference(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::AddReference(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::AddReference(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::AddReference(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::ConnectExecutionContext(_)) => false,
    }
}
