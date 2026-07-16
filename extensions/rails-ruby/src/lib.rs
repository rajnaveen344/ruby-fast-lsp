use ruby_fast_lsp_extension_api::{
    Argument, ArgumentValue, CallContext, CodeLensPatch, DefineMethodPatch, DocumentContext,
    ExtensionEvent, ExtensionOutput, IndexPatch, MethodParamKind, MethodParamPatch,
    MethodVisibility, NamespaceKind, PatchSource, Receiver, ReferencePatch, ReferenceTarget,
    ResolvedCall, ResponsePatch, RubyType, SourcePosition, SourceRange,
};
use ruby_fast_lsp_extension_guest_sdk::GuestExtension;

pub const EXTENSION_ID: &str = "rails-ruby";

const CALL_NAMES: &[&str] = &[
    "belongs_to",
    "has_one",
    "has_many",
    "before_validation",
    "after_validation",
    "before_save",
    "around_save",
    "after_save",
    "before_create",
    "around_create",
    "after_create",
    "before_update",
    "around_update",
    "after_update",
    "before_destroy",
    "around_destroy",
    "after_destroy",
    "before_commit",
    "after_commit",
    "after_rollback",
    "after_create_commit",
    "after_update_commit",
    "after_destroy_commit",
    "after_save_commit",
    "after_initialize",
    "after_find",
    "after_touch",
    "validate",
    "validates",
    "validates_associated",
    "validates_absence_of",
    "validates_acceptance_of",
    "validates_confirmation_of",
    "validates_exclusion_of",
    "validates_format_of",
    "validates_inclusion_of",
    "validates_length_of",
    "validates_numericality_of",
    "validates_presence_of",
    "validates_size_of",
    "validates_uniqueness_of",
    "validates_comparison_of",
    "resources",
    "resource",
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "match",
    "root",
    "perform_later",
    "perform_now",
];

const CALLBACKS: &[&str] = &[
    "before_validation",
    "after_validation",
    "before_save",
    "around_save",
    "after_save",
    "before_create",
    "around_create",
    "after_create",
    "before_update",
    "around_update",
    "after_update",
    "before_destroy",
    "around_destroy",
    "after_destroy",
    "before_commit",
    "after_commit",
    "after_rollback",
    "after_create_commit",
    "after_update_commit",
    "after_destroy_commit",
    "after_save_commit",
    "after_initialize",
    "after_find",
    "after_touch",
];

const VALIDATIONS: &[&str] = &[
    "validate",
    "validates",
    "validates_associated",
    "validates_absence_of",
    "validates_acceptance_of",
    "validates_confirmation_of",
    "validates_exclusion_of",
    "validates_format_of",
    "validates_inclusion_of",
    "validates_length_of",
    "validates_numericality_of",
    "validates_presence_of",
    "validates_size_of",
    "validates_uniqueness_of",
    "validates_comparison_of",
];

pub fn extension() -> RailsExtension {
    RailsExtension
}

pub struct RailsExtension;

impl GuestExtension for RailsExtension {
    fn indexed_call_names(&self) -> &'static [&'static str] {
        CALL_NAMES
    }

    fn index_call(&mut self, context: &CallContext) -> ExtensionOutput {
        ExtensionOutput::index_patches(rails_index_call(context))
    }

    fn handle_event(&mut self, event: &ExtensionEvent) -> ExtensionOutput {
        if event.event == "index.call.enter" {
            let context = event.call.as_ref().expect(
                "INVARIANT VIOLATED: Rails index.call.enter omitted CallContext. This is a host/guest ABI bug because call events require their typed payload. Fix: encode CallContext for every index.call.enter event.",
            );
            return self.index_call(context);
        }
        if event.event == "request.code_lens" {
            return event
                .document
                .as_ref()
                .map(code_lens_output)
                .unwrap_or_else(|| ExtensionOutput::index_patches(Vec::new()));
        }
        ExtensionOutput::index_patches(Vec::new())
    }
}

