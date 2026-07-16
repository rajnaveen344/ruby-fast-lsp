use ruby_fast_lsp_extension_api::{
    ArgumentValue, BlockExecutionContextPatch, CallContext, CalleeResolution, CodeLensPatch,
    DefineMethodPatch, DocumentContext, DocumentSymbolPatch, ExecutionContextTarget,
    ExtensionEvent, ExtensionOutput, GeneratedOwnerPatch, GeneratedOwnerScope, IndexPatch,
    LexicalScopeMode, LocalScopeMode, MethodReturnTypeSource, MethodVisibility,
    NamespaceDeclarationKind, NamespaceKind, PatchSource, Receiver, ResolvedCall, ResponsePatch,
    SourcePosition, SourceRange,
};
use ruby_fast_lsp_extension_guest_sdk::GuestExtension;

pub const EXTENSION_ID: &str = "minitest-ruby";

pub fn extension() -> MinitestExtension {
    MinitestExtension
}

pub struct MinitestExtension;

impl GuestExtension for MinitestExtension {
    fn indexed_call_names(&self) -> &'static [&'static str] {
        &[
            "describe", "it", "specify", "let", "subject", "before", "after",
        ]
    }

    fn index_call(&mut self, context: &CallContext) -> ExtensionOutput {
        minitest_call_output(context)
    }

    fn handle_event(&mut self, event: &ExtensionEvent) -> ExtensionOutput {
        if event.event == "index.call.enter" {
            let context = event.call.as_ref().expect(
                "INVARIANT VIOLATED: Minitest index.call.enter omitted CallContext. This is a host/guest ABI bug because call events require their typed payload. Fix: encode CallContext for every index.call.enter event.",
            );
            return self.index_call(context);
        }
        let Some(document) = event.document.as_ref() else {
            return ExtensionOutput::index_patches(Vec::new());
        };
        match event.event.as_str() {
            "request.document_symbol" => document_symbol_output(document),
            "request.code_lens" => code_lens_output(document),
            "activate" | "deactivate" | "settings.changed" | "files.changed"
            | "process.completed" => ExtensionOutput::index_patches(Vec::new()),
            _ => ExtensionOutput::index_patches(Vec::new()),
        }
    }
}

fn minitest_call_output(context: &CallContext) -> ExtensionOutput {
    let mut output = ExtensionOutput::index_patches(Vec::new());
    if context.method_name == "describe" && is_minitest_describe_context(context) {
        if let Some(execution_context) = describe_execution_context(context) {
            output.execution_contexts.push(execution_context);
        }
        return output;
    }
    if !inside_minitest_group(context) {
        return output;
    }
    if matches!(context.method_name.as_str(), "let" | "subject") {
        output.index_patches.extend(define_named_helper(context));
    }
    if matches!(
        context.method_name.as_str(),
        "it" | "specify" | "let" | "subject" | "before" | "after"
    ) {
        if let Some(execution_context) = runtime_execution_context(context) {
            output.execution_contexts.push(execution_context);
        }
    }
    output
}

fn is_minitest_describe_context(context: &CallContext) -> bool {
    let receiver_can_start_group = match &context.receiver {
        Receiver::None | Receiver::SelfReceiver => true,
        Receiver::Constant(parts) => {
            parts.as_slice() == ["Minitest", "Spec"]
                || parts.as_slice() == ["Minitest", "Spec", "DSL"]
        }
        Receiver::LocalVariable(_)
        | Receiver::InstanceVariable(_)
        | Receiver::ClassVariable(_)
        | Receiver::GlobalVariable(_)
        | Receiver::MethodCall { .. }
        | Receiver::Literal
        | Receiver::Expression => false,
    };
    (receiver_can_start_group && is_exact_minitest_callee(context, "describe"))
        || inside_minitest_group(context)
}

fn is_exact_minitest_callee(context: &CallContext, method: &str) -> bool {
    context
        .resolved_callees
        .iter()
        .any(|callee| is_exact_minitest_callee_parts(callee, method))
}

fn inside_minitest_group(context: &CallContext) -> bool {
    context
        .enclosing_calls
        .iter()
        .find(|call| call.method_name == "describe")
        .is_some_and(is_exact_minitest_describe_call)
}

