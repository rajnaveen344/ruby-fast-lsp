use actix_cors::Cors;
use actix_web::http::header::ContentType;
use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result};
use ruby_prism::{parse, Node};
use serde::{Deserialize, Serialize};
use std::fs;
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
    // Increment request count
    let mut count = state.request_count.lock().unwrap();
    *count += 1;
    println!("Request #{}: Parsing Ruby code", *count);

    // Parse Ruby code
    let code = data.code.clone();
    let parse_result = parse(code.as_bytes());

    // Check for syntax errors
    let errors_count = parse_result.errors().count();
    if errors_count > 0 {
        let errors: Vec<String> = parse_result.errors().map(|e| format!("{:?}", e)).collect();

        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Syntax error",
            "details": errors
        }));
    }

    // Convert AST to JSON-serializable structure
    let ast = convert_node_to_ast(&parse_result.node());

    HttpResponse::Ok().json(ast)
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
    let count = state.request_count.lock().unwrap();
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "requests_processed": *count
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Create app state
    let app_state = web::Data::new(AppState {
        request_count: Mutex::new(0),
    });

    println!("Starting AST server at http://127.0.0.1:3000");
    println!("Open your browser and navigate to http://127.0.0.1:3000 to use the AST visualizer");

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
    .bind("127.0.0.1:3000")?
    .run()
    .await
}
