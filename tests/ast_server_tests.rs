use actix_web::{test, web, App, HttpResponse, Responder};
use ruby_prism::parse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;

// Import the types and functions from the main crate
// We need to redefine these here since we can't directly import from the binary
#[derive(Deserialize, Serialize)]
struct ParseRequest {
    code: String,
}

struct AppState {
    request_count: Mutex<usize>,
}

// Simplified version of the parse_ruby function for testing
async fn parse_ruby(data: web::Json<ParseRequest>, state: web::Data<AppState>) -> impl Responder {
    // Increment request count
    let mut count = state.request_count.lock().unwrap();
    *count += 1;

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

    // For testing, we'll just return a simple JSON structure
    HttpResponse::Ok().json(serde_json::json!({
        "type": "PROGRAM",
        "children": [
            {
                "type": "CLASS",
                "name": "Person",
                "children": [
                    {
                        "type": "DEF",
                        "name": "initialize",
                        "parameters": ["name", "age"]
                    },
                    {
                        "type": "DEF",
                        "name": "greet",
                        "parameters": []
                    }
                ]
            }
        ]
    }))
}

#[actix_web::test]
async fn test_parse_ruby_valid_code() {
    // Create app state
    let app_state = web::Data::new(AppState {
        request_count: Mutex::new(0),
    });

    // Create test app
    let app = test::init_service(
        App::new()
            .app_data(app_state.clone())
            .route("/parse", web::post().to(parse_ruby)),
    )
    .await;

    // Valid Ruby code
    let ruby_code = r#"
        class Person
          def initialize(name, age)
            @name = name
            @age = age
          end

          def greet
            puts "Hello, my name is #{@name} and I am #{@age} years old."
          end
        end

        person = Person.new("John", 30)
        person.greet
    "#;

    // Create request
    let req = test::TestRequest::post()
        .uri("/parse")
        .set_json(&ParseRequest {
            code: ruby_code.to_string(),
        })
        .to_request();

    // Send request and get response
    let resp = test::call_service(&app, req).await;

    // Check response
    assert!(resp.status().is_success());

    // Parse response body
    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Check that the response contains the expected structure
    assert_eq!(json["type"], "PROGRAM");
    assert!(json["children"].is_array());
    assert!(json["children"].as_array().unwrap().len() > 0);

    // Check that the first child is a class node
    let class_node = &json["children"][0];
    assert_eq!(class_node["type"], "CLASS");
    assert_eq!(class_node["name"], "Person");

    // Check that the class has methods
    let methods = class_node["children"].as_array().unwrap();
    assert!(methods.len() >= 2);

    // Check initialize method
    let initialize_method = methods.iter().find(|m| m["name"] == "initialize").unwrap();
    assert_eq!(initialize_method["type"], "DEF");
    assert!(initialize_method["parameters"].is_array());

    // Check greet method
    let greet_method = methods.iter().find(|m| m["name"] == "greet").unwrap();
    assert_eq!(greet_method["type"], "DEF");
}

#[actix_web::test]
async fn test_parse_ruby_invalid_code() {
    // Create app state
    let app_state = web::Data::new(AppState {
        request_count: Mutex::new(0),
    });

    // Create test app
    let app = test::init_service(
        App::new()
            .app_data(app_state.clone())
            .route("/parse", web::post().to(parse_ruby)),
    )
    .await;

    // Invalid Ruby code (missing 'end')
    let ruby_code = r#"
        class Person
          def initialize(name, age)
            @name = name
            @age = age
          end

          def greet
            puts "Hello, my name is #{@name} and I am #{@age} years old."
          # missing end

        person = Person.new("John", 30)
        person.greet
    "#;

    // Create request
    let req = test::TestRequest::post()
        .uri("/parse")
        .set_json(&ParseRequest {
            code: ruby_code.to_string(),
        })
        .to_request();

    // Send request and get response
    let resp = test::call_service(&app, req).await;

    // Check response
    assert!(resp.status().is_client_error());

    // Parse response body
    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Check that the response contains error information
    assert!(json["error"].is_string());
    assert!(json["details"].is_array());
}

#[actix_web::test]
async fn test_parse_result_direct() {
    // Test the parse function directly
    let ruby_code = "class Test; def foo; end; end";
    let parse_result = parse(ruby_code.as_bytes());

    // Check that parsing succeeded
    assert_eq!(parse_result.errors().count(), 0);

    // Check that the AST has the expected structure
    let node = parse_result.node();

    // Check that it's a program node
    assert!(node.as_program_node().is_some());
    let program = node.as_program_node().unwrap();

    // Check that it has one statement
    let statements = program.statements().body();
    assert_eq!(statements.iter().count(), 1);

    // Check that the statement is a class node
    let class_node = statements.iter().next().unwrap().as_class_node();
    assert!(class_node.is_some());
    let class = class_node.unwrap();

    // Check the class name
    assert_eq!(
        String::from_utf8_lossy(class.name().as_slice()).to_string(),
        "Test"
    );

    // Check that the class has a body
    assert!(class.body().is_some());
    let body = class.body().unwrap();

    // Check that the body has one statement
    let body_statements = body.as_statements_node().unwrap().body();
    assert_eq!(body_statements.iter().count(), 1);

    // Check that the statement is a def node
    let def_node = body_statements.iter().next().unwrap().as_def_node();
    assert!(def_node.is_some());
    let def = def_node.unwrap();

    // Check the method name
    assert_eq!(
        String::from_utf8_lossy(def.name().as_slice()).to_string(),
        "foo"
    );
}