fn is_minitest_describe_call(call: &ResolvedCall) -> bool {
    call.method_name == "describe"
        && (matches!(
            call.receiver,
            ruby_fast_lsp_extension_api::Receiver::None
                | ruby_fast_lsp_extension_api::Receiver::SelfReceiver
        ) || is_exact_minitest_describe_call(call))
}

fn is_exact_minitest_describe_call(call: &ResolvedCall) -> bool {
    call.resolved_callees
        .iter()
        .any(|callee| is_exact_minitest_callee_parts(callee, "describe"))
}

fn is_exact_minitest_callee_parts(
    callee: &ruby_fast_lsp_extension_api::ResolvedCallee,
    method: &str,
) -> bool {
    if callee.method != method || callee.resolution != CalleeResolution::Exact {
        return false;
    }
    match (callee.owner.as_slice(), callee.owner_kind) {
        ([owner], NamespaceKind::Instance | NamespaceKind::Singleton) => {
            owner == "Kernel" || owner == "Object"
        }
        ([minitest, spec, dsl], NamespaceKind::Instance) => {
            minitest == "Minitest" && spec == "Spec" && dsl == "DSL"
        }
        ([], NamespaceKind::Instance | NamespaceKind::Singleton)
        | ([_, ..], NamespaceKind::Instance | NamespaceKind::Singleton) => false,
    }
}

fn describe_execution_context(context: &CallContext) -> Option<BlockExecutionContextPatch> {
    let block_range = context.block_range?;
    let (mut owners, enclosing_target) = generated_group_chain(context);
    let parent = enclosing_target.unwrap_or_else(minitest_spec_instance_target);
    let local_id = group_owner_local_id(context.call_range);
    owners.push(GeneratedOwnerPatch {
        local_id: local_id.clone(),
        scope: GeneratedOwnerScope::Source,
        declaration_kind: NamespaceDeclarationKind::Class,
        owner_kind: NamespaceKind::Instance,
        parent: Some(parent),
    });
    Some(BlockExecutionContextPatch {
        call_range: context.call_range,
        block_range,
        generated_owners: owners,
        implicit_receiver: ExecutionContextTarget::GeneratedOwner {
            local_id: local_id.clone(),
            owner_kind: Some(NamespaceKind::Singleton),
        },
        method_definition_owner: ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind: Some(NamespaceKind::Instance),
        },
        lexical_scope: LexicalScopeMode::Preserve,
        local_scope: LocalScopeMode::Preserve,
        source: source("describe"),
    })
}

fn runtime_execution_context(context: &CallContext) -> Option<BlockExecutionContextPatch> {
    let block_range = context.block_range?;
    let (owners, target) = generated_group_chain(context);
    let target = target?;
    Some(BlockExecutionContextPatch {
        call_range: context.call_range,
        block_range,
        generated_owners: owners,
        implicit_receiver: target.clone(),
        method_definition_owner: target,
        lexical_scope: LexicalScopeMode::Preserve,
        local_scope: LocalScopeMode::Preserve,
        source: source(context.method_name.as_str()),
    })
}

fn generated_group_chain(
    context: &CallContext,
) -> (Vec<GeneratedOwnerPatch>, Option<ExecutionContextTarget>) {
    let mut owners = Vec::new();
    let mut parent = minitest_spec_instance_target();
    for enclosing in context
        .enclosing_calls
        .iter()
        .filter(|call| is_minitest_describe_call(call))
    {
        let local_id = group_owner_local_id(enclosing.call_range);
        owners.push(GeneratedOwnerPatch {
            local_id: local_id.clone(),
            scope: GeneratedOwnerScope::Source,
            declaration_kind: NamespaceDeclarationKind::Class,
            owner_kind: NamespaceKind::Instance,
            parent: Some(parent),
        });
        parent = ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind: Some(NamespaceKind::Instance),
        };
    }
    let target = owners
        .last()
        .map(|owner| ExecutionContextTarget::GeneratedOwner {
            local_id: owner.local_id.clone(),
            owner_kind: Some(NamespaceKind::Instance),
        });
    (owners, target)
}

