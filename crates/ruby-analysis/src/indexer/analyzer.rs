use crate::core::{ExecutionContextFact, RubyConstant, SourceFileId, SourcePosition};
use crate::{
    analyzer_utils as utils, is_erb_path, mask_erb, Identifier, IdentifierType, IdentifierVisitor,
    LVScopeId,
};
use ruby_prism::{visit_call_node, CallNode, Visit};
use url::Url;

/// Main analyzer for Ruby code using Prism
pub struct RubyPrismAnalyzer {
    pub uri: Url,
    pub code: String,
    analysis_code: String,
    execution_context: Option<ExecutionContextFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelpTarget {
    pub namespace: Vec<RubyConstant>,
    pub namespace_kind: crate::core::NamespaceKind,
    pub receiver: crate::MethodReceiver,
    pub receiver_range: Option<(u32, u32)>,
    pub method: crate::core::RubyMethod,
    pub active_parameter: u32,
    pub active_keyword: Option<String>,
}

impl RubyPrismAnalyzer {
    pub fn new(uri: Url, code: String) -> Self {
        let analysis_code = if is_erb_path(uri.path()) {
            mask_erb(&code).source().to_string()
        } else {
            code.clone()
        };
        Self {
            uri,
            code,
            analysis_code,
            execution_context: None,
        }
    }

    pub fn with_execution_context(mut self, context: ExecutionContextFact) -> Self {
        self.execution_context = Some(context);
        self
    }

    /// Returns the identifier, identifier type, and the ancestors stack at the time of the lookup.
    pub fn get_identifier(
        &self,
        byte_offset: u32,
    ) -> (
        Option<Identifier>,
        Option<IdentifierType>,
        Vec<RubyConstant>,
        LVScopeId,
        crate::core::NamespaceKind,
    ) {
        let parse_result = ruby_prism::parse(self.analysis_code.as_bytes());
        let root_node = parse_result.node();

        let mut iden_visitor = IdentifierVisitor::new_with_execution_context_at_offset(
            self.code.clone(),
            byte_offset,
            self.execution_context.clone(),
        );
        iden_visitor.visit(&root_node);

        iden_visitor.get_result()
    }

    pub fn get_identifier_at_position(
        &self,
        position: SourcePosition,
    ) -> (
        Option<Identifier>,
        Option<IdentifierType>,
        Vec<RubyConstant>,
        LVScopeId,
        crate::core::NamespaceKind,
    ) {
        let source = crate::SourceDocument::new(&self.code, SourceFileId(0));
        let byte_offset = u32::try_from(source.line_character_to_offset(
            &self.code,
            position.line,
            position.character,
        ))
        .expect(
            "INVARIANT VIOLATED: analyzer source position exceeded u32 byte offsets. This is a bug because TextRange stores u32 offsets. Fix: widen domain offsets before accepting larger source files.",
        );
        self.get_identifier(byte_offset)
    }

    pub fn get_signature_help_target(&self, byte_offset: u32) -> Option<SignatureHelpTarget> {
        let parse_result = ruby_prism::parse(self.analysis_code.as_bytes());
        let root_node = parse_result.node();
        let mut finder = SignatureCallSiteFinder::new(byte_offset as usize, &self.analysis_code);
        finder.visit(&root_node);
        let call_site = finder.best?;

        let message_offset = u32::try_from(call_site.message_start.saturating_add(1)).expect(
            "INVARIANT VIOLATED: signature-help message offset exceeded u32. This is a bug because analysis TextRange offsets are u32. Fix: widen domain offsets before accepting larger source files.",
        );
        let (identifier, _, _, _, namespace_kind) = self.get_identifier(message_offset);
        let crate::Identifier::RubyMethod {
            namespace,
            receiver,
            iden: method,
        } = identifier?
        else {
            return None;
        };

        Some(SignatureHelpTarget {
            namespace,
            namespace_kind,
            receiver,
            receiver_range: call_site.receiver_range,
            method,
            active_parameter: call_site.active_parameter,
            active_keyword: call_site.active_keyword,
        })
    }

    /// Get the namespace context (enclosing module/class) at a byte offset.
    pub fn get_namespace_at_offset(&self, byte_offset: u32) -> Vec<RubyConstant> {
        let parse_result = ruby_prism::parse(self.analysis_code.as_bytes());
        let root_node = parse_result.node();

        let mut namespace_stack = Vec::new();
        self.collect_namespaces_containing_offset(&root_node, byte_offset, &mut namespace_stack);
        namespace_stack
    }

