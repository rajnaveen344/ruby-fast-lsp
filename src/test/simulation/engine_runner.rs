use super::graph::{CallShape, MethodKind, MethodTarget};
use super::oracle::OracleState;
use super::project::{EditStep, SyntheticProject};
use super::ruby_gen::{
    CallSite, ConstantRefSite, ProjectRender, SourceMap, SourcePos, TypeAssertKind,
};
use parking_lot::RwLock;
use ruby_analysis::core::{
    FullyQualifiedName, NamespaceKind as CoreNamespaceKind, RubyConstant, RubyMethod, RubyType,
    SourceKind, TextRange, TypeSubject,
};
use ruby_analysis::engine::{
    AnalysisEngine, AnalysisStats, FileFacts, ResolveMode, SourceFileInput,
};
use ruby_analysis::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use ruby_analysis::indexer::{AnalysisIndex, AnalysisIndexer, RubyDocument};
use ruby_prism::Visit;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

pub struct EngineSimulationRunner {
    project: SyntheticProject,
    engine: Arc<RwLock<AnalysisEngine>>,
    render: ProjectRender,
}

impl EngineSimulationRunner {
    pub fn start(project: SyntheticProject) -> Self {
        let render = project.render();
        let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
        let mut runner = Self {
            project,
            engine,
            render,
        };
        runner.reindex_all();
        runner
    }

    pub fn apply_step(&mut self, step: &EditStep) {
        for op in &step.ops {
            self.project.apply_op(op);
        }
        self.render = self.project.render();
        self.reindex_all();
    }

    pub fn stats(&self) -> AnalysisStats {
        self.engine.read().stats()
    }

    pub fn check_definitions(&self) {
        let oracle = OracleState::all_files(&self.project, &self.render.map);
        let engine = self.engine.read();
        let query = engine.query();

        for call in self
            .render
            .map
            .calls
            .iter()
            .filter(|call| call.definition_support.is_supported())
        {
            let Some(expected_target) = oracle.resolve_call(call) else {
                continue;
            };
            let Some(owner) = lookup_owner_for_call(call) else {
                continue;
            };
            let method = ruby_method(&call.target.name);
            let callees = if matches!(call.shape, CallShape::Super) {
                query
                    .resolve_super_method_callee(&owner, &method)
                    .map(|callee| vec![callee])
            } else {
                query.resolve_method_callees(&owner, &method)
            }
            .unwrap_or_else(|| {
                panic!(
                    "Expected engine method callees for lookup owner `{}` method `{}`",
                    owner, call.target.name
                )
            });
            let expected_def = self.def_pos(&expected_target);
            assert!(
                callees.iter().any(|callee| callee
                    .definition_ranges
                    .iter()
                    .any(|range| range_matches_pos(&engine, *range, expected_def))),
                "Expected engine lookup for {} from {} to resolve to {} at {}:{}, got {:?}",
                call.target.signature(),
                call.caller.signature(),
                expected_target.signature(),
                expected_def.file,
                expected_def.line,
                callees
            );
        }

        for constant_ref in &self.render.map.constant_refs {
            let Some(expected_target) = oracle.resolve_constant_ref(constant_ref) else {
                continue;
            };
            let parts = ruby_parts(constant_ref.text.trim_start_matches("::"));
            let context = ruby_parts(&constant_ref.caller.owner);
            let ranges = query.constant_definition_ranges(&parts, &context);
            let expected_def = self.const_def_pos(&expected_target);
            assert!(
                ranges
                    .iter()
                    .any(|range| range_matches_pos(&engine, *range, expected_def)),
                "Expected engine constant lookup for `{}` from {} to resolve to {} at {}:{}, got {:?}",
                constant_ref.text,
                constant_ref.caller.signature(),
                expected_target,
                expected_def.file,
                expected_def.line,
                ranges
            );
        }
    }