fn rails_index_call(context: &CallContext) -> Vec<IndexPatch> {
    match context.method_name.as_str() {
        "perform_later" | "perform_now" => active_job_patches(context),
        "belongs_to" | "has_one" | "has_many" => association_patches(context),
        method if CALLBACKS.contains(&method) || VALIDATIONS.contains(&method) => {
            callback_patches(context)
        }
        "resources" | "resource" => resource_patches(context),
        "get" | "post" | "put" | "patch" | "delete" | "match" | "root" => route_patches(context),
        _ => Vec::new(),
    }
}

fn association_patches(context: &CallContext) -> Vec<IndexPatch> {
    let Some(argument) = context.arguments.first() else {
        return Vec::new();
    };
    let Some(name) = argument_string(argument) else {
        return Vec::new();
    };
    if context.current_namespace.is_empty() || !method_name(name) {
        return Vec::new();
    }
    let collection = context.method_name == "has_many";
    let class_name = keyword_argument(context, "class_name");
    let polymorphic = keyword_argument(context, "polymorphic");
    let target = if polymorphic
        .is_some_and(|argument| matches!(argument.value, ArgumentValue::Boolean(true)))
    {
        None
    } else if let Some(class_name) = class_name {
        explicit_target(class_name)
    } else {
        Some(target_namespace(name, collection))
    };
    let reader_type = if collection {
        RubyType::Array(vec![target
            .as_ref()
            .map(|parts| RubyType::Named(parts.join("::")))
            .unwrap_or(RubyType::Unknown)])
    } else {
        target
            .as_ref()
            .map(|parts| {
                RubyType::Union(vec![
                    RubyType::Named(parts.join("::")),
                    RubyType::Named("NilClass".to_string()),
                ])
            })
            .unwrap_or(RubyType::Unknown)
    };
    let mut patches = Vec::new();
    if let Some(target) = &target {
        patches.push(reference_patch(
            ReferenceTarget::Namespace(target.clone()),
            class_name.map_or(argument.range, |argument| argument.range),
            context.method_name.as_str(),
        ));
    }
    patches.push(method_patch(
        name,
        context.current_namespace.clone(),
        argument.range,
        Vec::new(),
        Some(reader_type.clone()),
        context.method_name.as_str(),
    ));
    patches.push(method_patch(
        &format!("{name}="),
        context.current_namespace.clone(),
        argument.range,
        vec![MethodParamPatch {
            name: "value".to_string(),
            kind: MethodParamKind::Required,
        }],
        Some(reader_type),
        context.method_name.as_str(),
    ));
    patches
}

fn callback_patches(context: &CallContext) -> Vec<IndexPatch> {
    if context.current_namespace.is_empty() {
        return Vec::new();
    }
    context
        .arguments
        .iter()
        .filter(|argument| argument.keyword.is_none())
        .filter_map(|argument| {
            let name = argument_string(argument)?;
            method_name(name).then(|| {
                reference_patch(
                    ReferenceTarget::Method {
                        namespace: context.current_namespace.clone(),
                        owner_kind: NamespaceKind::Instance,
                        name: name.to_string(),
                    },
                    argument.range,
                    context.method_name.as_str(),
                )
            })
        })
        .collect()
}

fn active_job_patches(context: &CallContext) -> Vec<IndexPatch> {
    let Receiver::Constant(namespace) = &context.receiver else {
        return Vec::new();
    };
    if namespace.is_empty()
        || namespace.iter().any(|part| !constant_name(part))
        || !namespace.last().is_some_and(|name| name.ends_with("Job"))
    {
        return Vec::new();
    }
    vec![reference_patch(
        ReferenceTarget::Method {
            namespace: namespace.clone(),
            owner_kind: NamespaceKind::Instance,
            name: "perform".to_string(),
        },
        context.message_range,
        context.method_name.as_str(),
    )]
}

