mod engine;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use engine::{ChoiceResult, CurrentNodeView, Engine};
use serde_json::json;
use std::{fs, sync::Arc};
use tokio::{net::TcpListener, sync::RwLock};

type AppState = Arc<RwLock<Engine<'static>>>;

fn get_available_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to find an available port")
        .local_addr()
        .unwrap()
        .port()
}

fn write_port_to_file(port: u16) {
    let data = json!({ "port": port }).to_string();
    fs::write("port.json", data).expect("Failed to write port to file");
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    source: String,
    #[arg(short, long, default_value_t = get_available_port())]
    port: u16,
}

async fn get_current(State(engine): State<AppState>) -> (StatusCode, Json<CurrentNodeView>) {
    let engine = engine.read().await;
    (StatusCode::OK, Json(engine.get_current_node_view()))
}

async fn choose_option(
    State(engine): State<AppState>,
    Path(option): Path<String>,
) -> (StatusCode, Json<ChoiceResult>) {
    let mut engine = engine.write().await;
    let result = engine.choose_option(option.to_string());

    let status = match &result {
        ChoiceResult::Success => StatusCode::OK,
        ChoiceResult::InvalidOption { .. } => StatusCode::BAD_REQUEST,
    };

    (status, Json(result))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    write_port_to_file(args.port);
    let source = fs::read_to_string(args.source).expect("Failed to read source file");
    let source: &str = Box::leak(source.into_boxed_str());
    let engine = match Engine::from_program(source) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("Failed to build engine due to the following errors:\n");
            for (i, error) in e.iter().enumerate() {
                eprintln!("{}. {error}", i + 1);
            }
            return;
        }
    };
    let state: AppState = Arc::new(RwLock::new(engine));

    let app = Router::new()
        .route("/current", get(get_current))
        .route("/choose/{option}", post(choose_option))
        .with_state(state);
    let addr = format!("127.0.0.1:{}", args.port);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
