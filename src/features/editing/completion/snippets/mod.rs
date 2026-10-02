use ruby_analysis::core::MethodReceiver;
use ruby_analysis::indexer::Identifier;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat};

/// Context for snippet completion to determine appropriate snippets
#[derive(Debug, Clone, PartialEq)]
pub enum SnippetContext {
    /// General context - show all applicable snippets
    General,
    /// Method call context - we're completing a method on an object (e.g., "a.each")
    MethodCall,
}

/// Ruby code snippets for common control structures and patterns
pub struct RubySnippets;

impl RubySnippets {
    /// Get all available Ruby snippets organized by category
    pub fn get_all_snippets() -> Vec<CompletionItem> {
        let mut snippets = Vec::new();

        // Control flow (keyword) snippets
        snippets.extend(Self::get_control_flow_snippets());

        // Iterator/method snippets
        snippets.extend(Self::get_iterator_snippets());

        // Block snippets
        snippets.extend(Self::get_block_snippets());

        // Definition snippets
        snippets.extend(Self::get_definition_snippets());

        // Exception handling snippets
        snippets.extend(Self::get_exception_snippets());

        // Testing snippets
        snippets.extend(Self::get_testing_snippets());

        snippets
    }

    /// Get control flow (keyword) snippets
    fn get_control_flow_snippets() -> Vec<CompletionItem> {
        vec![
            Self::if_snippet(),
            Self::if_else_snippet(),
            Self::unless_snippet(),
            Self::unless_else_snippet(),
            Self::case_when_snippet(),
            Self::while_snippet(),
            Self::until_snippet(),
            Self::for_snippet(),
            Self::loop_snippet(),
        ]
    }

    /// Get iterator/method snippets
    fn get_iterator_snippets() -> Vec<CompletionItem> {
        vec![
            Self::times_snippet(),
            Self::each_snippet(),
            Self::each_with_index_snippet(),
            Self::map_snippet(),
            Self::select_snippet(),
            Self::reject_snippet(),
        ]
    }

    /// Get block snippets
    fn get_block_snippets() -> Vec<CompletionItem> {
        vec![Self::do_block_snippet(), Self::brace_block_snippet()]
    }

    /// Get definition snippets
    fn get_definition_snippets() -> Vec<CompletionItem> {
        vec![
            Self::def_snippet(),
            Self::def_with_args_snippet(),
            Self::class_snippet(),
            Self::class_with_superclass_snippet(),
            Self::module_snippet(),
        ]
    }

    /// Get exception handling snippets
    fn get_exception_snippets() -> Vec<CompletionItem> {
        vec![
            Self::begin_rescue_snippet(),
            Self::begin_rescue_ensure_snippet(),
            Self::rescue_snippet(),
        ]
    }

    /// Get testing snippets
    fn get_testing_snippets() -> Vec<CompletionItem> {
        vec![
            Self::describe_snippet(),
            Self::it_snippet(),
            Self::context_snippet(),
            Self::before_snippet(),
            Self::after_snippet(),
        ]
    }

    /// Get snippets that match a given prefix with context awareness
    pub fn get_matching_snippets_with_context(
        prefix: &str,
        context: SnippetContext,
    ) -> Vec<CompletionItem> {
        let all_snippets = Self::get_all_snippets();

        let matching_snippets: Vec<CompletionItem> = all_snippets
            .into_iter()
            .filter(|snippet| {
                // First filter by prefix
                let prefix_matches = if prefix.is_empty() {
                    true // Include all snippets when prefix is empty
                } else {
                    snippet
                        .label
                        .to_lowercase()
                        .starts_with(&prefix.to_lowercase())
                        || snippet.filter_text.as_ref().is_some_and(|filter| {
                            filter.to_lowercase().contains(&prefix.to_lowercase())
                        })
                };

                // Then filter by context appropriateness
                if !prefix_matches {
                    return false;
                }

                match context {
                    SnippetContext::MethodCall => {
                        // In method call context, exclude keyword/control flow snippets
                        !Self::is_keyword_snippet(&snippet.label)
                    }
                    SnippetContext::General => {
                        // In general context, include all snippets
                        true
                    }
                }
            })
            .map(|mut snippet| {
                // Modify iterator snippets based on context
                match context {
                    SnippetContext::MethodCall => {
                        // For method call context, remove the collection placeholder from iterator snippets
                        if Self::is_iterator_snippet(&snippet.label) {
                            snippet = Self::make_contextual_iterator_snippet(snippet);
                        }
                    }
                    SnippetContext::General => {
                        // Keep snippets as-is for general context
                    }
                }
                snippet
            })
            .collect();

        matching_snippets
    }

