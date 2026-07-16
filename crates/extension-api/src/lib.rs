use serde::{Deserialize, Serialize};

pub const ABI_VERSION: u32 = 1;

pub trait Extension {
    fn id(&self) -> &'static str;
    fn indexed_call_names(&self) -> &'static [&'static str];
    fn abi_version(&self) -> u32 {
        ABI_VERSION
    }
    fn index_call(&self, ctx: &CallContext) -> Vec<IndexPatch>;

    fn index_call_output(&self, ctx: &CallContext) -> ExtensionOutput {
        ExtensionOutput::index_patches(self.index_call(ctx))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionEvent {
    pub event: String,
    pub call: Option<CallContext>,
    pub document: Option<DocumentContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<WatchedFileChange>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_results: Option<Vec<ProcessResult>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WatchedFileChange {
    pub workspace_root: String,
    pub path: String,
    pub uri: String,
    pub kind: WatchedFileChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WatchedFileChangeKind {
    Created,
    Changed,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionOutput {
    pub index_patches: Vec<IndexPatch>,
    #[serde(default)]
    pub execution_contexts: Vec<BlockExecutionContextPatch>,
    pub response_patches: Vec<ResponsePatch>,
    pub command_patches: Vec<CommandPatch>,
    #[serde(default)]
    pub process_requests: Vec<ProcessRequest>,
    #[serde(default)]
    pub reindex_files: Vec<ReindexFile>,
}

impl ExtensionOutput {
    pub fn index_patches(index_patches: Vec<IndexPatch>) -> Self {
        Self {
            index_patches,
            execution_contexts: Vec::new(),
            response_patches: Vec::new(),
            command_patches: Vec::new(),
            process_requests: Vec::new(),
            reindex_files: Vec::new(),
        }
    }
}

/// A framework-neutral description of how Ruby code inside one call's block
/// executes. Lexical constant lookup and local closure behavior remain
/// independent from the runtime receiver and the owner used by `def`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockExecutionContextPatch {
    pub call_range: SourceRange,
    pub block_range: SourceRange,
    pub generated_owners: Vec<GeneratedOwnerPatch>,
    pub implicit_receiver: ExecutionContextTarget,
    pub method_definition_owner: ExecutionContextTarget,
    pub lexical_scope: LexicalScopeMode,
    pub local_scope: LocalScopeMode,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedOwnerPatch {
    /// Guest-local stable identity. The host combines this with extension and
    /// source identity to construct a collision-proof engine owner.
    pub local_id: String,
    #[serde(default, skip_serializing_if = "GeneratedOwnerScope::is_source")]
    pub scope: GeneratedOwnerScope,
    pub declaration_kind: NamespaceDeclarationKind,
    pub owner_kind: NamespaceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ExecutionContextTarget>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GeneratedOwnerScope {
    #[default]
    Source,
    Project,
}

impl GeneratedOwnerScope {
    fn is_source(scope: &Self) -> bool {
        matches!(scope, Self::Source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionContextTarget {
    Namespace {
        namespace: Vec<String>,
        owner_kind: NamespaceKind,
    },
    GeneratedOwner {
        local_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner_kind: Option<NamespaceKind>,
    },
    ProjectGeneratedOwner {
        local_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner_kind: Option<NamespaceKind>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexicalScopeMode {
    Preserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalScopeMode {
    Preserve,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReindexFile {
    pub workspace_root: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub request_id: String,
    pub command: String,
    pub arguments: Vec<String>,
    pub stdin: Option<String>,
    pub workspace_root: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessResult {
    pub request_id: String,
    pub status: ProcessResultStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessResultStatus {
    Exited,
    TimedOut,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallContext {
    /// Isolated Ruby project and dependency context for this source. Older ABI
    /// v1 guests decode this addition as absent; applicability-aware guests
    /// must fail closed when it is unavailable or incomplete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectContext>,
    pub method_name: String,
    pub receiver: Receiver,
    pub arguments: Vec<Argument>,
    pub current_namespace: Vec<String>,
    pub namespace_kind: NamespaceKind,
    pub call_range: SourceRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_range: Option<SourceRange>,
    pub message_range: SourceRange,
    pub resolved_callees: Vec<ResolvedCallee>,
    pub enclosing_calls: Vec<ResolvedCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectContext {
    pub project_uri: String,
    pub source_uri: String,
    pub source_kind: ProjectSourceKind,
    pub workspace_trusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruby_version: Option<String>,
    pub lockfile_present: bool,
    pub locked_gems_complete: bool,
    #[serde(default)]
    pub locked_gems: Vec<LockedGem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectSourceKind {
    Project,
    Gem,
    Stdlib,
    Stub,
    Signature,
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LockedGem {
    pub name: String,
    pub version: String,
    pub source: LockedGemSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LockedGemSource {
    Registry,
    Git,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCall {
    pub method_name: String,
    pub receiver: Receiver,
    #[serde(default)]
    pub arguments: Vec<Argument>,
    pub resolved_callees: Vec<ResolvedCallee>,
    pub call_range: SourceRange,
    pub message_range: SourceRange,
    /// Extension IDs whose validated lexical frame owns this enclosing call.
    /// The field is host-derived and allows overlapping DSLs (for example,
    /// RSpec and Minitest `describe`) to coexist without treating every active
    /// extension frame as globally shared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frame_extension_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCallee {
    pub owner: Vec<String>,
    pub owner_kind: NamespaceKind,
    pub method: String,
    pub resolution: CalleeResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalleeResolution {
    Exact,
    ReceiverOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentContext {
    pub uri: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Receiver {
    None,
    SelfReceiver,
    Constant(Vec<String>),
    LocalVariable(String),
    InstanceVariable(String),
    ClassVariable(String),
    GlobalVariable(String),
    MethodCall { method_name: String },
    Literal,
    Expression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<Keyword>,
    pub value: ArgumentValue,
    pub range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keyword {
    pub name: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArgumentValue {
    Symbol(String),
    String(String),
    Constant(Vec<String>),
    Boolean(bool),
    Nil,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexPatch {
    DefineNamespace(DefineNamespacePatch),
    DefineConstant(DefineConstantPatch),
    AddReference(ReferencePatch),
    DefineMethod(DefineMethodPatch),
    SetSuperclass(SetSuperclassPatch),
    ApplyMixin(ApplyMixinPatch),
    ConnectExecutionContext(ConnectExecutionContextPatch),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponsePatch {
    Diagnostic(DiagnosticPatch),
    CodeLens(CodeLensPatch),
    DocumentSymbol(DocumentSymbolPatch),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPatch {
    pub message: String,
    pub range: SourceRange,
    pub severity: DiagnosticSeverity,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLensPatch {
    pub title: String,
    pub command: String,
    pub range: SourceRange,
    pub arguments: Vec<String>,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSymbolPatch {
    pub name: String,
    pub detail: Option<String>,
    pub kind: String,
    pub range: SourceRange,
    pub selection_range: SourceRange,
    pub source: PatchSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPatch {
    RunTerminal(RunTerminalPatch),
    LaunchDebug(LaunchDebugPatch),
    ShowNotification(ShowNotificationPatch),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTerminalPatch {
    pub command: String,
    pub arguments: Vec<String>,
    pub cwd: Option<String>,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchDebugPatch {
    pub name: String,
    pub command: String,
    pub arguments: Vec<String>,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowNotificationPatch {
    pub message: String,
    pub level: NotificationLevel,
    pub source: PatchSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefineMethodPatch {
    pub name: String,
    pub namespace: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_target: Option<ExecutionContextTarget>,
    pub owner_kind: NamespaceKind,
    pub visibility: MethodVisibility,
    pub location: SourceRange,
    #[serde(default)]
    pub params: Vec<MethodParamPatch>,
    pub return_type: Option<RubyType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type_source: Option<MethodReturnTypeSource>,
    pub source: PatchSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MethodReturnTypeSource {
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefineNamespacePatch {
    pub namespace: Vec<String>,
    pub kind: NamespaceDeclarationKind,
    pub location: SourceRange,
    pub source: PatchSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamespaceDeclarationKind {
    Class,
    Module,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefineConstantPatch {
    pub namespace: Vec<String>,
    pub name: String,
    pub location: SourceRange,
    pub ruby_type: Option<RubyType>,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferencePatch {
    pub target: ReferenceTarget,
    pub location: SourceRange,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceTarget {
    Namespace(Vec<String>),
    Constant {
        namespace: Vec<String>,
        name: String,
    },
    Method {
        namespace: Vec<String>,
        owner_kind: NamespaceKind,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodParamPatch {
    pub name: String,
    pub kind: MethodParamKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MethodParamKind {
    Required,
    Optional,
    Rest,
    RequiredKeyword,
    OptionalKeyword,
    KeywordRest,
    Block,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_argument_round_trips_with_key_and_value_ranges() {
        let json = r#"{
            "keyword":{"name":"class_name","range":{"start":{"line":1,"character":21},"end":{"line":1,"character":32}}},
            "value":{"String":"Billing::Account"},
            "range":{"start":{"line":1,"character":33},"end":{"line":1,"character":49}}
        }"#;
        let argument: Argument = serde_json::from_str(json).expect("keyword argument must parse");
        let keyword = argument
            .keyword
            .as_ref()
            .expect("keyword metadata must be retained");
        assert_eq!(keyword.name, "class_name");
        assert_eq!(keyword.range.start.character, 21);
        assert_eq!(argument.range.start.character, 33);
        assert_eq!(
            serde_json::from_str::<Argument>(&serde_json::to_string(&argument).unwrap()).unwrap(),
            argument
        );

        let legacy = r#"{
            "value":{"Symbol":"account"},
            "range":{"start":{"line":1,"character":11},"end":{"line":1,"character":19}}
        }"#;
        assert_eq!(
            serde_json::from_str::<Argument>(legacy)
                .expect("pre-keyword ABI argument must remain compatible")
                .keyword,
            None
        );
    }

    #[test]
    fn enclosing_call_preserves_literal_arguments_for_dsl_frames() {
        let json = r#"{
            "method_name":"namespace",
            "receiver":"None",
            "arguments":[{"value":{"Symbol":"admin"},"range":{"start":{"line":1,"character":12},"end":{"line":1,"character":18}}}],
            "resolved_callees":[],
            "call_range":{"start":{"line":1,"character":2},"end":{"line":3,"character":5}},
            "message_range":{"start":{"line":1,"character":2},"end":{"line":1,"character":11}}
        }"#;
        let call: ResolvedCall =
            serde_json::from_str(json).expect("enclosing DSL frame must deserialize");
        assert_eq!(call.arguments.len(), 1);
        assert_eq!(
            call.arguments[0].value,
            ArgumentValue::Symbol("admin".to_string())
        );
        assert!(call.frame_extension_ids.is_empty());

        let mut owned_call = call;
        owned_call.frame_extension_ids = vec!["rspec-ruby".to_string()];
        assert_eq!(
            serde_json::from_str::<ResolvedCall>(
                &serde_json::to_string(&owned_call).expect("owned frame call must encode")
            )
            .expect("owned frame call must decode"),
            owned_call
        );
    }

    #[test]
    fn block_execution_context_round_trips_independent_runtime_owners() {
        let output = ExtensionOutput {
            index_patches: Vec::new(),
            execution_contexts: vec![BlockExecutionContextPatch {
                call_range: SourceRange {
                    start: SourcePosition {
                        line: 1,
                        character: 2,
                    },
                    end: SourcePosition {
                        line: 8,
                        character: 5,
                    },
                },
                block_range: SourceRange {
                    start: SourcePosition {
                        line: 1,
                        character: 21,
                    },
                    end: SourcePosition {
                        line: 8,
                        character: 5,
                    },
                },
                generated_owners: vec![GeneratedOwnerPatch {
                    local_id: "example-group:1:2".to_string(),
                    scope: GeneratedOwnerScope::Source,
                    declaration_kind: NamespaceDeclarationKind::Class,
                    owner_kind: NamespaceKind::Instance,
                    parent: Some(ExecutionContextTarget::Namespace {
                        namespace: vec![
                            "RSpec".to_string(),
                            "Core".to_string(),
                            "ExampleGroup".to_string(),
                        ],
                        owner_kind: NamespaceKind::Instance,
                    }),
                }],
                implicit_receiver: ExecutionContextTarget::GeneratedOwner {
                    local_id: "example-group:1:2".to_string(),
                    owner_kind: Some(NamespaceKind::Singleton),
                },
                method_definition_owner: ExecutionContextTarget::Namespace {
                    namespace: vec!["SpecDefinitions".to_string()],
                    owner_kind: NamespaceKind::Singleton,
                },
                lexical_scope: LexicalScopeMode::Preserve,
                local_scope: LocalScopeMode::Preserve,
                source: PatchSource {
                    extension_id: "rspec-ruby".to_string(),
                    macro_name: "describe".to_string(),
                },
            }],
            response_patches: Vec::new(),
            command_patches: Vec::new(),
            process_requests: Vec::new(),
            reindex_files: Vec::new(),
        };

        let json = serde_json::to_string(&output).expect("execution context output must encode");
        let decoded: ExtensionOutput =
            serde_json::from_str(&json).expect("execution context output must decode");
        assert_eq!(decoded, output);

        let legacy = r#"{
            "index_patches":[],
            "response_patches":[],
            "command_patches":[]
        }"#;
        assert!(serde_json::from_str::<ExtensionOutput>(legacy)
            .expect("ABI v1 output without execution contexts must remain compatible")
            .execution_contexts
            .is_empty());
    }

    #[test]
    fn generated_owner_scope_defaults_to_source_and_project_targets_round_trip() {
        let legacy = r#"{
            "local_id":"group:1:2",
            "declaration_kind":"Class",
            "owner_kind":"Instance",
            "parent":null
        }"#;
        let legacy_owner: GeneratedOwnerPatch =
            serde_json::from_str(legacy).expect("ABI v1 generated owner without scope must decode");
        assert_eq!(legacy_owner.scope, GeneratedOwnerScope::Source);

        let target = ExecutionContextTarget::ProjectGeneratedOwner {
            local_id: "shared-context:authenticated".to_string(),
            owner_kind: Some(NamespaceKind::Instance),
        };
        assert_eq!(
            serde_json::from_str::<ExecutionContextTarget>(
                &serde_json::to_string(&target).expect("project target must encode")
            )
            .expect("project target must decode"),
            target
        );
    }

    #[test]
    fn call_context_project_metadata_is_backward_compatible_and_typed() {
        let legacy = r#"{
            "method_name":"describe",
            "receiver":{"Constant":["RSpec"]},
            "arguments":[],
            "current_namespace":[],
            "namespace_kind":"Instance",
            "call_range":{"start":{"line":0,"character":0},"end":{"line":0,"character":20}},
            "block_range":null,
            "message_range":{"start":{"line":0,"character":6},"end":{"line":0,"character":14}},
            "resolved_callees":[],
            "enclosing_calls":[]
        }"#;
        let mut context: CallContext = serde_json::from_str(legacy)
            .expect("ABI v1 CallContext without project metadata must decode");
        assert!(context.project.is_none());

        context.project = Some(ProjectContext {
            project_uri: "file:///workspace/service".to_string(),
            source_uri: "file:///workspace/service/spec/user_spec.rb".to_string(),
            source_kind: ProjectSourceKind::Project,
            workspace_trusted: true,
            ruby_version: Some("3.3".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![LockedGem {
                name: "rspec-core".to_string(),
                version: "3.13.1".to_string(),
                source: LockedGemSource::Registry,
            }],
        });
        let encoded = serde_json::to_vec(&context).expect("project-aware CallContext must encode");
        assert_eq!(
            serde_json::from_slice::<CallContext>(&encoded)
                .expect("project-aware CallContext must decode"),
            context
        );
    }

    #[test]
    fn activation_project_metadata_is_additive_and_round_trips() {
        let legacy = r#"{
            "event":"lifecycle.activate",
            "call":null,
            "document":null
        }"#;
        let mut event: ExtensionEvent = serde_json::from_str(legacy)
            .expect("ABI v1 activation without project metadata must decode");
        assert!(event.project.is_none());

        event.project = Some(ProjectContext {
            project_uri: "file:///workspace/service".to_string(),
            source_uri: "file:///workspace/service".to_string(),
            source_kind: ProjectSourceKind::Project,
            workspace_trusted: true,
            ruby_version: Some("3.3".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![LockedGem {
                name: "rspec-core".to_string(),
                version: "3.13.6".to_string(),
                source: LockedGemSource::Registry,
            }],
        });
        assert_eq!(
            serde_json::from_slice::<ExtensionEvent>(
                &serde_json::to_vec(&event).expect("project activation must encode")
            )
            .expect("project activation must decode"),
            event
        );
    }

    #[test]
    fn execution_context_connection_round_trips_as_domain_patch() {
        let range = SourceRange {
            start: SourcePosition {
                line: 3,
                character: 2,
            },
            end: SourcePosition {
                line: 3,
                character: 29,
            },
        };
        let patch = IndexPatch::ConnectExecutionContext(ConnectExecutionContextPatch {
            template: ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "shared-examples-runtime:auditable".to_string(),
                owner_kind: Some(NamespaceKind::Singleton),
            },
            application: ExecutionContextTarget::GeneratedOwner {
                local_id: "example-group:1:0-8:3".to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            },
            location: range,
            source: PatchSource {
                extension_id: "rspec-ruby".to_string(),
                macro_name: "it_behaves_like".to_string(),
            },
        });
        let encoded = serde_json::to_vec(&patch)
            .expect("execution-context connection must serialize through ABI v1");
        assert_eq!(
            serde_json::from_slice::<IndexPatch>(&encoded)
                .expect("execution-context connection must deserialize through ABI v1"),
            patch
        );
    }

    #[test]
    fn method_block_return_source_round_trips_and_is_backward_compatible() {
        let legacy = r#"{
            "name":"user",
            "namespace":["UserSpec"],
            "owner_kind":"Instance",
            "visibility":"Public",
            "location":{"start":{"line":1,"character":6},"end":{"line":1,"character":10}},
            "params":[],
            "return_type":null,
            "source":{"extension_id":"rspec-ruby","macro_name":"let"}
        }"#;
        let mut method: DefineMethodPatch = serde_json::from_str(legacy)
            .expect("ABI v1 method patch without a return source must decode");
        assert_eq!(method.return_type_source, None);

        method.return_type_source = Some(MethodReturnTypeSource::Block);
        let encoded =
            serde_json::to_vec(&method).expect("block-derived method return source must encode");
        assert_eq!(
            serde_json::from_slice::<DefineMethodPatch>(&encoded)
                .expect("block-derived method return source must decode"),
            method
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyMixinPatch {
    pub namespace: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_target: Option<ExecutionContextTarget>,
    pub target_kind: NamespaceKind,
    /// Exact semantic mixin target for generated/project-scoped owners. When
    /// present, `mixin` must be empty; ordinary Ruby namespaces continue to use
    /// the backward-compatible `mixin` path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mixin_target: Option<ExecutionContextTarget>,
    pub mixin: Vec<String>,
    pub absolute: bool,
    pub kind: MixinKind,
    pub location: SourceRange,
    pub source: PatchSource,
}

/// Connects a reusable execution template to one concrete runtime owner.
///
/// Unlike a Ruby mixin, this relationship does not alter ordinary MRO. Method
/// lookup first searches the template receiver and only then searches every
/// connected application independently, preserving ambiguity across multiple
/// DSL instantiations instead of selecting one by traversal order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectExecutionContextPatch {
    pub template: ExecutionContextTarget,
    pub application: ExecutionContextTarget,
    pub location: SourceRange,
    pub source: PatchSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetSuperclassPatch {
    pub namespace: Vec<String>,
    pub superclass: Vec<String>,
    pub absolute: bool,
    pub location: SourceRange,
    pub source: PatchSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MixinKind {
    Include,
    Prepend,
    Extend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchSource {
    pub extension_id: String,
    pub macro_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamespaceKind {
    Instance,
    Singleton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MethodVisibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RubyType {
    Named(String),
    Array(Vec<RubyType>),
    Hash {
        keys: Vec<RubyType>,
        values: Vec<RubyType>,
    },
    Union(Vec<RubyType>),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePosition {
    pub line: u32,
    pub character: u32,
}
