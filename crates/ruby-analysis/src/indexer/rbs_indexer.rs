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
                    push_method(&mut facts, &parts, method, &offsets, file_id, source.len())?;
                }
                push_members(
                    &mut facts,
                    &parts,
                    &namespace,
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
                for method in &module.methods {
                    push_method(&mut facts, &parts, method, &offsets, file_id, source.len())?;
                }
                push_members(
                    &mut facts,
                    &parts,
                    &namespace,
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
    let return_type = method.return_type();
    let return_type_label = if is_constructor {
        Some(
            parts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("::"),
        )
    } else {
        return_type.map(rbs_type_to_string)
    };
    let fact = MethodFact::with_param_facts(fqn.clone(), owner, range, params)
        .with_visibility(method_visibility(method.visibility))
        .with_signature_metadata(None, return_type_label);
    facts
        .symbols
        .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
    facts.methods.push(fact);
    if is_constructor {
        facts.types.push(TypeFact::new(
            TypeSubject::MethodReturn(fqn),
            RubyType::Class(FullyQualifiedName::namespace(parts.to_vec())),
            range,
            TypeProvenance::Rbs,
        ));
    } else if let Some(return_type) = return_type {
        push_type_fact(facts, TypeSubject::MethodReturn(fqn), return_type, range);
    }
    Ok(())
}

fn push_members(
    facts: &mut AnalysisIndex,
    parts: &[RubyConstant],
    namespace: &FullyQualifiedName,
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
                    push_method(facts, parts, &synthetic, offsets, file_id, source_len)?;
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
                    push_method(facts, parts, &synthetic, offsets, file_id, source_len)?;
                }
            }
            rbs_parser::Member::Alias(_)
            | rbs_parser::Member::Public
            | rbs_parser::Member::Private => {}
        }
    }
    Ok(())
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
    let ruby_type = ruby_type(rbs_type);
    if ruby_type != RubyType::Unknown {
        facts.types.push(TypeFact::new(
            subject,
            ruby_type,
            range,
            TypeProvenance::Rbs,
        ));
    }
}

fn ruby_type(rbs_type: &RbsType) -> RubyType {
    match rbs_type {
        RbsType::Class(name) => named_type(name),
        RbsType::ClassInstance { name, args } if name.trim_start_matches(':') == "Array" => {
            match args.as_slice() {
                [element] => RubyType::array_of(ruby_type(element)),
                _ => RubyType::Unknown,
            }
        }
        RbsType::ClassInstance { name, args } if name.trim_start_matches(':') == "Hash" => {
            match args.as_slice() {
                [key, value] => RubyType::hash_of(ruby_type(key), ruby_type(value)),
                _ => RubyType::Unknown,
            }
        }
        RbsType::ClassInstance { name, .. } => named_type(name),
        RbsType::Union(types) => RubyType::union(types.iter().map(ruby_type)),
        RbsType::Optional(inner) => RubyType::optional(ruby_type(inner)),
        RbsType::Nil | RbsType::Void => RubyType::nil_class(),
        RbsType::Bool => RubyType::boolean(),
        RbsType::Literal(rbs_parser::Literal::String(_)) => RubyType::string(),
        RbsType::Literal(rbs_parser::Literal::Integer(_)) => RubyType::integer(),
        RbsType::Literal(rbs_parser::Literal::Symbol(_)) => RubyType::symbol(),
        RbsType::Literal(rbs_parser::Literal::True) => RubyType::true_class(),
        RbsType::Literal(rbs_parser::Literal::False) => RubyType::false_class(),
        RbsType::Interface(_)
        | RbsType::TypeVar(_)
        | RbsType::Intersection(_)
        | RbsType::Tuple(_)
        | RbsType::Record(_)
        | RbsType::Proc(_)
        | RbsType::SelfType
        | RbsType::Instance
        | RbsType::ClassType
        | RbsType::Untyped
        | RbsType::Top
        | RbsType::Bot => RubyType::Unknown,
    }
}

fn named_type(name: &str) -> RubyType {
    let normalized = name.trim_start_matches("::");
    match normalized {
        "String" => RubyType::string(),
        "Integer" => RubyType::integer(),
        "Float" => RubyType::float(),
        "Symbol" => RubyType::symbol(),
        "NilClass" => RubyType::nil_class(),
        other => constant_parts(other)
            .map(FullyQualifiedName::namespace)
            .map(RubyType::Class)
            .unwrap_or(RubyType::Unknown),
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
        assert_eq!(facts.graph_nodes.len(), 1);
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
}
