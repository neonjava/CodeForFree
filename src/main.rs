use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{env, sync::Arc};

type ApiError = (StatusCode, String);
#[derive(Clone)]
struct AppState {
    db_path: Arc<String>,
    client: reqwest::Client,
}
#[derive(Serialize)]
struct Project {
    id: i64,
    name: String,
}
#[derive(Serialize)]
struct Message {
    id: i64,
    role: String,
    content: String,
    mode: String,
}
#[derive(Deserialize)]
struct NewProject {
    name: String,
}
#[derive(Deserialize)]
struct ChatRequest {
    content: String,
    mode: String,
    model: Option<String>,
}

fn db(path: &str) -> Result<Connection, ApiError> {
    Connection::open(path).map_err(internal)
}
fn internal(error: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
fn ensure_project(conn: &Connection, id: i64) -> Result<(), ApiError> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if exists {
        Ok(())
    } else {
        Err((StatusCode::NOT_FOUND, "Project not found".into()))
    }
}
async fn projects(State(state): State<AppState>) -> Result<Json<Vec<Project>>, ApiError> {
    let conn = db(&state.db_path)?;
    let mut stmt = conn
        .prepare("SELECT id, name FROM projects ORDER BY id DESC")
        .map_err(internal)?;
    let items = stmt
        .query_map([], |r| {
            Ok(Project {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(internal)?;
    Ok(Json(
        items.collect::<Result<Vec<_>, _>>().map_err(internal)?,
    ))
}
async fn create_project(
    State(state): State<AppState>,
    Json(input): Json<NewProject>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Name must be 1–100 characters".into(),
        ));
    }
    let conn = db(&state.db_path)?;
    conn.execute("INSERT INTO projects(name) VALUES(?1)", [name])
        .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(Project {
            id: conn.last_insert_rowid(),
            name: name.into(),
        }),
    ))
}
async fn messages(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let conn = db(&state.db_path)?;
    ensure_project(&conn, id)?;
    let mut stmt = conn
        .prepare("SELECT id, role, content, mode FROM messages WHERE project_id=?1 ORDER BY id")
        .map_err(internal)?;
    let items = stmt
        .query_map([id], |r| {
            Ok(Message {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                mode: r.get(3)?,
            })
        })
        .map_err(internal)?;
    Ok(Json(
        items.collect::<Result<Vec<_>, _>>().map_err(internal)?,
    ))
}
async fn chat(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Json(input): Json<ChatRequest>,
) -> Result<Json<Message>, ApiError> {
    let content = input.content.trim();
    if content.is_empty() || content.len() > 100_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Message must be 1–100000 characters".into(),
        ));
    }
    if !["ask", "plan", "code"].contains(&input.mode.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Invalid mode".into()));
    }
    let history: Vec<Value> = {
        let conn = db(&state.db_path)?;
        ensure_project(&conn, id)?;
        let mut stmt = conn
            .prepare(
                "SELECT role, content FROM messages WHERE project_id=?1 ORDER BY id DESC LIMIT 30",
            )
            .map_err(internal)?;
        let rows = stmt
            .query_map([id], |r| {
                Ok(json!({"role": r.get::<_, String>(0)?, "content": r.get::<_, String>(1)?}))
            })
            .map_err(internal)?;
        let mut rows = rows.collect::<Result<Vec<_>, _>>().map_err(internal)?;
        rows.reverse();
        rows
    };
    let base = env::var("MODEL_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434/v1".into());
    let model = input
        .model
        .filter(|m| !m.trim().is_empty())
        .or_else(|| env::var("MODEL_NAME").ok())
        .unwrap_or_else(|| "llama3.2".into());
    let mut turns = vec![json!({"role":"system","content":match input.mode.as_str() {
        "plan" => "You are a coding assistant. Analyze the task and propose a clear implementation plan. Do not claim to have changed files.",
        "code" => "You are a coding assistant. Explain concrete code changes and provide code when useful. You cannot access or modify local files through this chat; never claim you did.",
        _ => "You are a helpful coding assistant. Be accurate and concise."
    }})];
    turns.extend(history);
    turns.push(json!({"role":"user","content":content}));
    let mut request = state
        .client
        .post(format!("{}/chat/completions", base.trim_end_matches('/')))
        .json(&json!({"model":model,"messages":turns,"stream":false}));
    if let Ok(key) = env::var("MODEL_API_KEY") {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Model connection failed: {e}"),
        )
    })?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Invalid model response: {e}"),
        )
    })?;
    if !status.is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "Provider error ({status}): {}",
                body.get("error").unwrap_or(&body)
            ),
        ));
    }
    let answer = body
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or((StatusCode::BAD_GATEWAY, "Provider returned no text".into()))?;
    let mut conn = db(&state.db_path)?;
    let tx = conn.transaction().map_err(internal)?;
    tx.execute(
        "INSERT INTO messages(project_id,role,content,mode) VALUES(?1,'user',?2,?3)",
        params![id, content, input.mode],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO messages(project_id,role,content,mode) VALUES(?1,'assistant',?2,?3)",
        params![id, answer, input.mode],
    )
    .map_err(internal)?;
    let message = Message {
        id: tx.last_insert_rowid(),
        role: "assistant".into(),
        content: answer.into(),
        mode: input.mode,
    };
    tx.commit().map_err(internal)?;
    Ok(Json(message))
}
async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}
async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = env::var("CODEFORFREE_DB").unwrap_or_else(|_| "codeforfree.sqlite3".into());
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS projects(id INTEGER PRIMARY KEY, name TEXT NOT NULL); CREATE TABLE IF NOT EXISTS messages(id INTEGER PRIMARY KEY, project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE, role TEXT NOT NULL, content TEXT NOT NULL, mode TEXT NOT NULL);")?;
    drop(conn);
    let state = AppState {
        db_path: Arc::new(db_path),
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?,
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/projects", get(projects).post(create_project))
        .route("/api/projects/{id}/messages", get(messages))
        .route("/api/projects/{id}/chat", post(chat))
        .with_state(state);
    let bind = env::var("CODEFORFREE_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    println!("CodeForFree running at http://{bind}");
    axum::serve(tokio::net::TcpListener::bind(&bind).await?, app).await?;
    Ok(())
}