    pub fn check_references(&self) {
        let oracle = OracleState::all_files(&self.project, &self.render.map);
        let engine = self.engine.read();
        let query = engine.query();
        let mut expected_method_calls: HashMap<MethodTarget, Vec<&CallSite>> = HashMap::new();
        for call in self
            .render
            .map
            .calls
            .iter()
            .filter(|call| call.reference_support.is_supported())
        {
            if let Some(target) = oracle.resolve_call(call) {
                expected_method_calls.entry(target).or_default().push(call);
            }
        }
        let mut expected_constant_refs: HashMap<String, Vec<&ConstantRefSite>> = HashMap::new();
        for constant_ref in &self.render.map.constant_refs {
            if let Some(target) = oracle.resolve_constant_ref(constant_ref) {
                expected_constant_refs
                    .entry(target)
                    .or_default()
                    .push(constant_ref);
            }
        }

        for (target, def) in &self.render.map.defs {
            if !self.project.method_enabled(target) {
                continue;
            }

            let expected_calls = expected_method_calls
                .get(target)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if expected_calls.is_empty() {
                continue;
            }

            let owner = method_owner_fqn(target);
            let method = ruby_method(&target.name);
            let ranges = query.method_reference_ranges(&owner, &method);
            for call in expected_calls {
                assert!(
                    ranges
                        .iter()
                        .any(|range| range_matches_pos(&engine, *range, &call.pos)),
                    "Expected engine refs for {} at {}:{} to include call {}:{}:{}, got {}",
                    target.signature(),
                    def.file,
                    def.line,
                    call.pos.file,
                    call.pos.line,
                    call.pos.character,
                    format_ranges(&engine, &ranges)
                );
            }
        }

        for (target, _def) in &self.render.map.constants {
            if !self.project.constant_enabled(target) {
                continue;
            }

            let expected_refs = expected_constant_refs
                .get(target)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if expected_refs.is_empty() {
                continue;
            }

            let parts = ruby_parts(target);
            let ranges = query.constant_reference_ranges(&parts, &[]);
            for constant_ref in expected_refs {
                assert!(
                    ranges.iter().any(|range| range_matches_pos(
                        &engine,
                        *range,
                        &constant_ref.pos
                    )),
                    "Expected engine refs for constant {} to include {}:{}:{}, got {:?}",
                    target,
                    constant_ref.pos.file,
                    constant_ref.pos.line,
                    constant_ref.pos.character,
                    ranges
                );
            }
        }
    }

