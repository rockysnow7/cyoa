mod engine;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use engine::{ChoiceResult, CurrentNodeView, Engine, Session};
use serde::Serialize;
use serde_json::json;
use std::{collections::HashMap, fs, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
};
use uuid::Uuid;

struct SharedState {
    story: Engine<'static>,
    sessions: RwLock<HashMap<String, Arc<Mutex<Session>>>>,
}

type AppState = Arc<SharedState>;

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

#[derive(Serialize)]
struct CreateSessionResponse {
    session_id: String,
}

async fn create_session(State(state): State<AppState>) -> Json<CreateSessionResponse> {
    let session_id = Uuid::new_v4().to_string();
    let session = state.story.new_session();
    state
        .sessions
        .write()
        .await
        .insert(session_id.clone(), Arc::new(Mutex::new(session)));
    println!("Created new session with ID: {session_id}");

    Json(CreateSessionResponse { session_id })
}

async fn clear_old_sessions(state: &SharedState) {
    for (session_id, session_arc) in state.sessions.write().await.iter() {
        let session = session_arc.lock().await;
        if session.is_expired() {
            println!("Session {session_id} has expired and will be removed.");
            state.sessions.write().await.remove(session_id);
        }
    }
}

type ApiError = (StatusCode, Json<serde_json::Value>);

fn session_not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "session not found" })),
    )
}

async fn get_session_arc(state: &SharedState, session_id: &str) -> Option<Arc<Mutex<Session>>> {
    state.sessions.read().await.get(session_id).map(Arc::clone)
}

async fn get_current(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<CurrentNodeView>, ApiError> {
    let session_arc = get_session_arc(&state, &session_id)
        .await
        .ok_or_else(session_not_found)?;
    let session = session_arc.lock().await;
    Ok(Json(state.story.get_current_node_view(&session)))
}

async fn choose_option(
    State(state): State<AppState>,
    Path((session_id, option)): Path<(String, String)>,
) -> Result<(StatusCode, Json<ChoiceResult>), ApiError> {
    let session_arc = get_session_arc(&state, &session_id)
        .await
        .ok_or_else(session_not_found)?;
    let mut session = session_arc.lock().await;
    let result = state.story.choose_option(&mut session, option);

    let status = match &result {
        ChoiceResult::Success => StatusCode::OK,
        ChoiceResult::InvalidOption { .. } => StatusCode::BAD_REQUEST,
    };

    Ok((status, Json(result)))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    write_port_to_file(args.port);
    let source = fs::read_to_string(args.source).expect("Failed to read source file");
    let source: &'static str = Box::leak(source.into_boxed_str());
    let story = match Engine::from_program(source) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("Failed to build engine due to the following errors:\n");
            for (i, error) in e.iter().enumerate() {
                eprintln!("{}. {error}", i + 1);
            }
            return;
        }
    };

    let state: AppState = Arc::new(SharedState {
        story,
        sessions: RwLock::new(HashMap::new()),
    });

    let app = Router::new()
        .route(
            "/clear_old_sessions",
            post(|State(state): State<AppState>| async move {
                clear_old_sessions(&state).await;
                StatusCode::OK
            }),
        )
        .route("/session", post(create_session))
        .route("/session/{session_id}/current", get(get_current))
        .route("/session/{session_id}/choose/{option}", post(choose_option))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", args.port);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