fn define_named_helper(context: &CallContext) -> Vec<IndexPatch> {
    let Some(owner_target) = current_group_target(context) else {
        return Vec::new();
    };
    let (name, location) = if context.method_name == "subject" {
        if !context.arguments.is_empty() {
            return Vec::new();
        }
        ("subject".to_string(), context.message_range)
    } else {
        let Some(argument) = context.arguments.first() else {
            return Vec::new();
        };
        let name = match &argument.value {
            ArgumentValue::Symbol(name) | ArgumentValue::String(name) => name.clone(),
            ArgumentValue::Constant(_)
            | ArgumentValue::Boolean(_)
            | ArgumentValue::Nil
            | ArgumentValue::Unsupported => return Vec::new(),
        };
        (name, argument.range)
    };
    vec![IndexPatch::DefineMethod(DefineMethodPatch {
        name,
        namespace: context.current_namespace.clone(),
        owner_target: Some(owner_target),
        owner_kind: NamespaceKind::Instance,
        visibility: MethodVisibility::Public,
        location,
        params: Vec::new(),
        return_type: None,
        return_type_source: Some(MethodReturnTypeSource::Block),
        source: source(context.method_name.as_str()),
    })]
}

fn current_group_target(context: &CallContext) -> Option<ExecutionContextTarget> {
    context
        .enclosing_calls
        .iter()
        .rev()
        .find(|call| is_minitest_describe_call(call))
        .map(|call| ExecutionContextTarget::GeneratedOwner {
            local_id: group_owner_local_id(call.call_range),
            owner_kind: Some(NamespaceKind::Instance),
        })
}

fn minitest_spec_instance_target() -> ExecutionContextTarget {
    ExecutionContextTarget::Namespace {
        namespace: vec!["Minitest".to_string(), "Spec".to_string()],
        owner_kind: NamespaceKind::Instance,
    }
}