    pub fn check_types(&self) {
        let engine = self.engine.read();
        let query = engine.query();

        for type_assert in self
            .render
            .map
            .type_asserts
            .iter()
            .filter(|type_assert| type_assert.kind == TypeAssertKind::LocalAssignment)
        {
            if !self.project.method_enabled(&type_assert.owner) {
                continue;
            }
            let file_id = query.file_id(&type_assert.pos.file).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: generated type assert file `{}` is not registered. This is a bug because engine runner indexed every generated file. Fix: inspect EngineSimulationRunner::reindex_all.",
                    type_assert.pos.file
                )
            });
            let facts = query.type_facts_in_file(file_id);
            assert!(
                facts.iter().any(|fact| {
                    fact.ruby_type == expected_ruby_type(&type_assert.expected)
                        && matches!(
                            &fact.subject,
                            TypeSubject::Local { .. }
                                | TypeSubject::InstanceVariable { .. }
                                | TypeSubject::ClassVariable { .. }
                                | TypeSubject::GlobalVariable(_)
                        )
                        && fact_starts_on_line(&engine, fact, type_assert.pos.line)
                }),
                "Expected engine type fact `{}` at {}:{} for {}, got {:?}",
                type_assert.expected,
                type_assert.pos.file,
                type_assert.pos.line,
                type_assert.owner.signature(),
                facts
            );
        }
    }

    fn reindex_all(&mut self) {
        let known_namespaces = known_namespaces(&self.render.map);
        let files = self
            .render
            .files
            .iter()
            .map(|(file, content)| (file.clone(), content.clone()))
            .collect::<Vec<_>>();
        let mut direct_passes = Vec::new();

        for (file, content) in &files {
            let file_id = self.engine.write().register_file(SourceFileInput {
                path: Path::new(file).to_path_buf(),
                content: content.clone(),
                kind: SourceKind::Project,
            });
            let direct_facts = self.collect_direct_facts(file_id, file, content, &known_namespaces);
            self.engine.write().replace_facts(
                file_id,
                FileFacts {
                    symbols: direct_facts.symbols.clone(),
                    methods: direct_facts.methods.clone(),
                    method_visibility_overrides: direct_facts.method_visibility_overrides.clone(),
                    types: direct_facts.types.clone(),
                    graph_nodes: direct_facts.graph_nodes.clone(),
                    graph_edges: direct_facts.graph_edges.clone(),
                    unresolved_graph_edges: direct_facts.unresolved_graph_edges.clone(),
                    reference_candidates: Vec::new(),
                    diagnostic_candidates: Vec::new(),
                    diagnostics: Vec::new(),
                    execution_contexts: Vec::new(),
                    inference: Default::default(),
                    local_read_types: Default::default(),
                },
                ResolveMode::Deferred,
            );
            direct_passes.push((file.clone(), content.clone(), file_id, direct_facts));
        }
        self.engine.write().resolve();

        for (file, content, file_id, direct_facts) in direct_passes {
            let uri = Url::parse(&format!("file:///sim/{}", file)).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: generated file URI for `{}` is invalid: {}. This is a bug because simulation file paths must be URL-safe. Fix: update file path generation.",
                    file, err
                )
            });
            let document = RubyDocument::with_analysis_file_id(uri, content.clone(), 1, file_id);
            let parse = ruby_prism::parse(content.as_bytes());
            let mut visitor = FactCollector::analysis_only(
                document,
                Arc::new(NullFactCollectorExtensionHost),
                self.engine.clone(),
            );
            visitor.visit(&parse.node());
            let local_read_types = visitor.local_read_type_evidence();
            let inference = visitor.inference_evidence();
            let mut types = direct_facts.types;
            types.extend(visitor.direct_facts.types);
            types.extend(visitor.type_store.all_facts());

            let facts = FileFacts {
                symbols: direct_facts.symbols,
                methods: direct_facts.methods,
                method_visibility_overrides: direct_facts.method_visibility_overrides,
                types,
                graph_nodes: direct_facts.graph_nodes,
                graph_edges: direct_facts.graph_edges,
                unresolved_graph_edges: direct_facts.unresolved_graph_edges,
                reference_candidates: visitor.reference_candidates,
                diagnostic_candidates: visitor.diagnostic_candidates,
                diagnostics: visitor.analysis_diagnostics,
                execution_contexts: visitor.extension_execution_context_facts,
                inference,
                local_read_types,
            };
            self.engine
                .write()
                .replace_facts(file_id, facts, ResolveMode::Deferred);
        }
        self.engine.write().resolve();
    }

    fn collect_direct_facts(
        &self,
        file_id: ruby_analysis::core::SourceFileId,
        _file: &str,
        content: &str,
        known_namespaces: &HashSet<FullyQualifiedName>,
    ) -> AnalysisIndex {
        AnalysisIndexer::with_known_namespaces(file_id, known_namespaces.clone())
            .index_source(content)
    }

    fn def_pos(&self, target: &MethodTarget) -> &SourcePos {
        self.render.map.defs.get(target).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: definition for `{}` is missing from source map. This is a bug because engine sim expected a generated method. Fix: inspect project fixture.",
                target.signature()
            )
        })
    }

    fn const_def_pos(&self, fqn: &str) -> &SourcePos {
        self.render.map.constants.get(fqn).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: definition for constant `{}` is missing from source map. This is a bug because engine sim expected a generated constant. Fix: inspect project fixture.",
                fqn
            )
        })
    }
}

fn known_namespaces(map: &SourceMap) -> HashSet<FullyQualifiedName> {
    let mut known = HashSet::new();
    for fqn in map.namespaces.keys() {
        let namespace = FullyQualifiedName::namespace(ruby_parts(fqn));
        known.insert(namespace.clone());
        known.insert(namespace.to_singleton_namespace().expect(
            "INVARIANT VIOLATED: generated namespace could not convert to singleton. This is a bug because namespace FQNs must support singleton form. Fix: inspect FQN generation.",
        ));
    }
    known
}