    /// Enhanced context determination that considers position and line content
    pub fn determine_context_with_position(
        identifier: &Option<Identifier>,
        line_text: &str,
        position_char: u32,
    ) -> SnippetContext {
        match identifier {
            Some(Identifier::RubyMethod {
                receiver:
                    MethodReceiver::LocalVariable(_)
                    | MethodReceiver::InstanceVariable(_)
                    | MethodReceiver::ClassVariable(_)
                    | MethodReceiver::GlobalVariable(_)
                    | MethodReceiver::MethodCall { .. }
                    | MethodReceiver::Expression,
                ..
            }) => SnippetContext::MethodCall,
            None => {
                // Check if we're right after a dot, which indicates method call context
                let char_pos = position_char as usize;
                if char_pos > 0 && char_pos <= line_text.len() {
                    let before_cursor = &line_text[..char_pos];
                    if before_cursor.ends_with('.') {
                        SnippetContext::MethodCall
                    } else {
                        SnippetContext::General
                    }
                } else {
                    SnippetContext::General
                }
            }
            _ => SnippetContext::General,
        }
    }

    /// Check if a snippet is an iterator snippet that should be contextual
    fn is_iterator_snippet(label: &str) -> bool {
        matches!(
            label,
            "each" | "each_with_index" | "map" | "select" | "reject"
        )
    }

    /// Check if a snippet is a keyword/control flow snippet that should be excluded in MethodCall context
    fn is_keyword_snippet(label: &str) -> bool {
        matches!(
            label,
            // Control flow keywords
            "if" | "if else" | "unless" | "unless else" | "case when" |
            "while" | "until" | "for" | "loop" |
            // Definition keywords
            "def" | "def with args" | "class" | "class with superclass" | "module" |
            // Exception handling keywords
            "begin rescue" | "begin rescue ensure" | "rescue" |
            // Testing keywords (these are also not methods)
            "describe" | "it" | "context" | "before" | "after"
        )
    }

    /// Convert a full template iterator snippet to a contextual method snippet
    fn make_contextual_iterator_snippet(mut snippet: CompletionItem) -> CompletionItem {
        match snippet.label.as_str() {
            "each" => {
                snippet.insert_text = Some("each do |${1:item}|\n  ${2:# code}\nend".to_string());
                snippet.detail = Some("each iterator (method)".to_string());
                snippet.documentation = Some(tower_lsp::lsp_types::Documentation::String(
                    "each do |item|\n  # code\nend".to_string(),
                ));
            }
            "each_with_index" => {
                snippet.insert_text = Some(
                    "each_with_index do |${1:item}, ${2:index}|\n  ${3:# code}\nend".to_string(),
                );
                snippet.detail = Some("each_with_index iterator (method)".to_string());
                snippet.documentation = Some(tower_lsp::lsp_types::Documentation::String(
                    "each_with_index do |item, index|\n  # code\nend".to_string(),
                ));
            }
            "map" => {
                snippet.insert_text =
                    Some("map do |${1:item}|\n  ${2:# transform}\nend".to_string());
                snippet.detail = Some("map iterator (method)".to_string());
                snippet.documentation = Some(tower_lsp::lsp_types::Documentation::String(
                    "map do |item|\n  # transform\nend".to_string(),
                ));
            }
            "select" => {
                snippet.insert_text =
                    Some("select do |${1:item}|\n  ${2:# condition}\nend".to_string());
                snippet.detail = Some("select iterator (method)".to_string());
                snippet.documentation = Some(tower_lsp::lsp_types::Documentation::String(
                    "select do |item|\n  # condition\nend".to_string(),
                ));
            }
            "reject" => {
                snippet.insert_text =
                    Some("reject do |${1:item}|\n  ${2:# condition}\nend".to_string());
                snippet.detail = Some("reject iterator (method)".to_string());
                snippet.documentation = Some(tower_lsp::lsp_types::Documentation::String(
                    "reject do |item|\n  # condition\nend".to_string(),
                ));
            }
            _ => {}
        }
        snippet
    }