fn resource_patches(context: &CallContext) -> Vec<IndexPatch> {
    let Some((controller_scope, helper_scope)) = route_scope(context) else {
        return Vec::new();
    };
    let Some(argument) = context
        .arguments
        .iter()
        .find(|argument| argument.keyword.is_none())
    else {
        return Vec::new();
    };
    let Some(name) = argument_string(argument) else {
        return Vec::new();
    };
    if !method_name(name) {
        return Vec::new();
    }
    let controller_argument = keyword_argument(context, "controller");
    let controller_name = controller_argument
        .and_then(argument_string)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            if context.method_name == "resources" {
                name.to_string()
            } else {
                format!("{name}s")
            }
        });
    let mut patches = Vec::new();
    if let Some(mut controller) = controller_namespace(&controller_name) {
        let mut scoped = controller_scope
            .iter()
            .map(|part| camelize(part))
            .collect::<Vec<_>>();
        scoped.append(&mut controller);
        patches.push(reference_patch(
            ReferenceTarget::Namespace(scoped),
            controller_argument.map_or(argument.range, |argument| argument.range),
            context.method_name.as_str(),
        ));
    }
    if keyword_argument(context, "only").is_some() || keyword_argument(context, "except").is_some()
    {
        return patches;
    }
    let explicit_name = keyword_argument(context, "as");
    let Some(helper_base) = explicit_name
        .and_then(argument_string)
        .or(Some(name))
        .filter(|name| method_name(name))
    else {
        return patches;
    };
    let helper_location = explicit_name.map_or(argument.range, |argument| argument.range);
    let helper_names = if context.method_name == "resources" {
        let collection = prefix_helper(&helper_scope, helper_base);
        let member = prefix_helper(&helper_scope, &singularize(helper_base));
        vec![
            collection,
            format!("new_{member}"),
            format!("edit_{member}"),
            member,
        ]
    } else {
        let base = prefix_helper(&helper_scope, helper_base);
        vec![format!("new_{base}"), format!("edit_{base}"), base]
    };
    append_route_helpers(
        &mut patches,
        helper_names,
        helper_location,
        context.method_name.as_str(),
    );
    patches
}

fn route_patches(context: &CallContext) -> Vec<IndexPatch> {
    let Some((controller_scope, helper_scope)) = route_scope(context) else {
        return Vec::new();
    };
    let target_argument = keyword_argument(context, "to").or_else(|| {
        context.arguments.iter().find(|argument| {
            argument.keyword.is_none()
                && argument_string(argument).is_some_and(|value| value.contains('#'))
        })
    });
    let mut patches = Vec::new();
    if let Some((mut controller, action, controller_range, action_range)) =
        target_argument.and_then(route_target)
    {
        let mut scoped = controller_scope
            .iter()
            .map(|part| camelize(part))
            .collect::<Vec<_>>();
        scoped.append(&mut controller);
        patches.push(reference_patch(
            ReferenceTarget::Namespace(scoped.clone()),
            controller_range,
            context.method_name.as_str(),
        ));
        patches.push(reference_patch(
            ReferenceTarget::Method {
                namespace: scoped,
                owner_kind: NamespaceKind::Instance,
                name: action,
            },
            action_range,
            context.method_name.as_str(),
        ));
    }
    let explicit_name = keyword_argument(context, "as");
    let helper_base = explicit_name
        .and_then(argument_string)
        .map(ToString::to_string)
        .or_else(|| (context.method_name == "root").then(|| "root".to_string()));
    let Some(helper_base) = helper_base.filter(|name| method_name(name)) else {
        return patches;
    };
    let helper = prefix_helper(&helper_scope, &helper_base);
    append_route_helpers(
        &mut patches,
        vec![helper],
        explicit_name.map_or(context.message_range, |argument| argument.range),
        context.method_name.as_str(),
    );
    patches
}

