use serde::{Deserialize, Serialize};

pub const ABI_VERSION: u32 = 1;

pub trait Extension {
    fn id(&self) -> &'static str;
    fn indexed_call_names(&self) -> &'static [&'static str];
    fn abi_version(&self) -> u32 {
        ABI_VERSION
    }
    fn index_call(&self, ctx: &CallContext) -> Vec<IndexPatch>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionEvent {
    pub event: String,
    pub call: Option<CallContext>,
    pub document: Option<DocumentContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionOutput {
    pub index_patches: Vec<IndexPatch>,
    pub response_patches: Vec<ResponsePatch>,
    pub command_patches: Vec<CommandPatch>,
}

impl ExtensionOutput {
    pub fn index_patches(index_patches: Vec<IndexPatch>) -> Self {
        Self {
            index_patches,
            response_patches: Vec::new(),
            command_patches: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallContext {
    pub method_name: String,
    pub receiver: Receiver,
    pub arguments: Vec<Argument>,
    pub current_namespace: Vec<String>,
    pub namespace_kind: NamespaceKind,
    pub call_range: SourceRange,
    pub message_range: SourceRange,
    pub resolved_callees: Vec<ResolvedCallee>,
    pub enclosing_calls: Vec<ResolvedCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCall {
    pub method_name: String,
    pub receiver: Receiver,
    pub resolved_callees: Vec<ResolvedCallee>,
    pub call_range: SourceRange,
    pub message_range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCallee {
    pub owner: Vec<String>,
    pub owner_kind: NamespaceKind,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentContext {
    pub uri: String,
    pub text: String,
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
    pub value: ArgumentValue,
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
    DefineMethod(DefineMethodPatch),
    ApplyMixin(ApplyMixinPatch),
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
    pub owner_kind: NamespaceKind,
    pub visibility: MethodVisibility,
    pub location: SourceRange,
    #[serde(default)]
    pub params: Vec<MethodParamPatch>,
    pub return_type: Option<RubyType>,
    pub source: PatchSource,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyMixinPatch {
    pub namespace: Vec<String>,
    pub target_kind: NamespaceKind,
    pub mixin: Vec<String>,
    pub absolute: bool,
    pub kind: MixinKind,
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