    // Control flow snippets
    fn if_snippet() -> CompletionItem {
        CompletionItem {
            label: "if".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("if statement".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "if condition\n  # code\nend".to_string(),
            )),
            insert_text: Some("if ${1:condition}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("if".to_string()),
            sort_text: Some("0001".to_string()),
            ..Default::default()
        }
    }

    fn if_else_snippet() -> CompletionItem {
        CompletionItem {
            label: "if else".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("if-else statement".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "if condition\n  # code\nelse\n  # code\nend".to_string(),
            )),
            insert_text: Some(
                "if ${1:condition}\n  ${2:# code}\nelse\n  ${3:# code}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("ifelse".to_string()),
            sort_text: Some("0002".to_string()),
            ..Default::default()
        }
    }

    fn unless_snippet() -> CompletionItem {
        CompletionItem {
            label: "unless".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("unless statement".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "unless condition\n  # code\nend".to_string(),
            )),
            insert_text: Some("unless ${1:condition}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("unless".to_string()),
            sort_text: Some("0003".to_string()),
            ..Default::default()
        }
    }

    fn unless_else_snippet() -> CompletionItem {
        CompletionItem {
            label: "unless else".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("unless-else statement".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "unless condition\n  # code\nelse\n  # code\nend".to_string(),
            )),
            insert_text: Some(
                "unless ${1:condition}\n  ${2:# code}\nelse\n  ${3:# code}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("unlesselse".to_string()),
            sort_text: Some("0004".to_string()),
            ..Default::default()
        }
    }

    fn case_when_snippet() -> CompletionItem {
        CompletionItem {
            label: "case when".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("case-when statement".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "case variable\nwhen value\n  # code\nend".to_string(),
            )),
            insert_text: Some(
                "case ${1:variable}\nwhen ${2:value}\n  ${3:# code}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("case when".to_string()),
            sort_text: Some("0005".to_string()),
            ..Default::default()
        }
    }

    fn while_snippet() -> CompletionItem {
        CompletionItem {
            label: "while".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("while loop".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "while condition\n  # code\nend".to_string(),
            )),
            insert_text: Some("while ${1:condition}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("while".to_string()),
            sort_text: Some("0006".to_string()),
            ..Default::default()
        }
    }

    fn until_snippet() -> CompletionItem {
        CompletionItem {
            label: "until".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("until loop".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "until condition\n  # code\nend".to_string(),
            )),
            insert_text: Some("until ${1:condition}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("until".to_string()),
            sort_text: Some("0007".to_string()),
            ..Default::default()
        }
    }

    fn for_snippet() -> CompletionItem {
        CompletionItem {
            label: "for".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("for loop".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "for item in collection\n  # code\nend".to_string(),
            )),
            insert_text: Some("for ${1:item} in ${2:collection}\n  ${3:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("for".to_string()),
            sort_text: Some("0008".to_string()),
            ..Default::default()
        }
    }

    fn loop_snippet() -> CompletionItem {
        CompletionItem {
            label: "loop".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("infinite loop".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "loop do\n  # code\n  break if condition\nend".to_string(),
            )),
            insert_text: Some("loop do\n  ${1:# code}\n  break if ${2:condition}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("loop".to_string()),
            sort_text: Some("0009".to_string()),
            ..Default::default()
        }
    }