fn lookup_owner_for_call(call: &super::ruby_gen::CallSite) -> Option<FullyQualifiedName> {
    match &call.shape {
        CallShape::Bare
        | CallShape::BareInDoBlock
        | CallShape::BareInBraceBlock
        | CallShape::BareInLambda
        | CallShape::BareInProc
        | CallShape::FrameworkRouteBlock
        | CallShape::Super => Some(instance_namespace(&call.caller.owner)),
        CallShape::LocalVar { .. }
        | CallShape::Ivar { .. }
        | CallShape::ConstructorSend
        | CallShape::StaticSend
        | CallShape::ArrayBlockParam { .. }
        | CallShape::YieldBlockParam { .. } => Some(instance_namespace(&call.target.owner)),
        CallShape::ClassSend => Some(singleton_namespace(&call.target.owner)),
        CallShape::MethodObject => match call.target.kind {
            MethodKind::Class => Some(singleton_namespace(&call.target.owner)),
            MethodKind::Instance => Some(instance_namespace(&call.caller.owner)),
        },
        CallShape::InstanceMethodObject => Some(instance_namespace(&call.target.owner)),
        CallShape::ClassReceiver { receiver_owner } => Some(singleton_namespace(receiver_owner)),
        CallShape::OneHopChain { .. } => Some(instance_namespace(&call.target.owner)),
        CallShape::ReceiverLocalVar { receiver_owner, .. } => {
            Some(instance_namespace(receiver_owner))
        }
    }
}

fn method_owner_fqn(target: &MethodTarget) -> FullyQualifiedName {
    match target.kind {
        MethodKind::Instance => instance_namespace(&target.owner),
        MethodKind::Class => singleton_namespace(&target.owner),
    }
}

fn instance_namespace(fqn: &str) -> FullyQualifiedName {
    FullyQualifiedName::namespace_with_kind(ruby_parts(fqn), CoreNamespaceKind::Instance)
}

fn singleton_namespace(fqn: &str) -> FullyQualifiedName {
    FullyQualifiedName::namespace_with_kind(ruby_parts(fqn), CoreNamespaceKind::Singleton)
}

fn ruby_parts(fqn: &str) -> Vec<RubyConstant> {
    if fqn.is_empty() {
        return Vec::new();
    }
    fqn.split("::")
        .map(|part| {
            RubyConstant::new(part).unwrap_or_else(|_| {
                panic!(
                    "INVARIANT VIOLATED: generated constant segment `{}` is invalid. This is a bug because generated Ruby FQNs must use valid constants. Fix: inspect project fixture.",
                    part
                )
            })
        })
        .collect()
}

fn ruby_method(name: &str) -> RubyMethod {
    RubyMethod::new(name).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: generated method `{}` is invalid. This is a bug because generated Ruby methods must be valid. Fix: inspect project fixture.",
            name
        )
    })
}

fn expected_ruby_type(name: &str) -> RubyType {
    match name {
        "String" => RubyType::string(),
        "Integer" => RubyType::integer(),
        "Float" => RubyType::float(),
        "Symbol" => RubyType::symbol(),
        "NilClass" => RubyType::nil_class(),
        class_name => RubyType::Class(FullyQualifiedName::try_from(class_name).unwrap_or_else(
            |_| {
                panic!(
                    "INVARIANT VIOLATED: generated expected type `{}` is invalid. This is a bug because sim return types must be valid FQNs. Fix: inspect project fixture.",
                    class_name
                )
            },
        )),
    }
}

fn fact_starts_on_line(
    engine: &AnalysisEngine,
    fact: &ruby_analysis::core::TypeFact,
    line: u32,
) -> bool {
    let Some(file) = engine.file(fact.range.file_id) else {
        return false;
    };
    file.byte_offset_to_line_character(fact.range.start_byte)
        .is_some_and(|(fact_line, _character)| fact_line == line)
}

fn range_matches_pos(engine: &AnalysisEngine, range: TextRange, pos: &SourcePos) -> bool {
    let Some(file) = engine.file(range.file_id) else {
        return false;
    };
    if !file.path.ends_with(&pos.file) {
        return false;
    }
    file.byte_offset_to_line_character(range.start_byte)
        .is_some_and(|(line, _character)| line == pos.line)
}

fn format_ranges(engine: &AnalysisEngine, ranges: &[TextRange]) -> String {
    let mut out = Vec::new();
    for range in ranges {
        let Some(file) = engine.file(range.file_id) else {
            out.push(format!("{range:?} missing-file"));
            continue;
        };
        let Some((line, character)) = file.byte_offset_to_line_character(range.start_byte) else {
            out.push(format!("{range:?} missing-position"));
            continue;
        };
        out.push(format!(
            "{}:{}:{} ({:?})",
            file.path.display(),
            line,
            character,
            range
        ));
    }
    format!("[{}]", out.join(", "))
}
