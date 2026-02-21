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
    session_timeout_hours: f32,
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
    #[arg(long, default_value_t = String::new())]
    prefix: String,
    #[arg(long, default_value_t = 24.0)]
    session_timeout_hours: f32,
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

async fn clear_expired_sessions(state: &SharedState) {
    let mut sessions = state.sessions.write().await;
    let mut expired_sessions: Vec<String> = Vec::new();
    for (session_id, session_arc) in sessions.iter() {
        let session = session_arc.lock().await;
        if session.is_expired(state.session_timeout_hours) {
            println!("Session {session_id} has expired and will be removed.");
            expired_sessions.push(session_id.clone());
        }
    }

    for session_id in expired_sessions {
        sessions.remove(&session_id);
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
    let mut session = session_arc.lock().await;
    session.update_last_active_at();
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
        session_timeout_hours: args.session_timeout_hours,
    });

    let prefix = args.prefix.clone();
    let app = Router::new()
        .route(
            format!("{prefix}/clear_expired_sessions").as_str(),
            post(|State(state): State<AppState>| async move {
                clear_expired_sessions(&state).await;
                StatusCode::OK
            }),
        )
        .route(format!("{prefix}/session").as_str(), post(create_session))
        .route(
            format!("{prefix}/session/{{session_id}}/current").as_str(),
            get(get_current),
        )
        .route(
            format!("{prefix}/session/{{session_id}}/choose/{{option}}").as_str(),
            post(choose_option),
        )
        .with_state(state);

    let addr = format!("127.0.0.1:{}", args.port);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
