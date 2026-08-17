use crate::core::method_store::MethodVisibility;
use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact, MethodParamFact,
    MethodParamKind, NamespaceKind, RubyConstant, RubyMethod, RubyType, SourceFileId, SymbolFact,
    SymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject, UnresolvedGraphEdgeFact,
};
use rbs_parser::{
    rbs_type_to_string, AttrKind, Declaration, Location, MethodDecl, MethodKind, ParamKind,
    RbsType, Visibility,
};

use super::AnalysisIndex;

pub fn index_rbs(
    file_id: SourceFileId,
    source: &str,
) -> Result<AnalysisIndex, rbs_parser::ParseError> {
    let declarations = rbs_parser::parse(source)?;
    let offsets = LineOffsets::new(source);
    let mut facts = AnalysisIndex::default();

    for declaration in declarations {
        match declaration {
            Declaration::Class(class) => {
                let parts = constant_parts(&class.name)?;
                let namespace = FullyQualifiedName::namespace(parts.clone());
                let range = offsets.range(file_id, class.location, source.len());
                facts
                    .symbols
                    .push(SymbolFact::new(namespace.clone(), SymbolKind::Class, range));
                facts.graph_nodes.push(GraphNodeFact::new(
                    namespace.clone(),
                    GraphNodeKind::Class,
                    range,
                ));
                facts.graph_nodes.push(GraphNodeFact::new(
                    namespace.to_singleton_namespace().expect(
                        "INVARIANT VIOLATED: an RBS class namespace cannot produce its singleton namespace. This is a bug because RBS class declarations always use Namespace FQNs. Fix: validate declaration names before graph construction.",
                    ),
                    GraphNodeKind::Class,
                    range,
                ));
                if let Some(superclass) = class.superclass.as_ref() {
                    push_unresolved_edge(
                        &mut facts,
                        namespace.clone(),
                        superclass,
                        GraphEdgeKind::Superclass,
                        range,
                    )?;
                }
                for method in &class.methods {
                    push_method(
                        &mut facts,
                        &parts,
                        &class.type_params,
                        method,
                        &offsets,
                        file_id,
                        source.len(),
                    )?;
                }
                push_members(
                    &mut facts,
                    &parts,
                    &namespace,
                    &class.type_params,
                    &class.members,
                    &offsets,
                    file_id,
                    source.len(),
                )?;
            }
            Declaration::Module(module) => {
                let parts = constant_parts(&module.name)?;
                let namespace = FullyQualifiedName::namespace(parts.clone());
                let range = offsets.range(file_id, module.location, source.len());
                facts.symbols.push(SymbolFact::new(
                    namespace.clone(),
                    SymbolKind::Module,
                    range,
                ));
                facts.graph_nodes.push(GraphNodeFact::new(
                    namespace.clone(),
                    GraphNodeKind::Module,
                    range,
                ));
                facts.graph_nodes.push(GraphNodeFact::new(
                    namespace.to_singleton_namespace().expect(
                        "INVARIANT VIOLATED: an RBS module namespace cannot produce its singleton namespace. This is a bug because RBS module declarations always use Namespace FQNs. Fix: validate declaration names before graph construction.",
                    ),
                    GraphNodeKind::Module,
                    range,
                ));
                for method in &module.methods {
                    push_method(
                        &mut facts,
                        &parts,
                        &module.type_params,
                        method,
                        &offsets,
                        file_id,
                        source.len(),
                    )?;
                }
                push_members(
                    &mut facts,
                    &parts,
                    &namespace,
                    &module.type_params,
                    &module.members,
                    &offsets,
                    file_id,
                    source.len(),
                )?;
            }
            Declaration::Constant(constant) => {
                let parts = constant_parts(&constant.name)?;
                let fqn = FullyQualifiedName::constant(parts);
                let range = offsets.range(file_id, constant.location, source.len());
                facts
                    .symbols
                    .push(SymbolFact::new(fqn.clone(), SymbolKind::Constant, range));
                push_type_fact(
                    &mut facts,
                    TypeSubject::Constant(fqn),
                    &constant.r#type,
                    range,
                );
            }
            Declaration::Global(global) => {
                let fqn = FullyQualifiedName::global_variable(global.name.clone()).map_err(
                    |message| {
                        rbs_parser::ParseError::new(format!(
                            "invalid RBS global {}: {message}",
                            global.name
                        ))
                    },
                )?;
                let range = offsets.range(file_id, global.location, source.len());
                facts.symbols.push(SymbolFact::new(
                    fqn.clone(),
                    SymbolKind::GlobalVariable,
                    range,
                ));
                push_type_fact(
                    &mut facts,
                    TypeSubject::GlobalVariable(global.name),
                    &global.r#type,
                    range,
                );
            }
            Declaration::Interface(_) | Declaration::TypeAlias(_) => {}
        }
    }

    Ok(facts)
}

