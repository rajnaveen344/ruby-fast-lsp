//! Class-body Java declarations: `java_import`, `import`, `include_package`,
//! Java interface inclusion, and `java_package`, with static import aliases.

use super::syntax::{
    dotted_call_name, is_java_class_name, java_package_prefix, static_symbol_or_string,
};
use super::JrubyImportProvider;
use ruby_analysis::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, ReferenceCandidate, RubyConstant, RubyType,
    SymbolFact, SymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_fast_lsp_jvm_metadata::ClassKind;
use ruby_prism::{CallNode, Node};

const MAX_STATIC_IMPORT_ALIAS_BYTES: usize = 256;

impl JrubyImportProvider {
    pub(super) fn process_import_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"java_import" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let alias_block = node.block().and_then(|block| block.as_block_node());
        if node.block().is_some() && alias_block.is_none() {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "unsupported-jruby-import-alias",
                "Dynamic java_import alias blocks are not resolved statically yet.".to_string(),
            );
            return;
        }
        let mut imports = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut imports);
        }
        for import in imports {
            let alias = if let Some(block) = &alias_block {
                let Ok(java_name) = JavaClassName::parse(&import.name) else {
                    self.add_import(visitor, import, None, true);
                    continue;
                };
                let Some(alias) = evaluate_static_import_alias(
                    block,
                    &java_name.package().join("."),
                    java_name.imported_constant(),
                ) else {
                    visitor.push_warning_diagnostic(
                        visitor.text_range_from_offsets(
                            node.location().start_offset(),
                            node.location().end_offset(),
                        ),
                        "unsupported-jruby-import-alias",
                        "The java_import alias block is not a bounded literal interpolation of its package and class-name parameters.".to_string(),
                    );
                    return;
                };
                Some(alias)
            } else {
                None
            };
            self.add_import(visitor, import, alias, true);
        }
    }

    pub(super) fn process_import_dispatch(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"import" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let alias_block = node.block().and_then(|block| block.as_block_node());
        if node.block().is_some() && alias_block.is_none() {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "unsupported-jruby-import-alias",
                "Dynamic import alias blocks are not resolved statically yet.".to_string(),
            );
            return;
        }
        let mut imports = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut imports);
        }
        for import in imports {
            if is_java_class_name(&import.name) {
                let alias = if let Some(block) = &alias_block {
                    let Ok(java_name) = JavaClassName::parse(&import.name) else {
                        self.add_import(visitor, import, None, true);
                        continue;
                    };
                    let Some(alias) = evaluate_static_import_alias(
                        block,
                        &java_name.package().join("."),
                        java_name.imported_constant(),
                    ) else {
                        visitor.push_warning_diagnostic(
                            visitor.text_range_from_offsets(
                                node.location().start_offset(),
                                node.location().end_offset(),
                            ),
                            "unsupported-jruby-import-alias",
                            "The import alias block is not a bounded literal interpolation of its package and class-name parameters.".to_string(),
                        );
                        return;
                    };
                    Some(alias)
                } else {
                    None
                };
                self.add_import(visitor, import, alias, true);
            } else {
                self.add_package(visitor, import);
            }
        }
    }

    pub(super) fn process_include_package_call(
        &self,
        visitor: &mut FactCollector,
        node: &CallNode<'_>,
    ) {
        if node.receiver().is_some() || node.name().as_slice() != b"include_package" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut packages = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut packages);
        }
        for package in packages {
            self.add_package(visitor, package);
        }
    }

    pub(super) fn process_java_interface_call(
        &self,
        visitor: &mut FactCollector,
        node: &CallNode<'_>,
    ) {
        if node.receiver().is_some()
            || !matches!(node.name().as_slice(), b"include" | b"java_implements")
        {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut interfaces = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut interfaces);
        }
        let source = FullyQualifiedName::namespace(visitor.scope_tracker().get_ns_stack());
        for interface in interfaces {
            if !is_java_class_name(&interface.name) {
                continue;
            }
            let Ok(java_name) = JavaClassName::parse(&interface.name) else {
                continue;
            };
            let Some(declaration) = self.catalog.classes.get(java_name.internal_name()) else {
                visitor.push_error_diagnostic(
                    interface.range,
                    "unresolved-java-interface",
                    format!(
                        "Java interface `{}` is not present on this project's isolated classpath.",
                        interface.name
                    ),
                );
                continue;
            };
            if declaration.class.kind() != ClassKind::Interface {
                visitor.push_error_diagnostic(
                    interface.range,
                    "invalid-java-interface",
                    format!("Java type `{}` is not an interface.", interface.name),
                );
                continue;
            }
            let target = FullyQualifiedName::namespace(
                java_name
                    .ruby_namespace_parts()
                    .into_iter()
                    .map(|part| {
                        RubyConstant::new(&part).expect(
                            "INVARIANT VIOLATED: validated Java interface proxy part is not a Ruby constant. \
                             This is a bug because JavaClassName owns proxy validation. \
                             Fix: keep Java interface proxy conversion single-sourced.",
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            visitor.add_graph_edge_fact(GraphEdgeFact::new(
                source.clone(),
                target,
                GraphEdgeKind::Include,
                interface.range,
            ));
        }
    }

    pub(super) fn process_java_package_call(
        &self,
        visitor: &mut FactCollector,
        node: &CallNode<'_>,
    ) {
        if node.receiver().is_some() || node.name().as_slice() != b"java_package" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            visitor.push_warning_diagnostic(
                visitor
                    .text_range_from_offsets(node.location().start_offset(), node.location().end_offset()),
                "unsupported-jruby-java-package",
                "java_package is a jrubyc declaration and requires exactly one static Java package name.".to_string(),
            );
            return;
        };
        let arguments = arguments.arguments().iter().collect::<Vec<_>>();
        let package = if arguments.len() == 1 {
            static_symbol_or_string(&arguments[0]).or_else(|| {
                arguments[0]
                    .as_call_node()
                    .and_then(|call| dotted_call_name(&call))
            })
        } else {
            None
        };
        if package.as_deref().and_then(java_package_prefix).is_none() {
            visitor.push_warning_diagnostic(
                visitor
                    .text_range_from_offsets(node.location().start_offset(), node.location().end_offset()),
                "unsupported-jruby-java-package",
                "java_package is a jrubyc declaration and requires exactly one static Java package name.".to_string(),
            );
        }
    }

    fn add_package(&self, visitor: &mut FactCollector, package: StaticJavaImport) {
        let names = match self.class_names_in_package(&package.name) {
            Ok(names) => names,
            Err(message) => {
                visitor.push_error_diagnostic(package.range, "invalid-java-package", message);
                return;
            }
        };
        if names.is_empty() {
            visitor.push_error_diagnostic(
                package.range,
                "unresolved-java-package",
                format!(
                    "Java package `{}` has no direct classes on this project's isolated classpath.",
                    package.name
                ),
            );
            return;
        }
        for name in names {
            let alias = name
                .rsplit('/')
                .next()
                .expect("INVARIANT VIOLATED: validated internal Java class has no class component")
                .to_string();
            self.add_import(
                visitor,
                StaticJavaImport {
                    name,
                    range: package.range,
                    name_range: package.name_range,
                },
                Some(alias),
                false,
            );
        }
    }

    fn add_import(
        &self,
        visitor: &mut FactCollector,
        import: StaticJavaImport,
        alias: Option<String>,
        emit_symbol: bool,
    ) {
        let Ok(java_name) = JavaClassName::parse(&import.name) else {
            visitor.push_error_diagnostic(
                import.range,
                "invalid-java-import",
                format!(
                    "`{}` is not a valid fully qualified Java class name.",
                    import.name
                ),
            );
            return;
        };
        let Some(declaration) = self.catalog.classes.get(java_name.internal_name()) else {
            visitor.push_error_diagnostic(
                import.range,
                "unresolved-java-import",
                format!(
                    "Java class `{}` is not present on this project's isolated classpath.",
                    import.name
                ),
            );
            return;
        };
        assert_eq!(
            declaration.class.name,
            java_name.internal_name(),
            "INVARIANT VIOLATED: Java catalog key and declaration name disagree. \
             This is a bug because archive ingestion validates class identity before catalog insertion. \
             Fix: preserve the parsed internal name as the catalog key."
        );

        let mut alias_parts = visitor.scope_tracker().get_ns_stack();
        let alias_name = alias
            .as_deref()
            .unwrap_or_else(|| java_name.imported_constant());
        let Ok(alias) = RubyConstant::new(alias_name) else {
            visitor.push_error_diagnostic(
                import.name_range,
                "invalid-java-import-alias",
                format!(
                    "Java class `{}` cannot be imported as Ruby constant `{}`.",
                    import.name, alias_name
                ),
            );
            return;
        };
        alias_parts.push(alias);
        let alias_fqn = FullyQualifiedName::constant(alias_parts);
        let declaration_range = import.range;
        if emit_symbol {
            visitor.add_symbol_fact(
                SymbolFact::new(alias_fqn.clone(), SymbolKind::Constant, declaration_range)
                    .with_name_range(import.name_range),
            );
        }

        let proxy_parts: Vec<RubyConstant> = java_name
            .ruby_namespace_parts()
            .into_iter()
            .map(|part| {
                RubyConstant::new(&part).expect(
                    "INVARIANT VIOLATED: JRuby proxy name component is not a Ruby constant. \
                     This is a bug because JavaClassName owns proxy constant validation. \
                     Fix: keep proxy name generation Ruby-constant-safe.",
                )
            })
            .collect();
        let proxy_fqn = FullyQualifiedName::constant(proxy_parts);
        visitor.add_reference_candidate(ReferenceCandidate::resolved(
            import.name_range,
            proxy_fqn.clone(),
            visitor.scope_tracker().current_method_fqn().cloned(),
        ));
        let type_fact = TypeFact::new(
            TypeSubject::Constant(alias_fqn),
            RubyType::ClassReference(proxy_fqn),
            declaration_range,
            TypeProvenance::Runtime,
        );
        visitor.add_type_fact(type_fact.clone());
        visitor.add_direct_type_fact(type_fact);
    }
}

#[derive(Debug)]
struct StaticJavaImport {
    name: String,
    range: TextRange,
    name_range: TextRange,
}

fn collect_static_imports(
    visitor: &FactCollector,
    node: &Node<'_>,
    imports: &mut Vec<StaticJavaImport>,
) {
    if let Some(array) = node.as_array_node() {
        for element in array.elements().iter() {
            collect_static_imports(visitor, &element, imports);
        }
        return;
    }
    if let Some(string) = node.as_string_node() {
        let name = String::from_utf8_lossy(string.unescaped()).to_string();
        let content = string.content_loc();
        let name_start = content
            .end_offset()
            .saturating_sub(imported_name_length(&name));
        imports.push(StaticJavaImport {
            name,
            range: visitor.text_range_from_offsets(
                node.location().start_offset(),
                node.location().end_offset(),
            ),
            name_range: visitor.text_range_from_offsets(name_start, content.end_offset()),
        });
        return;
    }
    let Some(call) = node.as_call_node() else {
        return;
    };
    let Some(name) = dotted_call_name(&call) else {
        return;
    };
    if !name.contains('.') {
        return;
    }
    let Some(message) = call.message_loc() else {
        return;
    };
    imports.push(StaticJavaImport {
        name,
        range: visitor
            .text_range_from_offsets(node.location().start_offset(), node.location().end_offset()),
        name_range: visitor.text_range_from_offsets(message.start_offset(), message.end_offset()),
    });
}

fn imported_name_length(name: &str) -> usize {
    name.rsplit(['.', '$'])
        .next()
        .map(str::len)
        .unwrap_or(name.len())
}

pub(super) fn evaluate_static_import_alias(
    block: &ruby_prism::BlockNode<'_>,
    package: &str,
    class_name: &str,
) -> Option<String> {
    let parameters = block
        .parameters()?
        .as_block_parameters_node()?
        .parameters()?;
    if parameters.requireds().iter().count() != 2
        || parameters.optionals().iter().next().is_some()
        || parameters.rest().is_some()
        || parameters.posts().iter().next().is_some()
        || parameters.keywords().iter().next().is_some()
        || parameters.keyword_rest().is_some()
        || parameters.block().is_some()
    {
        return None;
    }
    let names = parameters
        .requireds()
        .iter()
        .map(|parameter| {
            parameter
                .as_required_parameter_node()
                .map(|parameter| String::from_utf8_lossy(parameter.name().as_slice()).to_string())
        })
        .collect::<Option<Vec<_>>>()?;
    let expression = single_expression(block.body()?)?;
    let alias = evaluate_static_alias_expression(
        expression,
        (&names[0], package),
        (&names[1], class_name),
    )?;
    (alias.len() <= MAX_STATIC_IMPORT_ALIAS_BYTES).then_some(alias)
}

fn single_expression(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(statements) = node.as_statements_node() {
        let mut body = statements.body().iter();
        let expression = body.next()?;
        if body.next().is_some() {
            return None;
        }
        return Some(expression);
    }
    if let Some(embedded) = node.as_embedded_statements_node() {
        let statements = embedded.statements()?;
        let mut body = statements.body().iter();
        let expression = body.next()?;
        if body.next().is_some() {
            return None;
        }
        return Some(expression);
    }
    Some(node)
}

fn evaluate_static_alias_expression(
    node: Node<'_>,
    first: (&str, &str),
    second: (&str, &str),
) -> Option<String> {
    if let Some(string) = node.as_string_node() {
        return Some(String::from_utf8_lossy(string.unescaped()).to_string());
    }
    if let Some(local) = node.as_local_variable_read_node() {
        let name = String::from_utf8_lossy(local.name().as_slice());
        return match name.as_ref() {
            name if name == first.0 => Some(first.1.to_string()),
            name if name == second.0 => Some(second.1.to_string()),
            _ => None,
        };
    }
    let interpolated = node.as_interpolated_string_node()?;
    let mut output = String::new();
    for part in interpolated.parts().iter() {
        let value = if let Some(string) = part.as_string_node() {
            String::from_utf8_lossy(string.unescaped()).to_string()
        } else {
            evaluate_static_alias_expression(single_expression(part)?, first, second)?
        };
        if output.len().saturating_add(value.len()) > MAX_STATIC_IMPORT_ALIAS_BYTES {
            return None;
        }
        output.push_str(&value);
    }
    Some(output)
}
