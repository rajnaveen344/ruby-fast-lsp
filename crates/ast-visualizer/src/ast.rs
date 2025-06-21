use ruby_prism::Node;
use serde::Serialize;

// AST node structure for JSON serialization
#[derive(Serialize, Clone)]
pub struct AstNode {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<AstNode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver: Option<Box<AstNode>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<AstNode>,
}

// AstVisitor implementation for building the AST tree
pub struct AstVisitor {
    // The current node being built
    current_node: Option<AstNode>,
    // Stack of parent nodes
    node_stack: Vec<AstNode>,
}

impl AstVisitor {
    pub fn new() -> Self {
        Self {
            current_node: None,
            node_stack: Vec::new(),
        }
    }

    // Helper to create a new node
    fn create_node(
        &mut self,
        node_type: String,
        name: Option<String>,
        value: Option<String>,
    ) -> AstNode {
        AstNode {
            node_type,
            name,
            value,
            children: Vec::new(),
            parameters: Vec::new(),
            receiver: None,
            arguments: Vec::new(),
        }
    }

    // Helper to push the current node to the stack and create a new one
    fn push_node(&mut self, node_type: String, name: Option<String>, value: Option<String>) {
        if let Some(current) = self.current_node.take() {
            self.node_stack.push(current);
        }
        self.current_node = Some(self.create_node(node_type, name, value));
    }

    // Helper to pop a node from the stack
    fn pop_node(&mut self) -> Option<AstNode> {
        let child = self.current_node.take();
        self.current_node = self.node_stack.pop();

        if let Some(child_node) = child {
            if let Some(ref mut parent) = self.current_node {
                parent.children.push(child_node.clone());
            }
            Some(child_node)
        } else {
            None
        }
    }

    // Helper to get a string from a name
    fn get_name_string(&self, name: &[u8]) -> String {
        String::from_utf8_lossy(name).to_string()
    }

    // Build the AST tree from a node
    pub fn build_ast(&mut self, node: &Node) -> AstNode {
        // Start with a root node
        self.push_node("RootNode".to_string(), None, None);

        // Visit the node
        self.visit_node(node);

        // Return the root node
        self.pop_node()
            .unwrap_or_else(|| self.create_node("EMPTY".to_string(), None, None))
    }