    fn times_snippet() -> CompletionItem {
        CompletionItem {
            label: "times".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("times loop".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "n.times do |i|\n  # code\nend".to_string(),
            )),
            insert_text: Some("${1:n}.times do |${2:i}|\n  ${3:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("times".to_string()),
            sort_text: Some("0010".to_string()),
            ..Default::default()
        }
    }

    fn each_snippet() -> CompletionItem {
        CompletionItem {
            label: "each".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("each iterator".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "collection.each do |item|\n  # code\nend".to_string(),
            )),
            insert_text: Some(
                "${1:collection}.each do |${2:item}|\n  ${3:# code}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("each".to_string()),
            sort_text: Some("0011".to_string()),
            ..Default::default()
        }
    }

    fn each_with_index_snippet() -> CompletionItem {
        CompletionItem {
            label: "each_with_index".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("each_with_index iterator".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "collection.each_with_index do |item, index|\n  # code\nend".to_string(),
            )),
            insert_text: Some(
                "${1:collection}.each_with_index do |${2:item}, ${3:index}|\n  ${4:# code}\nend"
                    .to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("each_with_index".to_string()),
            sort_text: Some("0012".to_string()),
            ..Default::default()
        }
    }

    fn map_snippet() -> CompletionItem {
        CompletionItem {
            label: "map".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("map iterator".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "collection.map do |item|\n  # transform item\nend".to_string(),
            )),
            insert_text: Some(
                "${1:collection}.map do |${2:item}|\n  ${3:# transform item}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("map".to_string()),
            sort_text: Some("0013".to_string()),
            ..Default::default()
        }
    }

    fn select_snippet() -> CompletionItem {
        CompletionItem {
            label: "select".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("select iterator".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "collection.select do |item|\n  # condition\nend".to_string(),
            )),
            insert_text: Some(
                "${1:collection}.select do |${2:item}|\n  ${3:# condition}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("select".to_string()),
            sort_text: Some("0014".to_string()),
            ..Default::default()
        }
    }

    fn reject_snippet() -> CompletionItem {
        CompletionItem {
            label: "reject".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("reject iterator".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "collection.reject do |item|\n  # condition\nend".to_string(),
            )),
            insert_text: Some(
                "${1:collection}.reject do |${2:item}|\n  ${3:# condition}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("reject".to_string()),
            sort_text: Some("0015".to_string()),
            ..Default::default()
        }
    }

    // Block snippets
    fn do_block_snippet() -> CompletionItem {
        CompletionItem {
            label: "do".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("do block".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "do |args|\n  # code\nend".to_string(),
            )),
            insert_text: Some("do |${1:args}|\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("do".to_string()),
            sort_text: Some("0016".to_string()),
            ..Default::default()
        }
    }

    fn brace_block_snippet() -> CompletionItem {
        CompletionItem {
            label: "{ }".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("brace block".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "{ |args| code }".to_string(),
            )),
            insert_text: Some("{ |${1:args}| ${2:code} }".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("brace block".to_string()),
            sort_text: Some("0017".to_string()),
            ..Default::default()
        }
    }

    // Method and class snippets
    fn def_snippet() -> CompletionItem {
        CompletionItem {
            label: "def".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("method definition".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "def method_name\n  # code\nend".to_string(),
            )),
            insert_text: Some("def ${1:method_name}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("def".to_string()),
            sort_text: Some("0018".to_string()),
            ..Default::default()
        }
    }

    fn def_with_args_snippet() -> CompletionItem {
        CompletionItem {
            label: "def with args".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("method definition with arguments".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "def method_name(args)\n  # code\nend".to_string(),
            )),
            insert_text: Some("def ${1:method_name}(${2:args})\n  ${3:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("def args".to_string()),
            sort_text: Some("0019".to_string()),
            ..Default::default()
        }
    }

    fn class_snippet() -> CompletionItem {
        CompletionItem {
            label: "class".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("class definition".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "class ClassName\n  # code\nend".to_string(),
            )),
            insert_text: Some("class ${1:ClassName}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("class".to_string()),
            sort_text: Some("0020".to_string()),
            ..Default::default()
        }
    }

