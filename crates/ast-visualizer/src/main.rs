use actix_cors::Cors;
use actix_web::http::header::ContentType;
use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result};
use ruby_prism::{parse, Node};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;

// Request structure for parsing Ruby code
#[derive(Deserialize, Serialize)]
struct ParseRequest {
    code: String,
}

// AST node structure for JSON serialization
#[derive(Serialize, Clone)]
struct AstNode {
    #[serde(rename = "type")]
    node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<AstNode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parameters: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receiver: Option<Box<AstNode>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    arguments: Vec<AstNode>,
}

// State to track request count
struct AppState {
    request_count: Mutex<usize>,
}

// Parse Ruby code and return AST as JSON
async fn parse_ruby(data: web::Json<ParseRequest>, state: web::Data<AppState>) -> impl Responder {
    // Increment request count - handle poisoned mutex gracefully
    let count_result = state.request_count.lock();
    let request_num = match count_result {
        Ok(mut count) => {
            *count += 1;
            *count
        }
        Err(poisoned) => {
            // If the mutex is poisoned, recover the lock and continue
            let mut count = poisoned.into_inner();
            *count += 1;
            *count
        }
    };
    println!("Request #{}: Parsing Ruby code", request_num);

    // Parse Ruby code
    let code = data.code.clone();

    // Check input size - limit to 500KB to prevent memory issues
    const MAX_INPUT_SIZE: usize = 500 * 1024; // 500KB
    if code.len() > MAX_INPUT_SIZE {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Input too large",
            "message": format!("Input size ({} bytes) exceeds maximum allowed size ({} bytes)", code.len(), MAX_INPUT_SIZE)
        }));
    }

    // Validate input content
    // Check for non-UTF8 characters or other problematic content
    if code.contains("\0") {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid input",
            "message": "Input contains null bytes which are not allowed"
        }));
    }

    // Check for extremely long lines that might cause issues
    const MAX_LINE_LENGTH: usize = 10000; // 10K characters per line max
    if code.lines().any(|line| line.len() > MAX_LINE_LENGTH) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid input",
            "message": format!("Input contains lines exceeding the maximum allowed length of {} characters", MAX_LINE_LENGTH)
        }));
    }

    // Safely parse the code with error handling
    let parse_result = match std::panic::catch_unwind(|| {
        // Use a more defensive approach to parsing
        parse(code.as_bytes())
    }) {
        Ok(result) => result,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Parsing error",
                "message": "Failed to parse the input. The code may be too complex or contain unsupported constructs."
            }));
        }
    };

    // Check for syntax errors
    let errors_count = parse_result.errors().count();

    // If there are too many errors, reject the input
    const MAX_ERRORS: usize = 50;
    if errors_count > MAX_ERRORS {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Too many syntax errors",
            "message": format!("The input contains {} syntax errors, which exceeds the maximum allowed ({})", errors_count, MAX_ERRORS)
        }));
    }

    // If there are some errors, return them
    if errors_count > 0 {
        let errors: Vec<String> = parse_result.errors().map(|e| format!("{:?}", e)).collect();

        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Syntax error",
            "details": errors
        }));
    }

    // Convert AST to JSON-serializable structure with error handling
    let ast_result = std::panic::catch_unwind(|| convert_node_to_ast(&parse_result.node()));

    match ast_result {
        Ok(ast) => HttpResponse::Ok().json(ast),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "AST processing error",
            "message": "Failed to process the AST. The input may be too complex or contain unsupported constructs."
        }))
    }
}

// AstVisitor implementation for building the AST tree
struct AstVisitor {
    // The current node being built
    current_node: Option<AstNode>,
    // Stack of parent nodes
    node_stack: Vec<AstNode>,
}