fn append_route_helpers(
    patches: &mut Vec<IndexPatch>,
    helpers: Vec<String>,
    location: SourceRange,
    macro_name: &str,
) {
    for helper in helpers {
        for suffix in ["path", "url"] {
            patches.push(method_patch(
                &format!("{helper}_{suffix}"),
                vec!["ApplicationController".to_string()],
                location,
                vec![
                    MethodParamPatch {
                        name: "args".to_string(),
                        kind: MethodParamKind::Rest,
                    },
                    MethodParamPatch {
                        name: "kwargs".to_string(),
                        kind: MethodParamKind::KeywordRest,
                    },
                ],
                Some(RubyType::Named("String".to_string())),
                macro_name,
            ));
        }
    }
}

fn route_scope(context: &CallContext) -> Option<(Vec<String>, Vec<String>)> {
    let draw = context.enclosing_calls.iter().any(|call| {
        call.method_name == "draw"
            && matches!(
                &call.receiver,
                Receiver::MethodCall { method_name } if method_name == "routes"
            )
    });
    if !draw {
        return None;
    }
    let mut controller_scope = Vec::new();
    let mut helper_scope = Vec::new();
    for call in &context.enclosing_calls {
        match call.method_name.as_str() {
            "namespace" => {
                let name = call
                    .arguments
                    .iter()
                    .find(|argument| argument.keyword.is_none())
                    .and_then(argument_string)?;
                if !method_name(name) {
                    return None;
                }
                controller_scope.push(name.to_string());
                helper_scope.push(name.to_string());
            }
            "scope" => {
                if let Some(argument) = frame_keyword_argument(call, "module") {
                    let modules = argument_string(argument)?.split('/').collect::<Vec<_>>();
                    if modules.is_empty() || modules.iter().any(|name| !method_name(name)) {
                        return None;
                    }
                    controller_scope.extend(modules.into_iter().map(ToString::to_string));
                }
                if let Some(argument) = frame_keyword_argument(call, "as") {
                    let name = argument_string(argument)?;
                    if !method_name(name) {
                        return None;
                    }
                    helper_scope.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    Some((controller_scope, helper_scope))
}

fn route_target(argument: &Argument) -> Option<(Vec<String>, String, SourceRange, SourceRange)> {
    let value = argument_string(argument)?;
    let separator = value.find('#')?;
    if separator == 0 || separator + 1 >= value.len() {
        return None;
    }
    let controller = &value[..separator];
    let action = &value[separator + 1..];
    let namespace = controller_namespace(controller)?;
    if !method_name(action) {
        return None;
    }
    Some((
        namespace,
        action.to_string(),
        subrange(argument.range, 0, separator)?,
        subrange(argument.range, separator + 1, action.len())?,
    ))
}

fn code_lens_output(document: &DocumentContext) -> ExtensionOutput {
    let response_patches = controller_actions(document)
        .into_iter()
        .map(|action| {
            ResponsePatch::CodeLens(CodeLensPatch {
                title: "Open View".to_string(),
                command: "ruby-fast-lsp.rails.openView".to_string(),
                range: action.range,
                arguments: vec![document.uri.clone(), action.controller, action.action],
                source: source("controller_action"),
            })
        })
        .collect();
    ExtensionOutput {
        index_patches: Vec::new(),
        execution_contexts: Vec::new(),
        response_patches,
        command_patches: Vec::new(),
        process_requests: Vec::new(),
        reindex_files: Vec::new(),
    }
}

struct ControllerAction {
    controller: String,
    action: String,
    range: SourceRange,
}

fn controller_actions(document: &DocumentContext) -> Vec<ControllerAction> {
    let Some(controller) = controller_path(&document.uri) else {
        return Vec::new();
    };
    let mut actions = Vec::new();
    let mut public = true;
    for (line_index, raw_line) in document.text.split('\n').enumerate() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let stripped = line.trim_start_matches(char::is_whitespace);
        match stripped {
            "public" => {
                public = true;
                continue;
            }
            "private" | "protected" => {
                public = false;
                continue;
            }
            _ => {}
        }
        let Some(rest) = public.then(|| stripped.strip_prefix("def ")).flatten() else {
            continue;
        };
        let Some(action) = token_until(rest, &[' ', '(', '=', ';']) else {
            continue;
        };
        if !method_name(action) {
            continue;
        }
        let line_number = u32::try_from(line_index).expect(
            "INVARIANT VIOLATED: Rails controller line exceeds u32. This is a guest bug because LSP positions use u32. Fix: reject documents outside the protocol position domain.",
        );
        let indent = utf16_len(&line[..line.len() - stripped.len()]);
        actions.push(ControllerAction {
            controller: controller.clone(),
            action: action.to_string(),
            range: source_range(line_number, indent + 4, indent + 4 + utf16_len(action)),
        });
    }
    actions
}

fn controller_path(uri: &str) -> Option<String> {
    let marker = "/app/controllers/";
    let relative = uri.split_once(marker)?.1;
    let controller = relative.strip_suffix("_controller.rb")?;
    (!controller.is_empty() && controller.split('/').all(method_name))
        .then(|| controller.to_string())
}

fn method_patch(
    name: &str,
    namespace: Vec<String>,
    location: SourceRange,
    params: Vec<MethodParamPatch>,
    return_type: Option<RubyType>,
    macro_name: &str,
) -> IndexPatch {
    IndexPatch::DefineMethod(DefineMethodPatch {
        name: name.to_string(),
        namespace,
        owner_target: None,
        owner_kind: NamespaceKind::Instance,
        visibility: MethodVisibility::Public,
        location,
        params,
        return_type,
        return_type_source: None,
        source: source(macro_name),
    })
}

fn reference_patch(target: ReferenceTarget, location: SourceRange, macro_name: &str) -> IndexPatch {
    IndexPatch::AddReference(ReferencePatch {
        target,
        location,
        source: source(macro_name),
    })
}

fn keyword_argument<'a>(context: &'a CallContext, name: &str) -> Option<&'a Argument> {
    context.arguments.iter().find(|argument| {
        argument
            .keyword
            .as_ref()
            .is_some_and(|keyword| keyword.name == name)
    })
}

fn frame_keyword_argument<'a>(call: &'a ResolvedCall, name: &str) -> Option<&'a Argument> {
    call.arguments.iter().find(|argument| {
        argument
            .keyword
            .as_ref()
            .is_some_and(|keyword| keyword.name == name)
    })
}

