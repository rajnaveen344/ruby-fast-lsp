use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse, Responder, Result};
use ruby_prism::parse;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use crate::ast::convert_node_to_ast;
use crate::utils::offset_to_position;

// Request structure for parsing Ruby code
#[derive(Deserialize, Serialize)]
pub struct ParseRequest {
    pub code: String,
}

// State to track request count
pub struct AppState {
    pub request_count: Mutex<usize>,
}

// Parse Ruby code and return AST as JSON
pub async fn parse_ruby(
    data: web::Json<ParseRequest>,
    state: web::Data<AppState>,
) -> impl Responder {
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

    // If there are some errors, return them in a structured format
    if errors_count > 0 {
        // Create a structured error response that can be visualized
        let error_nodes: Vec<serde_json::Value> = parse_result
            .errors()
            .map(|e| {
                let location = e.location();
                let message = format!("{:?}", e);

                // Get location information
                let start_offset = location.start_offset();
                let _end_offset = location.end_offset(); // We could use this for error highlighting

                // Calculate line and column from the code string
                let (start_line, start_column) = offset_to_position(&code, start_offset);

                serde_json::json!({
                    "type": "SyntaxErrorNode",
                    "message": message,
                    "line": start_line,
                    "column": start_column
                })
            })
            .collect();

        // Return a special AST structure for errors
        return HttpResponse::Ok().json(serde_json::json!({
            "type": "ErrorTreeNode",
            "name": "Syntax Errors",
            "children": error_nodes
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

// Health check endpoint
pub async fn health_check(state: web::Data<AppState>) -> impl Responder {
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
pub async fn index() -> Result<HttpResponse> {
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