    // Visit a node and create the appropriate AST node
    fn visit_node(&mut self, node: &Node) {
        match node {
            Node::ProgramNode { .. } => {
                let program_node = node.as_program_node().unwrap();
                self.push_node("ProgramNode".to_string(), None, None);

                // Process statements
                for stmt in program_node.statements().body().iter() {
                    self.visit_node(&stmt);
                }

                self.pop_node();
            }
            Node::ClassNode { .. } => {
                let class_node = node.as_class_node().unwrap();
                let name = self.get_name_string(class_node.name().as_slice());
                self.push_node("ClassNode".to_string(), Some(name.clone()), None);

                // Process constant path
                self.visit_node(&class_node.constant_path());

                // Process class body
                if let Some(body) = class_node.body() {
                    if let Some(statements_node) = body.as_statements_node() {
                        for stmt in statements_node.body().iter() {
                            self.visit_node(&stmt);
                        }
                    } else {
                        self.visit_node(&body);
                    }
                }

                self.pop_node();
            }
            Node::ModuleNode { .. } => {
                let module_node = node.as_module_node().unwrap();
                let name = self.get_name_string(module_node.name().as_slice());
                self.push_node("ModuleNode".to_string(), Some(name.clone()), None);

                // Process constant path
                self.visit_node(&module_node.constant_path());

                // Process module body
                if let Some(body) = module_node.body() {
                    if let Some(statements_node) = body.as_statements_node() {
                        for stmt in statements_node.body().iter() {
                            self.visit_node(&stmt);
                        }
                    } else {
                        self.visit_node(&body);
                    }
                }

                self.pop_node();
            }
            Node::DefNode { .. } => {
                let def_node = node.as_def_node().unwrap();
                let name = self.get_name_string(def_node.name().as_slice());
                self.push_node("DefNode".to_string(), Some(name), None);

                // Add parameters if any
                if let Some(params) = def_node.parameters() {
                    let mut parameters = Vec::new();

                    // Required parameters
                    for param in params.requireds().iter() {
                        if let Some(param_node) = param.as_required_parameter_node() {
                            let param_name = self.get_name_string(param_node.name().as_slice());
                            parameters.push(param_name);
                        }
                    }

                    // Optional parameters
                    for param in params.optionals().iter() {
                        if let Some(param_node) = param.as_optional_parameter_node() {
                            let param_name = self.get_name_string(param_node.name().as_slice());
                            parameters.push(format!("{} = ...", param_name));
                        }
                    }

                    // Rest parameter
                    if let Some(rest) = params.rest() {
                        if let Some(rest_node) = rest.as_rest_parameter_node() {
                            if let Some(name) = rest_node.name() {
                                let param_name = self.get_name_string(name.as_slice());
                                parameters.push(format!("*{}", param_name));
                            } else {
                                parameters.push("*".to_string());
                            }
                        }
                    }

                    // Add parameters to the current node
                    if let Some(ref mut current) = self.current_node {
                        current.parameters = parameters;
                    }
                }

                // Process method body
                if let Some(body) = def_node.body() {
                    if let Some(statements_node) = body.as_statements_node() {
                        for stmt in statements_node.body().iter() {
                            self.visit_node(&stmt);
                        }
                    } else {
                        self.visit_node(&body);
                    }
                }

                self.pop_node();
            }
            Node::CallNode { .. } => {
                let call_node = node.as_call_node().unwrap();
                let name = self.get_name_string(call_node.name().as_slice());
                self.push_node("CallNode".to_string(), Some(name), None);

                // Process receiver
                if let Some(recv) = call_node.receiver() {
                    self.push_node("ReceiverNode".to_string(), None, None);
                    self.visit_node(&recv);
                    let receiver_node = self.pop_node().unwrap();

                    if let Some(ref mut current) = self.current_node {
                        current.receiver = Some(Box::new(receiver_node));
                    }
                }

                // Process arguments
                if let Some(args) = call_node.arguments() {
                    let mut arguments = Vec::new();
                    for arg in args.arguments().iter() {
                        self.push_node("ArgumentNode".to_string(), None, None);
                        self.visit_node(&arg);
                        let arg_node = self.pop_node().unwrap();
                        arguments.push(arg_node);
                    }

                    if let Some(ref mut current) = self.current_node {
                        current.arguments = arguments;
                    }
                }

                if let Some(block) = call_node.block() {
                    self.push_node("BlockNode".to_string(), None, None);
                    self.visit_node(&block);
                }

                self.pop_node();
            }
            Node::ConstantReadNode { .. } => {
                let constant_node = node.as_constant_read_node().unwrap();
                let name = self.get_name_string(constant_node.name().as_slice());
                self.push_node("ConstantReadNode".to_string(), Some(name), None);
                self.pop_node();
            }
            Node::ConstantPathNode { .. } => {
                let constant_path_node = node.as_constant_path_node().unwrap();
                let name = if let Some(name_id) = constant_path_node.name() {
                    self.get_name_string(name_id.as_slice())
                } else {
                    "<unknown>".to_string()
                };
                self.push_node("ConstantPathNode".to_string(), Some(name), None);

                // Add parent if any
                if let Some(parent) = constant_path_node.parent() {
                    self.visit_node(&parent);
                }

                self.pop_node();
            }
            Node::InstanceVariableReadNode { .. } => {
                let ivar_node = node.as_instance_variable_read_node().unwrap();
                let name = self.get_name_string(ivar_node.name().as_slice());
                self.push_node("InstanceVariableReadNode".to_string(), Some(name), None);
                self.pop_node();
            }
            Node::InstanceVariableWriteNode { .. } => {
                let ivasgn_node = node.as_instance_variable_write_node().unwrap();
                let name = self.get_name_string(ivasgn_node.name().as_slice());
                self.push_node("InstanceVariableWriteNode".to_string(), Some(name), None);

                // Process value
                let value_node = ivasgn_node.value();
                self.visit_node(&value_node);

                self.pop_node();
            }
            Node::LocalVariableReadNode { .. } => {
                let lvar_node = node.as_local_variable_read_node().unwrap();
                let name = self.get_name_string(lvar_node.name().as_slice());
                let depth = format!("depth: {}", lvar_node.depth());
                self.push_node("LocalVariableReadNode".to_string(), Some(name), Some(depth));
                self.pop_node();
            }
            Node::LocalVariableWriteNode { .. } => {
                let lvasgn_node = node.as_local_variable_write_node().unwrap();
                let name = self.get_name_string(lvasgn_node.name().as_slice());
                let depth = format!("depth: {}", lvasgn_node.depth());
                self.push_node(
                    "LocalVariableWriteNode".to_string(),
                    Some(name),
                    Some(depth),
                );

                // Process value
                let value_node = lvasgn_node.value();
                self.visit_node(&value_node);

                self.pop_node();
            }
            Node::StringNode { .. } => {
                let string_node = node.as_string_node().unwrap();
                let value = String::from_utf8_lossy(string_node.unescaped()).to_string();
                self.push_node("StringNode".to_string(), None, Some(value));
                self.pop_node();
            }
            Node::SymbolNode { .. } => {
                let symbol_node = node.as_symbol_node().unwrap();
                let value = String::from_utf8_lossy(symbol_node.unescaped()).to_string();
                self.push_node("SymbolNode".to_string(), None, Some(format!(":{}", value)));
                self.pop_node();
            }
            Node::IntegerNode { .. } => {
                let integer_node = node.as_integer_node().unwrap();
                let value = format!("{:?}", integer_node.value());
                self.push_node("IntegerNode".to_string(), None, Some(value));
                self.pop_node();
            }
            Node::StatementsNode { .. } => {
                let statements_node = node.as_statements_node().unwrap();
                self.push_node("StatementsNode".to_string(), None, None);

                // Process all statements
                for stmt in statements_node.body().iter() {
                    self.visit_node(&stmt);
                }

                self.pop_node();
            }
            Node::BlockNode { .. } => {
                let block_node = node.as_block_node().unwrap();
                self.push_node("BlockNode".to_string(), None, None);

                // Add parameters if any
                if let Some(_params) = block_node.parameters() {
                    self.push_node("BlockParametersNode".to_string(), None, None);
                    self.pop_node();
                }

                // Add body if any
                if let Some(body) = block_node.body() {
                    self.visit_node(&body);
                }

                self.pop_node();
            }
            Node::IfNode { .. } => {
                let if_node = node.as_if_node().unwrap();
                self.push_node("IfNode".to_string(), None, None);

                // Process predicate (condition)
                self.push_node("PredicateNode".to_string(), None, None);
                self.visit_node(&if_node.predicate());
                self.pop_node();

                // Process statements (then branch)
                if let Some(statements) = if_node.statements() {
                    self.push_node("StatementsNode".to_string(), None, None);

                    // Process all statements
                    for stmt in statements.body().iter() {
                        self.visit_node(&stmt);
                    }

                    self.pop_node();
                }

                // Process subsequent (else branch)
                if let Some(subsequent) = if_node.subsequent() {
                    self.push_node("SubsequentNode".to_string(), None, None);
                    self.visit_node(&subsequent);
                    self.pop_node();
                }

                self.pop_node();
            }
            Node::ForNode { .. } => {
                let for_node = node.as_for_node().unwrap();
                self.push_node("ForNode".to_string(), None, None);

                // Process index (iterator)
                self.push_node("IndexNode".to_string(), None, None);
                self.visit_node(&for_node.index());
                self.pop_node();

                // Process collection
                self.push_node("CollectionNode".to_string(), None, None);
                self.visit_node(&for_node.collection());
                self.pop_node();

                // Process statements (body)
                if let Some(statements) = for_node.statements() {
                    self.push_node("StatementsNode".to_string(), None, None);

                    // Process all statements
                    for stmt in statements.body().iter() {
                        self.visit_node(&stmt);
                    }

                    self.pop_node();
                }

                self.pop_node();
            }
            Node::OrNode { .. } => {
                let or_node = node.as_or_node().unwrap();
                self.push_node("OrNode".to_string(), None, None);

                // Process left operand
                self.push_node("LeftNode".to_string(), None, None);
                self.visit_node(&or_node.left());
                self.pop_node();

                // Process right operand
                self.push_node("RightNode".to_string(), None, None);
                self.visit_node(&or_node.right());
                self.pop_node();

                self.pop_node();
            }
            Node::AndNode { .. } => {
                let and_node = node.as_and_node().unwrap();
                self.push_node("AndNode".to_string(), None, None);

                // Process left operand
                self.push_node("LeftNode".to_string(), None, None);
                self.visit_node(&and_node.left());
                self.pop_node();

                // Process right operand
                self.push_node("RightNode".to_string(), None, None);
                self.visit_node(&and_node.right());
                self.pop_node();

                self.pop_node();
            }
            Node::SuperNode { .. } => {
                let super_node = node.as_super_node().unwrap();
                self.push_node("SuperNode".to_string(), None, None);

                // Add arguments if any
                if let Some(args) = super_node.arguments() {
                    for arg in args.arguments().iter() {
                        self.visit_node(&arg);
                    }
                }

                self.pop_node();
            }
            Node::ForwardingSuperNode { .. } => {
                self.push_node("ForwardingSuperNode".to_string(), None, None);
                self.pop_node();
            }
            Node::BlockArgumentNode { .. } => {
                let block_arg_node = node.as_block_argument_node().unwrap();
                self.push_node("BlockArgumentNode".to_string(), None, None);

                // Add expression if any
                if let Some(expr) = block_arg_node.expression() {
                    self.visit_node(&expr);
                }

                self.pop_node();
            }
            // Handle other node types as needed
            _ => {
                self.push_node(format!("{:?}", node), None, None);
                self.pop_node();
            }
        }
    }
}

// Convert Prism AST node to our serializable structure
pub fn convert_node_to_ast(node: &Node) -> AstNode {
    let mut visitor = AstVisitor::new();
    visitor.build_ast(node)
}