fn argument_string(argument: &Argument) -> Option<&str> {
    match &argument.value {
        ArgumentValue::Symbol(value) | ArgumentValue::String(value) => Some(value),
        ArgumentValue::Constant(_)
        | ArgumentValue::Boolean(_)
        | ArgumentValue::Nil
        | ArgumentValue::Unsupported => None,
    }
}

fn explicit_target(argument: &Argument) -> Option<Vec<String>> {
    let parts = match &argument.value {
        ArgumentValue::Constant(parts) => parts.clone(),
        ArgumentValue::Symbol(value) | ArgumentValue::String(value) => value
            .trim_start_matches("::")
            .split("::")
            .map(ToString::to_string)
            .collect(),
        ArgumentValue::Boolean(_) | ArgumentValue::Nil | ArgumentValue::Unsupported => return None,
    };
    (!parts.is_empty() && parts.iter().all(|part| constant_name(part))).then_some(parts)
}

fn target_namespace(name: &str, collection: bool) -> Vec<String> {
    let name = if collection {
        singularize(name)
    } else {
        name.to_string()
    };
    vec![camelize(&name)]
}

fn singularize(name: &str) -> String {
    match name {
        "people" => "person".to_string(),
        "children" => "child".to_string(),
        "men" => "man".to_string(),
        "women" => "woman".to_string(),
        "mice" => "mouse".to_string(),
        "geese" => "goose".to_string(),
        "teeth" => "tooth".to_string(),
        "feet" => "foot".to_string(),
        "oxen" => "ox".to_string(),
        _ if name.ends_with("ies") && name.len() > 3 => format!("{}y", &name[..name.len() - 3]),
        _ if name.ends_with("ses") && name.len() > 3 => name[..name.len() - 2].to_string(),
        _ if name.ends_with('s') && !name.ends_with("ss") => name[..name.len() - 1].to_string(),
        _ => name.to_string(),
    }
}