impl AstVisitor {
    fn new() -> Self {
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
    fn build_ast(&mut self, node: &Node) -> AstNode {
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
                self.push_node("ClassNode".to_string(), Some(name), None);

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
                self.push_node("ModuleNode".to_string(), Some(name), None);

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
fn convert_node_to_ast(node: &Node) -> AstNode {
    let mut visitor = AstVisitor::new();
    visitor.build_ast(node)
}

// Legacy implementation for reference
#[allow(dead_code)]
fn convert_node_to_ast_legacy(node: &Node) -> AstNode {
    match node {
        Node::ProgramNode { .. } => {
            let program_node = node.as_program_node().unwrap();
            let mut children = Vec::new();

            // Process statements
            for stmt in program_node.statements().body().iter() {
                children.push(convert_node_to_ast(&stmt));
            }

            AstNode {
                node_type: "ProgramNode".to_string(),
                name: None,
                value: None,
                children,
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::ClassNode { .. } => {
            let class_node = node.as_class_node().unwrap();
            let mut children = Vec::new();

            // Process class body
            if let Some(body) = class_node.body() {
                let statements_node = body.as_statements_node().unwrap();
                let statements = statements_node.body();
                for stmt in statements.iter() {
                    children.push(convert_node_to_ast(&stmt));
                }
            }

            // Get class name
            let name = String::from_utf8_lossy(class_node.name().as_slice()).to_string();

            AstNode {
                node_type: "ClassNode".to_string(),
                name: Some(name),
                value: None,
                children,
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::DefNode { .. } => {
            let def_node = node.as_def_node().unwrap();
            let mut children = Vec::new();
            let mut parameters = Vec::new();

            // Process method parameters
            if let Some(params) = def_node.parameters() {
                // Get parameters from the parameters node
                for param in params.requireds().iter() {
                    let param_node = param.as_required_parameter_node().unwrap();
                    let param_name =
                        String::from_utf8_lossy(param_node.name().as_slice()).to_string();
                    parameters.push(param_name);
                }
            }

            // Process method body
            if let Some(body) = def_node.body() {
                let statements_node = body.as_statements_node().unwrap();
                let statements = statements_node.body();
                for stmt in statements.iter() {
                    children.push(convert_node_to_ast(&stmt));
                }
            }

            // Get method name
            let name = String::from_utf8_lossy(def_node.name().as_slice()).to_string();

            AstNode {
                node_type: "DefNode".to_string(),
                name: Some(name),
                value: None,
                children,
                parameters,
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::CallNode { .. } => {
            let call_node = node.as_call_node().unwrap();
            let mut arguments = Vec::new();

            // Process method arguments
            if let Some(args) = call_node.arguments() {
                for arg in args.arguments().iter() {
                    arguments.push(convert_node_to_ast(&arg));
                }
            }

            // Get method name
            let name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();

            // Process receiver
            let receiver = if let Some(recv) = call_node.receiver() {
                Some(Box::new(convert_node_to_ast(&recv)))
            } else {
                None
            };

            AstNode {
                node_type: "CallNode".to_string(),
                name: Some(name),
                value: None,
                children: Vec::new(),
                parameters: Vec::new(),
                receiver,
                arguments,
            }
        }
        Node::InstanceVariableReadNode { .. } => {
            let ivar_node = node.as_instance_variable_read_node().unwrap();
            let name = String::from_utf8_lossy(ivar_node.name().as_slice()).to_string();

            AstNode {
                node_type: "InstanceVariableReadNode".to_string(),
                name: Some(name),
                value: None,
                children: Vec::new(),
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::InstanceVariableWriteNode { .. } => {
            let ivasgn_node = node.as_instance_variable_write_node().unwrap();
            let name = String::from_utf8_lossy(ivasgn_node.name().as_slice()).to_string();

            let mut children = Vec::new();
            let value_node = ivasgn_node.value();
            children.push(convert_node_to_ast(&value_node));

            AstNode {
                node_type: "InstanceVariableWriteNode".to_string(),
                name: Some(name),
                value: None,
                children,
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::LocalVariableReadNode { .. } => {
            let lvar_node = node.as_local_variable_read_node().unwrap();
            let name = String::from_utf8_lossy(lvar_node.name().as_slice()).to_string();

            AstNode {
                node_type: "LocalVariableReadNode".to_string(),
                name: Some(name),
                value: None,
                children: Vec::new(),
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::LocalVariableWriteNode { .. } => {
            let lvasgn_node = node.as_local_variable_write_node().unwrap();
            let name = String::from_utf8_lossy(lvasgn_node.name().as_slice()).to_string();

            let mut children = Vec::new();
            let value_node = lvasgn_node.value();
            children.push(convert_node_to_ast(&value_node));

            AstNode {
                node_type: "LocalVariableWriteNode".to_string(),
                name: Some(name),
                value: None,
                children,
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::ConstantReadNode { .. } => {
            let constant_node = node.as_constant_read_node().unwrap();
            let name = String::from_utf8_lossy(constant_node.name().as_slice()).to_string();

            AstNode {
                node_type: "ConstantReadNode".to_string(),
                name: Some(name),
                value: None,
                children: Vec::new(),
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::StringNode { .. } => {
            let string_node = node.as_string_node().unwrap();
            let value = String::from_utf8_lossy(string_node.unescaped()).to_string();

            AstNode {
                node_type: "StringNode".to_string(),
                name: None,
                value: Some(value),
                children: Vec::new(),
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::IntegerNode { .. } => {
            let integer_node = node.as_integer_node().unwrap();
            let value = format!("{:?}", integer_node.value());

            AstNode {
                node_type: "IntegerNode".to_string(),
                name: None,
                value: Some(value),
                children: Vec::new(),
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        Node::InterpolatedStringNode { .. } => {
            let dstr_node = node.as_interpolated_string_node().unwrap();
            let mut children = Vec::new();

            // Process parts
            for part in dstr_node.parts().iter() {
                children.push(convert_node_to_ast(&part));
            }

            AstNode {
                node_type: "InterpolatedStringNode".to_string(),
                name: None,
                value: None,
                children,
                parameters: Vec::new(),
                receiver: None,
                arguments: Vec::new(),
            }
        }
        // Handle other node types as needed
        _ => AstNode {
            node_type: format!("UNKNOWN_{:?}", node),
            name: None,
            value: None,
            children: Vec::new(),
            parameters: Vec::new(),
            receiver: None,
            arguments: Vec::new(),
        },
    }
}

// Health check endpoint
async fn health_check(state: web::Data<AppState>) -> impl Responder {
    // Handle poisoned mutex gracefully
    let count_result = state.request_count.lock();
    let count = match count_result {
        Ok(count) => *count,
        Err(poisoned) => {
            // If the mutex is poisoned, recover the lock and continue
            *poisoned.into_inner()
        }
    };

    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "requests_processed": count
    }))
}

// Serve the HTML page
async fn index() -> Result<HttpResponse> {
    // Path to the HTML file
    let html_path = Path::new("static/index.html");

    // Check if the file exists in the current directory
    if html_path.exists() {
        let html_content = fs::read_to_string(html_path).map_err(|e| {
            eprintln!("Error reading HTML file: {}", e);
            actix_web::error::ErrorInternalServerError("Error reading HTML file")
        })?;

        Ok(HttpResponse::Ok()
            .content_type(ContentType::html())
            .body(html_content))
    } else {
        // Try to find the file relative to the crate directory
        let crate_html_path = Path::new("crates/ast-visualizer/static/index.html");

        if crate_html_path.exists() {
            let html_content = fs::read_to_string(crate_html_path).map_err(|e| {
                eprintln!("Error reading HTML file: {}", e);
                actix_web::error::ErrorInternalServerError("Error reading HTML file")
            })?;

            Ok(HttpResponse::Ok()
                .content_type(ContentType::html())
                .body(html_content))
        } else {
            eprintln!(
                "HTML file not found at {:?} or {:?}",
                html_path, crate_html_path
            );
            Ok(HttpResponse::NotFound().body("HTML file not found"))
        }
    }
}

// Function to find an available port starting from the given port
fn find_available_port(start_port: u16) -> std::io::Result<u16> {
    let mut port = start_port;
    let max_attempts = 10; // Try up to 10 ports

    for _ in 0..max_attempts {
        match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(_) => return Ok(port),
            Err(_) => {
                // Port is not available, try the next one
                port += 1;
            }
        }
    }

    // If we've tried all ports and none are available, return an error
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        format!(
            "Could not find an available port after trying {} ports",
            max_attempts
        ),
    ))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Get port from environment variable or use default (8080)
    let default_port = 8080;
    let port = match env::var("PORT") {
        Ok(port_str) => match port_str.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                eprintln!(
                    "Invalid PORT value: {}, using default: {}",
                    port_str, default_port
                );
                default_port
            }
        },
        Err(_) => default_port,
    };

    // Find an available port starting from the specified port
    let available_port = find_available_port(port)?;

    // Create app state
    let app_state = web::Data::new(AppState {
        request_count: Mutex::new(0),
    });

    println!("Starting AST server at http://127.0.0.1:{}", available_port);
    println!(
        "Open your browser and navigate to http://127.0.0.1:{} to use the AST visualizer",
        available_port
    );

    // Start HTTP server
    HttpServer::new(move || {
        // Configure CORS
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .route("/", web::get().to(index))
            .route("/health", web::get().to(health_check))
            .route("/parse", web::post().to(parse_ruby))
    })
    .bind(format!("127.0.0.1:{}", available_port))?
    .run()
    .await
}
