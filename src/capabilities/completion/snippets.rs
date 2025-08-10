use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat};

/// Ruby code snippets for common control structures and patterns
pub struct RubySnippets;

impl RubySnippets {
    /// Get all available Ruby snippets
    pub fn get_all_snippets() -> Vec<CompletionItem> {
        vec![
            // Control flow snippets
            Self::if_snippet(),
            Self::if_else_snippet(),
            Self::unless_snippet(),
            Self::unless_else_snippet(),
            Self::case_when_snippet(),
            Self::while_snippet(),
            Self::until_snippet(),
            Self::for_snippet(),
            Self::loop_snippet(),
            Self::times_snippet(),
            Self::each_snippet(),
            Self::each_with_index_snippet(),
            Self::map_snippet(),
            Self::select_snippet(),
            Self::reject_snippet(),
            
            // Block snippets
            Self::do_block_snippet(),
            Self::brace_block_snippet(),
            
            // Method and class snippets
            Self::def_snippet(),
            Self::def_with_args_snippet(),
            Self::class_snippet(),
            Self::class_with_superclass_snippet(),
            Self::module_snippet(),
            Self::attr_reader_snippet(),
            Self::attr_writer_snippet(),
            Self::attr_accessor_snippet(),
            
            // Exception handling
            Self::begin_rescue_snippet(),
            Self::begin_rescue_ensure_snippet(),
            Self::rescue_snippet(),
            
            // Common patterns
            Self::initialize_snippet(),
            Self::to_s_snippet(),
            Self::hash_snippet(),
            Self::array_snippet(),
            Self::string_interpolation_snippet(),
            Self::lambda_snippet(),
            Self::proc_snippet(),
            
            // Testing snippets
            Self::describe_snippet(),
            Self::it_snippet(),
            Self::context_snippet(),
            Self::before_snippet(),
            Self::after_snippet(),
            
            // Rails-specific snippets (if applicable)
            Self::validates_snippet(),
            Self::belongs_to_snippet(),
            Self::has_many_snippet(),
            Self::has_one_snippet(),
            Self::scope_snippet(),
        ]
    }

    /// Get snippets that match a given prefix
    pub fn get_matching_snippets(prefix: &str) -> Vec<CompletionItem> {
        if prefix.is_empty() {
            return Self::get_all_snippets();
        }
        
        Self::get_all_snippets()
            .into_iter()
            .filter(|snippet| {
                snippet.label.to_lowercase().starts_with(&prefix.to_lowercase()) ||
                snippet.filter_text.as_ref().map_or(false, |filter| {
                    filter.to_lowercase().contains(&prefix.to_lowercase())
                })
            })
            .collect()
    }