fn group_owner_local_id(range: SourceRange) -> String {
    format!(
        "spec-group:{}:{}-{}:{}",
        range.start.line, range.start.character, range.end.line, range.end.character
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestNode {
    kind: TestNodeKind,
    label: String,
    test_name: Option<String>,
    range: SourceRange,
    selection_range: SourceRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestNodeKind {
    Class,
    SpecGroup,
    Method,
}

impl TestNodeKind {
    fn symbol_kind(self) -> &'static str {
        match self {
            Self::Class | Self::SpecGroup => "Class",
            Self::Method => "Method",
        }
    }
}

fn document_symbol_output(document: &DocumentContext) -> ExtensionOutput {
    let response_patches = test_nodes(document)
        .into_iter()
        .filter(|node| node.kind != TestNodeKind::Class)
        .map(|node| {
            ResponsePatch::DocumentSymbol(DocumentSymbolPatch {
                name: node.label,
                detail: None,
                kind: node.kind.symbol_kind().to_string(),
                range: node.range,
                selection_range: node.selection_range,
                source: source("minitest"),
            })
        })
        .collect();
    response_output(response_patches)
}

fn code_lens_output(document: &DocumentContext) -> ExtensionOutput {
    let mut response_patches = Vec::new();
    for node in test_nodes(document) {
        let line = (node.range.start.line + 1).to_string();
        let arguments = vec![
            document.uri.clone(),
            line,
            node.test_name.unwrap_or_default(),
        ];
        for (title, command) in [
            ("Run Minitest", "ruby-fast-lsp.minitest.run"),
            ("Debug Minitest", "ruby-fast-lsp.minitest.debug"),
        ] {
            response_patches.push(ResponsePatch::CodeLens(CodeLensPatch {
                title: title.to_string(),
                command: command.to_string(),
                range: node.selection_range,
                arguments: arguments.clone(),
                source: source("minitest"),
            }));
        }
    }
    response_output(response_patches)
}

fn response_output(response_patches: Vec<ResponsePatch>) -> ExtensionOutput {
    ExtensionOutput {
        index_patches: Vec::new(),
        execution_contexts: Vec::new(),
        response_patches,
        command_patches: Vec::new(),
        process_requests: Vec::new(),
        reindex_files: Vec::new(),
    }
}

fn test_nodes(document: &DocumentContext) -> Vec<TestNode> {
    if !is_test_file(&document.uri) {
        return Vec::new();
    }
    document
        .text
        .split('\n')
        .enumerate()
        .filter_map(|(line, text)| node_for_line(text.strip_suffix('\r').unwrap_or(text), line))
        .collect()
}

fn is_test_file(uri: &str) -> bool {
    uri.ends_with("_test.rb") || uri.contains("/test/")
}

fn node_for_line(line: &str, line_index: usize) -> Option<TestNode> {
    let stripped = line.trim_start_matches(char::is_whitespace);
    let indent = utf16_len(&line[..line.len() - stripped.len()]);
    if let Some(rest) = stripped.strip_prefix("class ") {
        let name = token_until(rest, &[' ', '<', '(', ';'])?;
        if !name
            .rsplit("::")
            .next()
            .is_some_and(|part| part.ends_with("Test"))
        {
            return None;
        }
        return Some(node(
            TestNodeKind::Class,
            name,
            None,
            line,
            line_index,
            indent,
            6,
            utf16_len(name),
        ));
    }
    if let Some(rest) = stripped.strip_prefix("def ") {
        let name = token_until(rest, &[' ', '(', '=', ';'])?;
        if !name.starts_with("test_") || name.len() <= 5 {
            return None;
        }
        return Some(node(
            TestNodeKind::Method,
            name,
            Some(name),
            line,
            line_index,
            indent,
            4,
            utf16_len(name),
        ));
    }
    if let Some(description) = dsl_description(stripped, "describe") {
        return Some(node(
            TestNodeKind::SpecGroup,
            &description,
            None,
            line,
            line_index,
            indent,
            0,
            8,
        ));
    }
    for method in ["it", "specify"] {
        if let Some(description) = dsl_description(stripped, method) {
            return Some(node(
                TestNodeKind::Method,
                &description,
                Some(&minitest_name_filter(&description)),
                line,
                line_index,
                indent,
                0,
                utf16_len(method),
            ));
        }
    }
    let rest = if let Some(rest) = stripped.strip_prefix("test ") {
        rest.trim_start()
    } else if let Some(rest) = stripped.strip_prefix("test(") {
        rest.trim_start()
    } else {
        return None;
    };
    let description = quoted_string(rest)?;
    if description.is_empty() {
        return None;
    }
    Some(node(
        TestNodeKind::Method,
        &description,
        Some(&format!("test_: {description}")),
        line,
        line_index,
        indent,
        0,
        4,
    ))
}

fn dsl_description(line: &str, method: &str) -> Option<String> {
    let rest = line.strip_prefix(method)?;
    let rest = if let Some(rest) = rest.strip_prefix('(') {
        rest.trim_start()
    } else if rest.chars().next().is_some_and(char::is_whitespace) {
        rest.trim_start()
    } else {
        return None;
    };
    quoted_string(rest)
}

fn minitest_name_filter(description: &str) -> String {
    let mut escaped = String::new();
    for character in description.chars() {
        if matches!(
            character,
            '\\' | '.'
                | '^'
                | '$'
                | '*'
                | '+'
                | '?'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '|'
                | '/'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    format!("/{escaped}/")
}

#[allow(clippy::too_many_arguments)]
fn node(
    kind: TestNodeKind,
    label: &str,
    test_name: Option<&str>,
    line: &str,
    line_index: usize,
    indent: u32,
    selection_offset: u32,
    selection_length: u32,
) -> TestNode {
    let line_end = utf16_len(line);
    let line_number = u32::try_from(line_index).expect(
        "INVARIANT VIOLATED: Minitest document line exceeds u32. This is a guest bug because LSP source positions use u32. Fix: reject documents beyond the protocol position domain before invoking extensions.",
    );
    TestNode {
        kind,
        label: label.to_string(),
        test_name: test_name.map(ToString::to_string),
        range: source_range(line_number, indent, line_end),
        selection_range: source_range(
            line_number,
            indent + selection_offset,
            indent + selection_offset + selection_length,
        ),
    }
}

fn source_range(line: u32, start: u32, end: u32) -> SourceRange {
    SourceRange {
        start: SourcePosition {
            line,
            character: start,
        },
        end: SourcePosition {
            line,
            character: end,
        },
    }
}

fn token_until<'a>(text: &'a str, delimiters: &[char]) -> Option<&'a str> {
    let end = text
        .char_indices()
        .find_map(|(index, character)| delimiters.contains(&character).then_some(index))
        .unwrap_or(text.len());
    (end > 0).then_some(&text[..end])
}

fn quoted_string(text: &str) -> Option<String> {
    let mut characters = text.chars();
    let quote = characters.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut escaped = false;
    let mut value = String::new();
    for character in characters {
        if escaped {
            value.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return Some(value);
        } else {
            value.push(character);
        }
    }
    None
}

fn utf16_len(value: &str) -> u32 {
    u32::try_from(value.encode_utf16().count()).expect(
        "INVARIANT VIOLATED: Minitest source range exceeds u32 UTF-16 units. This is a guest bug because LSP source positions use u32. Fix: reject documents beyond the protocol position domain before invoking extensions.",
    )
}

fn source(macro_name: &str) -> PatchSource {
    PatchSource {
        extension_id: EXTENSION_ID.to_string(),
        macro_name: macro_name.to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
ruby_fast_lsp_extension_guest_sdk::export_extension!(extension);

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_fast_lsp_extension_api::{Argument, CalleeResolution, Receiver, ResolvedCallee};

    fn document(uri: &str, text: &str) -> DocumentContext {
        DocumentContext {
            uri: uri.to_string(),
            text: text.to_string(),
            project: None,
        }
    }

    fn range(line: u32) -> SourceRange {
        SourceRange {
            start: SourcePosition { line, character: 0 },
            end: SourcePosition {
                line,
                character: 12,
            },
        }
    }

    fn describe_call(line: u32) -> ResolvedCall {
        ResolvedCall {
            method_name: "describe".to_string(),
            receiver: Receiver::None,
            arguments: Vec::new(),
            resolved_callees: vec![ResolvedCallee {
                owner: vec!["Object".to_string()],
                owner_kind: NamespaceKind::Instance,
                method: "describe".to_string(),
                resolution: CalleeResolution::Exact,
            }],
            call_range: range(line),
            message_range: range(line),
            frame_extension_ids: vec!["minitest-ruby".to_string()],
        }
    }

    fn call_context(method: &str, line: u32) -> CallContext {
        CallContext {
            project: None,
            method_name: method.to_string(),
            receiver: Receiver::None,
            arguments: Vec::new(),
            current_namespace: Vec::new(),
            namespace_kind: NamespaceKind::Instance,
            call_range: range(line),
            block_range: Some(range(line)),
            message_range: range(line),
            resolved_callees: Vec::new(),
            enclosing_calls: Vec::new(),
        }
    }

    #[test]
    fn exact_root_describe_creates_an_isolated_generated_spec_class() {
        let mut context = call_context("describe", 3);
        context.resolved_callees = describe_call(3).resolved_callees;
        let output = minitest_call_output(&context);
        assert_eq!(output.execution_contexts.len(), 1);
        let execution = &output.execution_contexts[0];
        assert_eq!(execution.generated_owners.len(), 1);
        assert_eq!(
            execution.generated_owners[0].scope,
            GeneratedOwnerScope::Source
        );
        assert_eq!(
            execution.generated_owners[0].parent,
            Some(minitest_spec_instance_target())
        );
        assert_eq!(
            execution.implicit_receiver,
            ExecutionContextTarget::GeneratedOwner {
                local_id: group_owner_local_id(range(3)),
                owner_kind: Some(NamespaceKind::Singleton),
            }
        );
        assert_eq!(
            execution.method_definition_owner,
            ExecutionContextTarget::GeneratedOwner {
                local_id: group_owner_local_id(range(3)),
                owner_kind: Some(NamespaceKind::Instance),
            }
        );
    }

    #[test]
    fn nested_let_defines_a_typed_helper_on_the_owning_group() {
        let mut context = call_context("let", 5);
        context.arguments.push(Argument {
            keyword: None,
            value: ArgumentValue::Symbol("service".to_string()),
            range: range(5),
        });
        context.enclosing_calls.push(describe_call(3));
        let output = minitest_call_output(&context);
        let IndexPatch::DefineMethod(method) = &output.index_patches[0] else {
            panic!("INVARIANT VIOLATED: Minitest let emitted a non-method patch. This is a guest test bug because let must use the public DefineMethod contract. Fix: keep helper generation on the ordinary method fact path.");
        };
        assert_eq!(method.name, "service");
        assert_eq!(
            method.return_type_source,
            Some(MethodReturnTypeSource::Block)
        );
        assert_eq!(
            method.owner_target,
            Some(ExecutionContextTarget::GeneratedOwner {
                local_id: group_owner_local_id(range(3)),
                owner_kind: Some(NamespaceKind::Instance),
            })
        );
        assert_eq!(output.execution_contexts.len(), 1);
    }

    #[test]
    fn unresolved_root_describe_fails_closed() {
        assert_eq!(
            minitest_call_output(&call_context("describe", 3)),
            ExtensionOutput::index_patches(Vec::new())
        );
    }

    #[test]
    fn explicit_rspec_describe_is_not_claimed_when_both_frameworks_are_locked() {
        let mut context = call_context("describe", 3);
        context.receiver = Receiver::Constant(vec!["RSpec".to_string()]);
        context.resolved_callees = describe_call(3).resolved_callees;

        assert_eq!(
            minitest_call_output(&context),
            ExtensionOutput::index_patches(Vec::new()),
            "an explicit RSpec receiver must win over globally visible Minitest describe candidates"
        );
    }

    #[test]
    fn discovers_classes_methods_and_declarative_tests() {
        let nodes = test_nodes(&document(
            "file:///repo/test/models/user_test.rb",
            "class UserTest < Minitest::Test\n  def test_valid\n  end\n\n  test \"rejects blanks\" do\n  end\nend\n",
        ));
        assert_eq!(
            nodes
                .iter()
                .map(|node| node.label.as_str())
                .collect::<Vec<_>>(),
            ["UserTest", "test_valid", "rejects blanks"]
        );
        assert_eq!(nodes[1].test_name.as_deref(), Some("test_valid"));
        assert_eq!(nodes[2].test_name.as_deref(), Some("test_: rejects blanks"));
    }

    #[test]
    fn discovers_spec_groups_and_examples_with_exact_filters() {
        let nodes = test_nodes(&document(
            "file:///repo/test/service_test.rb",
            "describe \"outer\" do\n  it(\"uses / service\") do\n  end\n  specify 'works' do\n  end\nend\n",
        ));
        assert_eq!(
            nodes
                .iter()
                .map(|node| (node.kind, node.label.as_str(), node.test_name.as_deref()))
                .collect::<Vec<_>>(),
            [
                (TestNodeKind::SpecGroup, "outer", None),
                (
                    TestNodeKind::Method,
                    "uses / service",
                    Some("/uses \\/ service/")
                ),
                (TestNodeKind::Method, "works", Some("/works/")),
            ]
        );
    }

    #[test]
    fn emits_run_and_debug_lenses_with_utf16_ranges() {
        let output = code_lens_output(&document(
            "file:///repo/test/emoji_test.rb",
            "  test \"😀 behavior\" do\n  end\n",
        ));
        assert_eq!(output.response_patches.len(), 2);
        let ResponsePatch::CodeLens(lens) = &output.response_patches[0] else {
            panic!("INVARIANT VIOLATED: Minitest code-lens output contained another patch kind. This is a guest test bug because code_lens_output emits only code lenses. Fix: keep response routing explicit.");
        };
        assert_eq!(lens.range, source_range(0, 2, 6));
        assert_eq!(
            lens.arguments,
            ["file:///repo/test/emoji_test.rb", "1", "test_: 😀 behavior"]
        );
    }

    #[test]
    fn ignores_test_shaped_code_outside_test_files() {
        assert!(test_nodes(&document(
            "file:///repo/lib/user.rb",
            "class UserTest\n  def test_valid\n  end\nend\n"
        ))
        .is_empty());
    }
}