fn camelize(name: &str) -> String {
    let mut result = String::new();
    let mut uppercase = true;
    for character in name.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            result.extend(character.to_uppercase());
            uppercase = false;
        } else {
            result.push(character);
        }
    }
    result
}

fn controller_namespace(name: &str) -> Option<Vec<String>> {
    let mut parts = name.split('/').collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| !method_name(part)) {
        return None;
    }
    let controller = parts.pop()?;
    let mut namespace = parts.into_iter().map(camelize).collect::<Vec<_>>();
    namespace.push(format!("{}Controller", camelize(controller)));
    Some(namespace)
}

fn method_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_lowercase())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn constant_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn prefix_helper(namespaces: &[String], helper: &str) -> String {
    if namespaces.is_empty() {
        helper.to_string()
    } else {
        format!("{}_{helper}", namespaces.join("_"))
    }
}

fn subrange(range: SourceRange, start_offset: usize, length: usize) -> Option<SourceRange> {
    if range.start.line != range.end.line {
        return None;
    }
    let start_offset = u32::try_from(start_offset).ok()?;
    let length = u32::try_from(length).ok()?;
    let start = range.start.character.checked_add(start_offset).expect(
        "INVARIANT VIOLATED: Rails subrange start overflowed u32. This is a guest bug because the host supplied a valid LSP range and the offset came from its bounded literal. Fix: keep literal offsets within the source range.",
    );
    let end = start.checked_add(length).expect(
        "INVARIANT VIOLATED: Rails subrange end overflowed u32. This is a guest bug because the host supplied a valid LSP range and the length came from its bounded literal. Fix: keep literal lengths within the source range.",
    );
    Some(source_range(range.start.line, start, end))
}

fn token_until<'a>(text: &'a str, delimiters: &[char]) -> Option<&'a str> {
    let end = text
        .char_indices()
        .find_map(|(index, character)| delimiters.contains(&character).then_some(index))
        .unwrap_or(text.len());
    (end > 0).then_some(&text[..end])
}