    // Control flow snippets
    fn if_snippet() -> CompletionItem {
        CompletionItem {
            label: "if".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("if statement".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "if condition\n  # code\nend".to_string()
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
                "if condition\n  # code\nelse\n  # code\nend".to_string()
            )),
            insert_text: Some("if ${1:condition}\n  ${2:# code}\nelse\n  ${3:# code}\nend".to_string()),
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
                "unless condition\n  # code\nend".to_string()
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
                "unless condition\n  # code\nelse\n  # code\nend".to_string()
            )),
            insert_text: Some("unless ${1:condition}\n  ${2:# code}\nelse\n  ${3:# code}\nend".to_string()),
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
                "case variable\nwhen value1\n  # code\nwhen value2\n  # code\nelse\n  # code\nend".to_string()
            )),
            insert_text: Some("case ${1:variable}\nwhen ${2:value1}\n  ${3:# code}\nwhen ${4:value2}\n  ${5:# code}\nelse\n  ${6:# code}\nend".to_string()),
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
                "while condition\n  # code\nend".to_string()
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
                "until condition\n  # code\nend".to_string()
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
                "for item in collection\n  # code\nend".to_string()
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
                "loop do\n  # code\n  break if condition\nend".to_string()
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
                "n.times do |i|\n  # code\nend".to_string()
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
                "collection.each do |item|\n  # code\nend".to_string()
            )),
            insert_text: Some("${1:collection}.each do |${2:item}|\n  ${3:# code}\nend".to_string()),
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
                "collection.each_with_index do |item, index|\n  # code\nend".to_string()
            )),
            insert_text: Some("${1:collection}.each_with_index do |${2:item}, ${3:index}|\n  ${4:# code}\nend".to_string()),
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
                "collection.map do |item|\n  # transform item\nend".to_string()
            )),
            insert_text: Some("${1:collection}.map do |${2:item}|\n  ${3:# transform item}\nend".to_string()),
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
                "collection.select do |item|\n  # condition\nend".to_string()
            )),
            insert_text: Some("${1:collection}.select do |${2:item}|\n  ${3:# condition}\nend".to_string()),
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
                "collection.reject do |item|\n  # condition\nend".to_string()
            )),
            insert_text: Some("${1:collection}.reject do |${2:item}|\n  ${3:# condition}\nend".to_string()),
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
                "do |args|\n  # code\nend".to_string()
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
                "{ |args| code }".to_string()
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
                "def method_name\n  # code\nend".to_string()
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
                "def method_name(args)\n  # code\nend".to_string()
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
                "class ClassName\n  # code\nend".to_string()
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
                "class ClassName < SuperClass\n  # code\nend".to_string()
            )),
            insert_text: Some("class ${1:ClassName} < ${2:SuperClass}\n  ${3:# code}\nend".to_string()),
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
                "module ModuleName\n  # code\nend".to_string()
            )),
            insert_text: Some("module ${1:ModuleName}\n  ${2:# code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("module".to_string()),
            sort_text: Some("0022".to_string()),
            ..Default::default()
        }
    }

    fn attr_reader_snippet() -> CompletionItem {
        CompletionItem {
            label: "attr_reader".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("attr_reader declaration".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "attr_reader :attribute".to_string()
            )),
            insert_text: Some("attr_reader :${1:attribute}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("attr_reader".to_string()),
            sort_text: Some("0023".to_string()),
            ..Default::default()
        }
    }

    fn attr_writer_snippet() -> CompletionItem {
        CompletionItem {
            label: "attr_writer".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("attr_writer declaration".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "attr_writer :attribute".to_string()
            )),
            insert_text: Some("attr_writer :${1:attribute}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("attr_writer".to_string()),
            sort_text: Some("0024".to_string()),
            ..Default::default()
        }
    }

    fn attr_accessor_snippet() -> CompletionItem {
        CompletionItem {
            label: "attr_accessor".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("attr_accessor declaration".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "attr_accessor :attribute".to_string()
            )),
            insert_text: Some("attr_accessor :${1:attribute}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("attr_accessor".to_string()),
            sort_text: Some("0025".to_string()),
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
                "begin\n  # code\nrescue => e\n  # handle error\nend".to_string()
            )),
            insert_text: Some("begin\n  ${1:# code}\nrescue => ${2:e}\n  ${3:# handle error}\nend".to_string()),
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
                "rescue ExceptionType => e\n  # handle error".to_string()
            )),
            insert_text: Some("rescue ${1:StandardError} => ${2:e}\n  ${3:# handle error}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("rescue".to_string()),
            sort_text: Some("0028".to_string()),
            ..Default::default()
        }
    }

    // Common patterns
    fn initialize_snippet() -> CompletionItem {
        CompletionItem {
            label: "initialize".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("initialize method".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "def initialize(args)\n  # initialization code\nend".to_string()
            )),
            insert_text: Some("def initialize(${1:args})\n  ${2:# initialization code}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("initialize".to_string()),
            sort_text: Some("0029".to_string()),
            ..Default::default()
        }
    }

    fn to_s_snippet() -> CompletionItem {
        CompletionItem {
            label: "to_s".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("to_s method".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "def to_s\n  # string representation\nend".to_string()
            )),
            insert_text: Some("def to_s\n  ${1:# string representation}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("to_s".to_string()),
            sort_text: Some("0030".to_string()),
            ..Default::default()
        }
    }

    fn hash_snippet() -> CompletionItem {
        CompletionItem {
            label: "hash".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("hash literal".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "{ key: value }".to_string()
            )),
            insert_text: Some("{ ${1:key}: ${2:value} }".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("hash".to_string()),
            sort_text: Some("0031".to_string()),
            ..Default::default()
        }
    }

    fn array_snippet() -> CompletionItem {
        CompletionItem {
            label: "array".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("array literal".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "[item1, item2]".to_string()
            )),
            insert_text: Some("[${1:item1}, ${2:item2}]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("array".to_string()),
            sort_text: Some("0032".to_string()),
            ..Default::default()
        }
    }

    fn string_interpolation_snippet() -> CompletionItem {
        CompletionItem {
            label: "string interpolation".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("string interpolation".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "\"text #{variable} text\"".to_string()
            )),
            insert_text: Some("\"${1:text} #{${2:variable}} ${3:text}\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("interpolation".to_string()),
            sort_text: Some("0033".to_string()),
            ..Default::default()
        }
    }

    fn lambda_snippet() -> CompletionItem {
        CompletionItem {
            label: "lambda".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("lambda expression".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "lambda { |args| code }".to_string()
            )),
            insert_text: Some("lambda { |${1:args}| ${2:code} }".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("lambda".to_string()),
            sort_text: Some("0034".to_string()),
            ..Default::default()
        }
    }

    fn proc_snippet() -> CompletionItem {
        CompletionItem {
            label: "proc".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("proc expression".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "Proc.new { |args| code }".to_string()
            )),
            insert_text: Some("Proc.new { |${1:args}| ${2:code} }".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("proc".to_string()),
            sort_text: Some("0035".to_string()),
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
                "describe 'description' do\n  # tests\nend".to_string()
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
                "it 'description' do\n  # test\nend".to_string()
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
                "context 'when condition' do\n  # tests\nend".to_string()
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
                "before do\n  # setup\nend".to_string()
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
                "after do\n  # cleanup\nend".to_string()
            )),
            insert_text: Some("after do\n  ${1:# cleanup}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("after".to_string()),
            sort_text: Some("0040".to_string()),
            ..Default::default()
        }
    }

    // Rails-specific snippets
    fn validates_snippet() -> CompletionItem {
        CompletionItem {
            label: "validates".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("Rails validation".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "validates :attribute, presence: true".to_string()
            )),
            insert_text: Some("validates :${1:attribute}, ${2:presence: true}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("validates".to_string()),
            sort_text: Some("0041".to_string()),
            ..Default::default()
        }
    }

    fn belongs_to_snippet() -> CompletionItem {
        CompletionItem {
            label: "belongs_to".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("Rails belongs_to association".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "belongs_to :model".to_string()
            )),
            insert_text: Some("belongs_to :${1:model}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("belongs_to".to_string()),
            sort_text: Some("0042".to_string()),
            ..Default::default()
        }
    }

    fn has_many_snippet() -> CompletionItem {
        CompletionItem {
            label: "has_many".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("Rails has_many association".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "has_many :models".to_string()
            )),
            insert_text: Some("has_many :${1:models}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("has_many".to_string()),
            sort_text: Some("0043".to_string()),
            ..Default::default()
        }
    }

    fn has_one_snippet() -> CompletionItem {
        CompletionItem {
            label: "has_one".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("Rails has_one association".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "has_one :model".to_string()
            )),
            insert_text: Some("has_one :${1:model}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("has_one".to_string()),
            sort_text: Some("0044".to_string()),
            ..Default::default()
        }
    }

    fn scope_snippet() -> CompletionItem {
        CompletionItem {
            label: "scope".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("Rails scope".to_string()),
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                "scope :name, -> { where(condition) }".to_string()
            )),
            insert_text: Some("scope :${1:name}, -> { ${2:where(condition)} }".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            filter_text: Some("scope".to_string()),
            sort_text: Some("0045".to_string()),
            ..Default::default()
        }
    }
}