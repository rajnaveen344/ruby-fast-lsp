mod ast;
mod server;
mod utils;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use std::env;
use std::sync::Mutex;

use server::{health_check, index, parse_ruby, AppState};
use utils::find_available_port;

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

    // Create and start the HTTP server
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