    /// Recursively collect namespace (module/class) names that contain the given position.
    fn collect_namespaces_containing_offset(
        &self,
        node: &ruby_prism::Node,
        byte_offset: u32,
        namespace_stack: &mut Vec<RubyConstant>,
    ) {
        let target_offset = usize::try_from(byte_offset).expect(
            "INVARIANT VIOLATED: u32 analysis offset could not fit usize. This is a bug because supported targets must address u32 source offsets. Fix: reject the unsupported target architecture.",
        );
        let position_in_node = |node_loc: &ruby_prism::Location| -> bool {
            let start_offset = node_loc.start_offset();
            let end_offset = node_loc.end_offset();
            target_offset >= start_offset && target_offset < end_offset
        };

        if let Some(class_node) = node.as_class_node() {
            if position_in_node(&class_node.location()) {
                let constant_path = class_node.constant_path();
                push_constant_path_parts(&constant_path, namespace_stack);

                if let Some(body) = class_node.body() {
                    self.collect_namespaces_containing_offset(&body, byte_offset, namespace_stack);
                }
                return;
            }
        }

        if let Some(module_node) = node.as_module_node() {
            if position_in_node(&module_node.location()) {
                let constant_path = module_node.constant_path();
                push_constant_path_parts(&constant_path, namespace_stack);

                if let Some(body) = module_node.body() {
                    self.collect_namespaces_containing_offset(&body, byte_offset, namespace_stack);
                }
                return;
            }
        }

        if let Some(program) = node.as_program_node() {
            for stmt in program.statements().body().iter() {
                self.collect_namespaces_containing_offset(&stmt, byte_offset, namespace_stack);
            }
        } else if let Some(stmts) = node.as_statements_node() {
            for stmt in stmts.body().iter() {
                self.collect_namespaces_containing_offset(&stmt, byte_offset, namespace_stack);
            }
        } else if let Some(begin_node) = node.as_begin_node() {
            if let Some(stmts) = begin_node.statements() {
                for stmt in stmts.body().iter() {
                    self.collect_namespaces_containing_offset(&stmt, byte_offset, namespace_stack);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SignatureCallSite {
    message_start: usize,
    receiver_range: Option<(u32, u32)>,
    active_parameter: u32,
    active_keyword: Option<String>,
    span_len: usize,
}

struct SignatureCallSiteFinder<'a> {
    byte_offset: usize,
    source: &'a str,
    best: Option<SignatureCallSite>,
}

impl<'a> SignatureCallSiteFinder<'a> {
    fn new(byte_offset: usize, source: &'a str) -> Self {
        Self {
            byte_offset,
            source,
            best: None,
        }
    }

    fn consider(&mut self, node: &CallNode<'_>) {
        let Some(message) = node.message_loc() else {
            return;
        };
        let call = node.location();
        if self.byte_offset < message.end_offset() || self.byte_offset > call.end_offset() {
            return;
        }
        if node
            .block()
            .is_some_and(|block| self.byte_offset >= block.location().start_offset())
        {
            return;
        }

        let span_len = call.end_offset() - call.start_offset();
        if self
            .best
            .as_ref()
            .is_some_and(|current| current.span_len <= span_len)
        {
            return;
        }

        self.best = Some(SignatureCallSite {
            message_start: message.start_offset(),
            receiver_range: node.receiver().map(|receiver| {
                let location = receiver.location();
                (
                    u32::try_from(location.start_offset()).expect(
                        "INVARIANT VIOLATED: signature-help receiver start exceeded u32 byte offsets. This is a bug because analysis TextRange offsets are u32. Fix: widen domain offsets before accepting larger source files.",
                    ),
                    u32::try_from(location.end_offset()).expect(
                        "INVARIANT VIOLATED: signature-help receiver end exceeded u32 byte offsets. This is a bug because analysis TextRange offsets are u32. Fix: widen domain offsets before accepting larger source files.",
                    ),
                )
            }),
            active_parameter: active_parameter_for_call(node, self.byte_offset, self.source),
            active_keyword: active_keyword_for_call(node, self.byte_offset),
            span_len,
        });
    }
}

fn active_keyword_for_call(node: &CallNode<'_>, byte_offset: usize) -> Option<String> {
    let arguments = node.arguments()?;
    for argument in arguments.arguments().iter() {
        let Some(keyword_hash) = argument.as_keyword_hash_node() else {
            continue;
        };
        for element in keyword_hash.elements().iter() {
            let Some(assoc) = element.as_assoc_node() else {
                continue;
            };
            let location = assoc.location();
            if byte_offset < location.start_offset() || byte_offset > location.end_offset() {
                continue;
            }
            let symbol = assoc.key().as_symbol_node()?;
            return Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
        }
    }
    None
}

impl<'pr> Visit<'pr> for SignatureCallSiteFinder<'_> {
    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        self.consider(node);
        visit_call_node(self, node);
    }
}

fn active_parameter_for_call(node: &CallNode<'_>, byte_offset: usize, source: &str) -> u32 {
    let Some(arguments) = node.arguments() else {
        return 0;
    };
    let args = arguments.arguments().iter().collect::<Vec<_>>();
    if args.is_empty() {
        return 0;
    }

    for (index, argument) in args.iter().enumerate() {
        let location = argument.location();
        if byte_offset <= location.end_offset() {
            return index as u32;
        }
    }

    let last = args.last().expect(
        "INVARIANT VIOLATED: argument list became empty after a non-empty check. \
         This is a bug because the local argument vector is immutable. \
         Fix: keep argument collection and active-parameter calculation together.",
    );
    let tail_start = last.location().end_offset().min(source.len());
    let tail_end = byte_offset.min(source.len());
    if tail_start <= tail_end && source[tail_start..tail_end].contains(',') {
        args.len() as u32
    } else {
        (args.len() - 1) as u32
    }
}

fn push_constant_path_parts(node: &ruby_prism::Node<'_>, namespace_stack: &mut Vec<RubyConstant>) {
    if let Some(cpn) = node.as_constant_path_node() {
        let mut names = Vec::new();
        utils::collect_namespaces(&cpn, &mut names);
        namespace_stack.extend(names);
    } else if let Some(crn) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(crn.name().as_slice());
        if let Ok(constant) = RubyConstant::new(&name) {
            namespace_stack.push(constant);
        }
    }
}