fn utf16_len(value: &str) -> u32 {
    u32::try_from(value.encode_utf16().count()).expect(
        "INVARIANT VIOLATED: Rails source range exceeds u32 UTF-16 units. This is a guest bug because LSP positions use u32. Fix: reject documents outside the protocol position domain.",
    )
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
    use ruby_fast_lsp_extension_api::{Keyword, ResolvedCallee};

    fn range(line: u32, start: u32, end: u32) -> SourceRange {
        source_range(line, start, end)
    }

    fn argument(value: ArgumentValue, location: SourceRange) -> Argument {
        Argument {
            keyword: None,
            value,
            range: location,
        }
    }

    fn keyword(name: &str, value: ArgumentValue, location: SourceRange) -> Argument {
        Argument {
            keyword: Some(Keyword {
                name: name.to_string(),
                range: location,
            }),
            value,
            range: location,
        }
    }

    fn context(method: &str, value: &str) -> CallContext {
        let location = range(1, 13, 20);
        CallContext {
            project: None,
            method_name: method.to_string(),
            receiver: Receiver::None,
            arguments: vec![argument(ArgumentValue::Symbol(value.to_string()), location)],
            current_namespace: vec!["User".to_string()],
            namespace_kind: NamespaceKind::Instance,
            call_range: location,
            block_range: None,
            message_range: location,
            resolved_callees: Vec::<ResolvedCallee>::new(),
            enclosing_calls: Vec::new(),
        }
    }

    fn routes_draw_frame() -> ResolvedCall {
        ResolvedCall {
            method_name: "draw".to_string(),
            receiver: Receiver::MethodCall {
                method_name: "routes".to_string(),
            },
            arguments: Vec::new(),
            resolved_callees: Vec::new(),
            call_range: range(0, 0, 29),
            message_range: range(0, 25, 29),
            frame_extension_ids: vec!["rails-ruby".to_string()],
        }
    }

    fn method_from(patch: &IndexPatch) -> &DefineMethodPatch {
        let IndexPatch::DefineMethod(method) = patch else {
            panic!("INVARIANT VIOLATED: Rails unit test expected DefineMethod. This is a test bug because the selected patch position is behaviorally fixed. Fix: update the assertion when the public patch contract intentionally changes.");
        };
        method
    }

    fn reference_from(patch: &IndexPatch) -> &ReferencePatch {
        let IndexPatch::AddReference(reference) = patch else {
            panic!("INVARIANT VIOLATED: Rails unit test expected AddReference. This is a test bug because the selected patch position is behaviorally fixed. Fix: update the assertion when the public patch contract intentionally changes.");
        };
        reference
    }

    #[test]
    fn associations_preserve_reference_reader_writer_and_structured_types() {
        let patches = rails_index_call(&context("belongs_to", "account"));
        assert_eq!(patches.len(), 3);
        assert_eq!(
            reference_from(&patches[0]).target,
            ReferenceTarget::Namespace(vec!["Account".to_string()])
        );
        assert_eq!(method_from(&patches[1]).name, "account");
        assert_eq!(
            method_from(&patches[1]).return_type,
            Some(RubyType::Union(vec![
                RubyType::Named("Account".to_string()),
                RubyType::Named("NilClass".to_string()),
            ]))
        );
        assert_eq!(method_from(&patches[2]).name, "account=");
        assert_eq!(
            method_from(&patches[2]).params[0].kind,
            MethodParamKind::Required
        );

        let many = rails_index_call(&context("has_many", "companies"));
        assert_eq!(
            method_from(&many[1]).return_type,
            Some(RubyType::Array(vec![RubyType::Named(
                "Company".to_string()
            )]))
        );
    }

    #[test]
    fn association_options_are_precise_and_polymorphic_targets_fail_closed() {
        let mut explicit = context("belongs_to", "account");
        explicit.arguments.push(keyword(
            "class_name",
            ArgumentValue::String("Billing::Account".to_string()),
            range(1, 35, 51),
        ));
        let patches = rails_index_call(&explicit);
        assert_eq!(
            reference_from(&patches[0]).target,
            ReferenceTarget::Namespace(vec!["Billing".to_string(), "Account".to_string()])
        );
        assert_eq!(reference_from(&patches[0]).location, range(1, 35, 51));

        let mut polymorphic = context("belongs_to", "subject");
        polymorphic.arguments.push(keyword(
            "polymorphic",
            ArgumentValue::Boolean(true),
            range(1, 35, 39),
        ));
        let patches = rails_index_call(&polymorphic);
        assert_eq!(patches.len(), 2);
        assert_eq!(
            method_from(&patches[0]).return_type,
            Some(RubyType::Unknown)
        );
    }

    #[test]
    fn callbacks_and_validations_emit_exact_instance_method_references() {
        for macro_name in ["before_save", "validate", "validates_presence_of"] {
            let patches = rails_index_call(&context(macro_name, "normalize_account"));
            assert_eq!(patches.len(), 1);
            assert_eq!(
                reference_from(&patches[0]).target,
                ReferenceTarget::Method {
                    namespace: vec!["User".to_string()],
                    owner_kind: NamespaceKind::Instance,
                    name: "normalize_account".to_string(),
                }
            );
        }
    }

    #[test]
    fn resource_routes_generate_controller_references_and_typed_helpers() {
        let mut resources = context("resources", "people");
        resources.enclosing_calls.push(routes_draw_frame());
        resources.enclosing_calls.push(ResolvedCall {
            method_name: "namespace".to_string(),
            receiver: Receiver::None,
            arguments: vec![argument(
                ArgumentValue::Symbol("admin".to_string()),
                range(1, 12, 18),
            )],
            resolved_callees: Vec::new(),
            call_range: range(1, 2, 18),
            message_range: range(1, 2, 11),
            frame_extension_ids: vec!["rails-ruby".to_string()],
        });
        let patches = rails_index_call(&resources);
        assert_eq!(
            reference_from(&patches[0]).target,
            ReferenceTarget::Namespace(vec!["Admin".to_string(), "PeopleController".to_string(),])
        );
        let names = patches[1..]
            .iter()
            .map(|patch| method_from(patch).name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"admin_people_path"));
        assert!(names.contains(&"new_admin_person_path"));
        assert!(names.contains(&"admin_person_url"));
        assert!(method_from(&patches[1])
            .return_type
            .as_ref()
            .is_some_and(|ruby_type| ruby_type == &RubyType::Named("String".to_string())));
    }

    #[test]
    fn named_route_splits_controller_action_ranges_and_defines_helpers() {
        let mut route = context("get", "/account");
        route.arguments.push(keyword(
            "to",
            ArgumentValue::String("users#show".to_string()),
            range(2, 25, 35),
        ));
        route.arguments.push(keyword(
            "as",
            ArgumentValue::Symbol("account".to_string()),
            range(2, 41, 48),
        ));
        route.enclosing_calls.push(routes_draw_frame());
        let patches = rails_index_call(&route);
        assert_eq!(
            reference_from(&patches[0]).target,
            ReferenceTarget::Namespace(vec!["UsersController".to_string()])
        );
        assert_eq!(reference_from(&patches[0]).location, range(2, 25, 30));
        assert_eq!(
            reference_from(&patches[1]).target,
            ReferenceTarget::Method {
                namespace: vec!["UsersController".to_string()],
                owner_kind: NamespaceKind::Instance,
                name: "show".to_string(),
            }
        );
        assert_eq!(reference_from(&patches[1]).location, range(2, 31, 35));
        assert_eq!(method_from(&patches[2]).name, "account_path");
        assert_eq!(method_from(&patches[3]).name, "account_url");
    }

    #[test]
    fn routes_outside_routes_draw_and_dynamic_jobs_are_ignored() {
        assert!(rails_index_call(&context("resources", "users")).is_empty());
        let mut job = context("perform_later", "user");
        job.receiver = Receiver::LocalVariable("job_class".to_string());
        assert!(rails_index_call(&job).is_empty());
    }

    #[test]
    fn active_job_entry_points_reference_instance_perform() {
        for entry_point in ["perform_later", "perform_now"] {
            let mut job = context(entry_point, "user");
            job.receiver = Receiver::Constant(vec!["Billing".to_string(), "EmailJob".to_string()]);
            let patches = rails_index_call(&job);
            assert_eq!(
                reference_from(&patches[0]).target,
                ReferenceTarget::Method {
                    namespace: vec!["Billing".to_string(), "EmailJob".to_string()],
                    owner_kind: NamespaceKind::Instance,
                    name: "perform".to_string(),
                }
            );
        }
    }

    #[test]
    fn controller_lenses_include_only_public_actions() {
        let document = DocumentContext {
            uri: "file:///repo/app/controllers/admin/users_controller.rb".to_string(),
            text: "class Admin::UsersController\n  def show\n  end\n  private\n  def secret\n  end\nend\n".to_string(),
            project: None,
        };
        let output = code_lens_output(&document);
        assert_eq!(output.response_patches.len(), 1);
        let ResponsePatch::CodeLens(lens) = &output.response_patches[0] else {
            panic!("INVARIANT VIOLATED: Rails controller action emitted another response kind. This is a guest test bug because controller discovery owns only code lenses. Fix: keep response routing explicit.");
        };
        assert_eq!(lens.arguments[1], "admin/users");
        assert_eq!(lens.arguments[2], "show");
        assert_eq!(lens.range, range(1, 6, 10));
    }
}