    fn class_with_superclass_snippet() -> CompletionItem {
        CompletionItem {
            label: "class with superclass".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("class definition with superclass".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "class ClassName < SuperClass\n  # code\nend".to_string(),
            )),
            insert_text: Some(
                "class ${1:ClassName} < ${2:SuperClass}\n  ${3:# code}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("class superclass".to_string()),
            sort_text: Some("0021".to_string()),
            ..Default::default()
        }
    }

    fn module_snippet() -> CompletionItem {
        CompletionItem {
            label: "module".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("module definition".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "module ModuleName\n  # code\nend".to_string(),
            )),
            insert_text: Some("module ${1:ModuleName}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("module".to_string()),
            sort_text: Some("0022".to_string()),
            ..Default::default()
        }
    }

    // Exception handling
    fn begin_rescue_snippet() -> CompletionItem {
        CompletionItem {
            label: "begin rescue".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("begin-rescue block".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "begin\n  # code\nrescue => e\n  # handle error\nend".to_string(),
            )),
            insert_text: Some(
                "begin\n  ${1:# code}\nrescue => ${2:e}\n  ${3:# handle error}\nend".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("begin rescue".to_string()),
            sort_text: Some("0026".to_string()),
            ..Default::default()
        }
    }

    fn begin_rescue_ensure_snippet() -> CompletionItem {
        CompletionItem {
            label: "begin rescue ensure".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("begin-rescue-ensure block".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "begin\n  # code\nrescue => e\n  # handle error\nensure\n  # cleanup\nend".to_string()
            )),
            insert_text: Some("begin\n  ${1:# code}\nrescue => ${2:e}\n  ${3:# handle error}\nensure\n  ${4:# cleanup}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("begin rescue ensure".to_string()),
            sort_text: Some("0027".to_string()),
            ..Default::default()
        }
    }

    fn rescue_snippet() -> CompletionItem {
        CompletionItem {
            label: "rescue".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("rescue clause".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "rescue ExceptionType => e\n  # handle error".to_string(),
            )),
            insert_text: Some(
                "rescue ${1:StandardError} => ${2:e}\n  ${3:# handle error}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("rescue".to_string()),
            sort_text: Some("0028".to_string()),
            ..Default::default()
        }
    }

    // Testing snippets
    fn describe_snippet() -> CompletionItem {
        CompletionItem {
            label: "describe".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("RSpec describe block".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "describe 'description' do\n  # tests\nend".to_string(),
            )),
            insert_text: Some("describe '${1:description}' do\n  ${2:# tests}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("describe".to_string()),
            sort_text: Some("0036".to_string()),
            ..Default::default()
        }
    }

    fn it_snippet() -> CompletionItem {
        CompletionItem {
            label: "it".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("RSpec it block".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "it 'description' do\n  # test\nend".to_string(),
            )),
            insert_text: Some("it '${1:description}' do\n  ${2:# test}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("it".to_string()),
            sort_text: Some("0037".to_string()),
            ..Default::default()
        }
    }

    fn context_snippet() -> CompletionItem {
        CompletionItem {
            label: "context".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("RSpec context block".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "context 'when condition' do\n  # tests\nend".to_string(),
            )),
            insert_text: Some("context '${1:when condition}' do\n  ${2:# tests}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("context".to_string()),
            sort_text: Some("0038".to_string()),
            ..Default::default()
        }
    }

    fn before_snippet() -> CompletionItem {
        CompletionItem {
            label: "before".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("RSpec before hook".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "before do\n  # setup\nend".to_string(),
            )),
            insert_text: Some("before do\n  ${1:# setup}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("before".to_string()),
            sort_text: Some("0039".to_string()),
            ..Default::default()
        }
    }

    fn after_snippet() -> CompletionItem {
        CompletionItem {
            label: "after".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("RSpec after hook".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "after do\n  # cleanup\nend".to_string(),
            )),
            insert_text: Some("after do\n  ${1:# cleanup}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("after".to_string()),
            sort_text: Some("0040".to_string()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests;
