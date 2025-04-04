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
#[derive(Serialize)]
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

// Convert Prism AST node to our serializable structure
fn convert_node_to_ast(node: &Node) -> AstNode {
    match node {
        Node::ProgramNode { .. } => {
            let program_node = node.as_program_node().unwrap();
            let mut children = Vec::new();

            // Process statements
            for stmt in program_node.statements().body().iter() {
                children.push(convert_node_to_ast(&stmt));
            }

            AstNode {
                node_type: "PROGRAM".to_string(),
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
                node_type: "CLASS".to_string(),
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
                node_type: "DEF".to_string(),
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
                node_type: "SEND".to_string(),
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
                node_type: "IVAR".to_string(),
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
                node_type: "IVASGN".to_string(),
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
                node_type: "LVAR".to_string(),
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
                node_type: "LASGN".to_string(),
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
                node_type: "CONST".to_string(),
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
                node_type: "STR".to_string(),
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
                node_type: "INT".to_string(),
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
                node_type: "DSTR".to_string(),
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