fn push_method(
    facts: &mut AnalysisIndex,
    parts: &[RubyConstant],
    owner_type_params: &[rbs_parser::TypeParam],
    method: &MethodDecl,
    offsets: &LineOffsets,
    file_id: SourceFileId,
    source_len: usize,
) -> Result<(), rbs_parser::ParseError> {
    let is_constructor = method.name == "initialize" && method.kind == MethodKind::Instance;
    let normalized_name = if is_constructor { "new" } else { &method.name };
    let ruby_method = RubyMethod::new(normalized_name).map_err(|message| {
        rbs_parser::ParseError::new(format!("invalid RBS method {normalized_name}: {message}"))
    })?;
    let fqn = FullyQualifiedName::method(parts.to_vec(), ruby_method);
    let owner_kind = match (method.kind, is_constructor) {
        (MethodKind::Instance, false) => NamespaceKind::Instance,
        (MethodKind::Instance, true) | (MethodKind::Singleton, false) => NamespaceKind::Singleton,
        (MethodKind::Singleton, true) => NamespaceKind::Singleton,
    };
    let owner = FullyQualifiedName::namespace_with_kind(parts.to_vec(), owner_kind);
    let range = offsets.range(file_id, method.location, source_len);
    let params = method
        .overloads
        .first()
        .map(|overload| {
            overload
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    let name = parameter
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("arg{}", index + 1));
                    MethodParamFact::new(name, param_kind(&parameter.kind))
                        .with_signature_metadata(Some(rbs_type_to_string(&parameter.r#type)), None)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let complete_return_type = complete_rbs_type_union(
        method
            .overloads
            .iter()
            .map(|overload| &overload.return_type),
    );
    let return_type_label = if is_constructor {
        Some(
            parts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("::"),
        )
    } else {
        complete_return_type.as_ref().map(ToString::to_string)
    };
    let owner_type_parameters = owner_type_params
        .iter()
        .map(normalized_type_parameter_name)
        .collect::<Vec<_>>();
    let mut callable_signatures = Vec::new();
    for overload in &method.overloads {
        let method_type_parameters = overload
            .type_params
            .iter()
            .map(normalized_type_parameter_name)
            .collect::<Vec<_>>();
        let callable = crate::inference::higher_order::callable_signature_from_rbs(
            &owner_type_parameters,
            &method_type_parameters,
            overload,
        )
        .map_err(|reason| {
            rbs_parser::ParseError::new(format!(
                "unsupported higher-order RBS signature for {normalized_name}: {}",
                reason.code()
            ))
        })?;
        if let Some(callable) = callable {
            callable_signatures.push(callable);
        }
    }
    let fact = MethodFact::with_param_facts(fqn.clone(), owner, range, params)
        .with_visibility(method_visibility(method.visibility))
        .with_signature_metadata(None, return_type_label)
        .with_callable_signatures(callable_signatures);
    facts
        .symbols
        .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
    facts.methods.push(fact);
    for (name, ruby_type) in complete_parameter_contracts(method) {
        facts.types.push(TypeFact::new(
            TypeSubject::Parameter {
                method: fqn.clone(),
                name,
            },
            ruby_type,
            range,
            TypeProvenance::Rbs,
        ));
    }
    if is_constructor {
        facts.types.push(TypeFact::new(
            TypeSubject::MethodReturn(fqn),
            RubyType::Class(FullyQualifiedName::namespace(parts.to_vec())),
            range,
            TypeProvenance::Rbs,
        ));
    } else if let Some(return_type) = complete_return_type {
        facts.types.push(TypeFact::new(
            TypeSubject::MethodReturn(fqn),
            return_type,
            range,
            TypeProvenance::Rbs,
        ));
    }
    Ok(())
}

fn complete_parameter_contracts(method: &MethodDecl) -> Vec<(String, RubyType)> {
    let Some(first) = method.overloads.first() else {
        return Vec::new();
    };
    if method
        .overloads
        .iter()
        .any(|overload| overload.params.len() != first.params.len())
    {
        return Vec::new();
    }

    let mut contracts = Vec::with_capacity(first.params.len());
    for (index, first_parameter) in first.params.iter().enumerate() {
        let name = parameter_name(first_parameter, index);
        let mut parameter_types = Vec::with_capacity(method.overloads.len());
        for overload in &method.overloads {
            let parameter = &overload.params[index];
            if parameter_name(parameter, index) != name || parameter.kind != first_parameter.kind {
                return Vec::new();
            }
            parameter_types.push(&parameter.r#type);
        }
        let Some(ruby_type) = complete_rbs_type_union(parameter_types) else {
            return Vec::new();
        };
        contracts.push((name, ruby_type));
    }
    contracts
}

fn parameter_name(parameter: &rbs_parser::MethodParam, index: usize) -> String {
    parameter
        .name
        .clone()
        .unwrap_or_else(|| format!("arg{}", index + 1))
}

fn complete_rbs_type_union<'a>(
    rbs_types: impl IntoIterator<Item = &'a RbsType>,
) -> Option<RubyType> {
    let mut ruby_types = Vec::new();
    for rbs_type in rbs_types {
        let ruby_type = crate::inference::rbs::rbs_type_to_ruby_type(rbs_type);
        if RubyType::contains_unknown(&ruby_type) {
            return None;
        }
        ruby_types.push(ruby_type);
    }
    if ruby_types.is_empty() {
        return None;
    }
    let union = RubyType::union(ruby_types);
    (!RubyType::contains_unknown(&union)).then_some(union)
}

fn push_members(
    facts: &mut AnalysisIndex,
    parts: &[RubyConstant],
    namespace: &FullyQualifiedName,
    owner_type_params: &[rbs_parser::TypeParam],
    members: &[rbs_parser::Member],
    offsets: &LineOffsets,
    file_id: SourceFileId,
    source_len: usize,
) -> Result<(), rbs_parser::ParseError> {
    for member in members {
        match member {
            rbs_parser::Member::Include(target) => push_unresolved_edge(
                facts,
                namespace.clone(),
                target,
                GraphEdgeKind::Include,
                offsets.range(file_id, None, source_len),
            )?,
            rbs_parser::Member::Prepend(target) => push_unresolved_edge(
                facts,
                namespace.clone(),
                target,
                GraphEdgeKind::Prepend,
                offsets.range(file_id, None, source_len),
            )?,
            rbs_parser::Member::Extend(target) => push_unresolved_edge(
                facts,
                FullyQualifiedName::singleton_namespace(parts.to_vec()),
                target,
                GraphEdgeKind::Extend,
                offsets.range(file_id, None, source_len),
            )?,
            rbs_parser::Member::Attr(attribute) => {
                let method_names: &[&str] = match attribute.kind {
                    AttrKind::Reader => &[attribute.name.as_str()],
                    AttrKind::Writer => &[],
                    AttrKind::Accessor => &[attribute.name.as_str()],
                };
                for method_name in method_names {
                    let synthetic = MethodDecl {
                        name: (*method_name).to_string(),
                        kind: if attribute.is_singleton {
                            MethodKind::Singleton
                        } else {
                            MethodKind::Instance
                        },
                        overloads: vec![rbs_parser::MethodType::new(attribute.r#type.clone())],
                        visibility: Visibility::Public,
                        location: attribute.location,
                    };
                    push_method(
                        facts,
                        parts,
                        owner_type_params,
                        &synthetic,
                        offsets,
                        file_id,
                        source_len,
                    )?;
                }
                if matches!(attribute.kind, AttrKind::Writer | AttrKind::Accessor) {
                    let synthetic = MethodDecl {
                        name: format!("{}=", attribute.name),
                        kind: if attribute.is_singleton {
                            MethodKind::Singleton
                        } else {
                            MethodKind::Instance
                        },
                        overloads: vec![rbs_parser::MethodType::new(attribute.r#type.clone())],
                        visibility: Visibility::Public,
                        location: attribute.location,
                    };
                    push_method(
                        facts,
                        parts,
                        owner_type_params,
                        &synthetic,
                        offsets,
                        file_id,
                        source_len,
                    )?;
                }
            }
            rbs_parser::Member::Alias(_)
            | rbs_parser::Member::Public
            | rbs_parser::Member::Private => {}
        }
    }
    Ok(())
}

fn normalized_type_parameter_name(parameter: &rbs_parser::TypeParam) -> String {
    parameter
        .name
        .split_whitespace()
        .last()
        .unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: an RBS type parameter has no non-whitespace name. This is a bug because the parser accepted an unusable generic binding. Fix: reject empty type-parameter names during RBS indexing."
            )
        })
        .to_string()
}

fn push_unresolved_edge(
    facts: &mut AnalysisIndex,
    source: FullyQualifiedName,
    target: &RbsType,
    kind: GraphEdgeKind,
    range: TextRange,
) -> Result<(), rbs_parser::ParseError> {
    let Some(name) = rbs_type_name(target) else {
        return Ok(());
    };
    let absolute = name.starts_with("::");
    let target_parts = constant_parts(name)?;
    facts
        .unresolved_graph_edges
        .push(UnresolvedGraphEdgeFact::new(
            source.clone(),
            target_parts,
            absolute,
            source,
            kind,
            range,
        ));
    Ok(())
}

fn rbs_type_name(rbs_type: &RbsType) -> Option<&str> {
    match rbs_type {
        RbsType::Class(name) | RbsType::ClassInstance { name, .. } => Some(name),
        _ => None,
    }
}

fn push_type_fact(
    facts: &mut AnalysisIndex,
    subject: TypeSubject,
    rbs_type: &RbsType,
    range: TextRange,
) {
    let ruby_type = crate::inference::rbs::rbs_type_to_ruby_type(rbs_type);
    if ruby_type != RubyType::Unknown {
        facts.types.push(TypeFact::new(
            subject,
            ruby_type,
            range,
            TypeProvenance::Rbs,
        ));
    }
}

fn constant_parts(name: &str) -> Result<Vec<RubyConstant>, rbs_parser::ParseError> {
    name.trim_start_matches("::")
        .split("::")
        .map(|part| {
            RubyConstant::new(part).map_err(|message| {
                rbs_parser::ParseError::new(format!("invalid RBS constant {name}: {message}"))
            })
        })
        .collect()
}

fn param_kind(kind: &ParamKind) -> MethodParamKind {
    match kind {
        ParamKind::Required => MethodParamKind::Required,
        ParamKind::Optional => MethodParamKind::Optional,
        ParamKind::Rest => MethodParamKind::Rest,
        ParamKind::Keyword => MethodParamKind::RequiredKeyword,
        ParamKind::KeywordOpt => MethodParamKind::OptionalKeyword,
        ParamKind::KeywordRest => MethodParamKind::KeywordRest,
        ParamKind::Block => MethodParamKind::Block,
    }
}

fn method_visibility(visibility: Visibility) -> MethodVisibility {
    match visibility {
        Visibility::Public => MethodVisibility::Public,
        Visibility::Protected => MethodVisibility::Protected,
        Visibility::Private => MethodVisibility::Private,
    }
}

struct LineOffsets {
    starts: Vec<usize>,
}

impl LineOffsets {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        Self { starts }
    }

    fn range(
        &self,
        file_id: SourceFileId,
        location: Option<Location>,
        source_len: usize,
    ) -> TextRange {
        let Some(location) = location else {
            return TextRange::new(file_id, 0, source_len as u32);
        };
        let start = self.offset(location.start_row, location.start_col, source_len);
        let end = self.offset(location.end_row, location.end_col, source_len);
        TextRange::new(file_id, start as u32, end as u32)
    }

    fn offset(&self, row: usize, column: usize, source_len: usize) -> usize {
        let line_start = *self.starts.get(row).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: RBS parser returned row {row} outside {} source lines. \
                 This is a bug because parser locations must address the parsed source. \
                 Fix: keep RBS location conversion synchronized with tree-sitter points.",
                self.starts.len()
            )
        });
        let offset = line_start + column;
        assert!(
            offset <= source_len,
            "INVARIANT VIOLATED: RBS parser byte location {offset} exceeds source length {source_len}. \
             This is a bug because parser locations must remain inside the parsed file. \
             Fix: verify RBS columns are interpreted as UTF-8 byte columns."
        );
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rbs_declarations_emit_navigation_signature_and_type_facts() {
        let source = "class NativeWidget\n  def initialize: (String name) -> void\n  def encode: (String value) -> String\nend\n";
        let facts = index_rbs(SourceFileId(7), source).expect("RBS fixture must parse");
        assert_eq!(facts.graph_nodes.len(), 2);
        assert!(facts.graph_nodes.iter().any(|fact| {
            fact.fqn.namespace_kind() == Some(NamespaceKind::Instance)
                && fact.kind == GraphNodeKind::Class
        }));
        assert!(facts.graph_nodes.iter().any(|fact| {
            fact.fqn.namespace_kind() == Some(NamespaceKind::Singleton)
                && fact.kind == GraphNodeKind::Class
        }));
        assert_eq!(facts.methods.len(), 2);
        let constructor = facts
            .methods
            .iter()
            .find(|fact| fact.fqn.name() == "new")
            .expect("RBS initialize must become the callable constructor");
        assert_eq!(
            constructor.owner.namespace_kind(),
            Some(NamespaceKind::Singleton)
        );
        let encode = facts
            .methods
            .iter()
            .find(|fact| fact.fqn.name() == "encode")
            .expect("ordinary RBS method must be indexed");
        assert_eq!(encode.params, vec!["value"]);
        assert_eq!(encode.return_type_label.as_deref(), Some("String"));
        assert!(facts
            .types
            .iter()
            .any(|fact| fact.ruby_type == RubyType::string()));
    }

    #[test]
    fn rbs_records_become_canonical_shape_facts_with_optional_fields() {
        let source = "class PayloadFactory\n  def build: () -> { id: Integer, ?name: String }\nend\nPAYLOAD: { id: Integer, ?name: String }\n";
        let facts = index_rbs(SourceFileId(8), source).expect("RBS record fixture must parse");
        let expected = "{ id: Integer, name?: String }";

        let method_return = facts
            .types
            .iter()
            .find(|fact| matches!(fact.subject, TypeSubject::MethodReturn(_)))
            .expect("the RBS record return must produce a method-return type fact");
        assert_eq!(method_return.ruby_type.to_string(), expected);

        let constant = facts
            .types
            .iter()
            .find(|fact| matches!(fact.subject, TypeSubject::Constant(_)))
            .expect("the RBS record constant must produce a value-constant type fact");
        assert_eq!(constant.ruby_type.to_string(), expected);
    }

    #[test]
    fn overloaded_rbs_records_emit_exhaustive_parameter_and_return_unions() {
        let source = r#"class PayloadService
  def normalize: ({ kind: :number, value: Integer } payload) -> Integer
               | ({ kind: :text, value: String } payload) -> String
end
"#;
        let facts =
            index_rbs(SourceFileId(9), source).expect("overloaded RBS record fixture must parse");

        let parameter = facts
            .types
            .iter()
            .find(|fact| matches!(fact.subject, TypeSubject::Parameter { .. }))
            .expect("complete overloads must emit one exhaustive parameter union");
        assert_eq!(
            parameter.ruby_type.to_string(),
            "({ kind: :number, value: Integer } | { kind: :text, value: String })"
        );

        let return_type = facts
            .types
            .iter()
            .find(|fact| matches!(fact.subject, TypeSubject::MethodReturn(_)))
            .expect("complete overloads must emit one exhaustive return union");
        assert_eq!(return_type.ruby_type.to_string(), "(Integer | String)");
    }

    #[test]
    fn incomplete_overload_evidence_emits_no_partial_contract_fact() {
        let source = r#"class PayloadService
  def normalize: ({ value: Integer } payload) -> Integer
               | (untyped payload) -> untyped
end
"#;
        let facts = index_rbs(SourceFileId(10), source)
            .expect("incomplete overloaded RBS fixture must parse");

        assert!(
            facts.types.iter().all(|fact| !matches!(
                fact.subject,
                TypeSubject::Parameter { .. } | TypeSubject::MethodReturn(_)
            )),
            "one unsupported overload must prevent a partial concrete contract"
        );
    }

    #[test]
    fn generic_block_templates_survive_rbs_indexing_on_the_method_fact() {
        let source = r#"class Transformer
  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Output
end
"#;
        let facts = index_rbs(SourceFileId(11), source)
            .expect("the generic callable signature fixture must parse");
        let method = facts
            .methods
            .iter()
            .find(|fact| fact.fqn.name() == "apply")
            .expect("the block-bearing method must be indexed");
        let [signature] = method.callable_signatures() else {
            panic!(
                "INVARIANT VIOLATED: one RBS overload did not produce exactly one callable signature. This is a bug because callable overload evidence must remain file-owned and complete. Fix: retain every supported block-bearing MethodType on its MethodFact."
            );
        };
        assert_eq!(signature.type_parameters, ["Input", "Output"]);
        assert_eq!(
            signature.block.parameters,
            [crate::core::CallableTypeTemplate::Variable(
                "Input".to_string()
            )]
        );
        assert_eq!(
            signature.block.return_type,
            crate::core::CallableTypeTemplate::Variable("Output".to_string())
        );
        assert_eq!(
            signature.return_type,
            crate::core::CallableTypeTemplate::Variable("Output".to_string())
        );
    }
}
