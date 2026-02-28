use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// Embed all JSON at compile time
static SCENARIOS_JSON: &str = include_str!("../data/scenarios.json");
static COMMITS_JSON: &str = include_str!("../data/commits.json");
static USERS_JSON: &str = include_str!("../data/users.json");

static GRAPHS: [&str; 8] = [
    include_str!("../data/graphs/1.json"),
    include_str!("../data/graphs/2.json"),
    include_str!("../data/graphs/3.json"),
    include_str!("../data/graphs/4.json"),
    include_str!("../data/graphs/5.json"),
    include_str!("../data/graphs/6.json"),
    include_str!("../data/graphs/7.json"),
    include_str!("../data/graphs/8.json"),
];

static RUNS: [&str; 8] = [
    include_str!("../data/runs/1.json"),
    include_str!("../data/runs/2.json"),
    include_str!("../data/runs/3.json"),
    include_str!("../data/runs/4.json"),
    include_str!("../data/runs/5.json"),
    include_str!("../data/runs/6.json"),
    include_str!("../data/runs/7.json"),
    include_str!("../data/runs/8.json"),
];

type AppState = Arc<RwLock<Vec<Value>>>;

fn assemble_scenarios() -> Vec<Value> {
    let base: Vec<Value> = serde_json::from_str(SCENARIOS_JSON).expect("invalid scenarios.json");
    base.into_iter()
        .map(|mut s| {
            let id = s["id"].as_u64().unwrap_or(1) as usize;
            let idx = id.saturating_sub(1).min(GRAPHS.len() - 1);
            let graph: Value = serde_json::from_str(GRAPHS[idx]).unwrap_or(json!([]));
            let runs: Value = serde_json::from_str(RUNS[idx]).unwrap_or(json!([]));
            let obj = s.as_object_mut().unwrap();
            obj.insert("graph".into(), graph);
            obj.insert("runs".into(), runs);
            Value::Object(obj.clone())
        })
        .collect()
}

async fn get_scenarios(State(state): State<AppState>) -> Json<Value> {
    let scenarios = state.read().await;
    let slim: Vec<Value> = scenarios
        .iter()
        .map(|s| {
            let mut obj = s.as_object().unwrap().clone();
            obj.remove("graph");
            Value::Object(obj)
        })
        .collect();
    Json(json!(slim))
}

async fn get_overview(State(state): State<AppState>) -> Json<Value> {
    let scenarios = state.read().await;
    let tracked = scenarios
        .iter()
        .filter(|s| s["pinned"].as_bool() == Some(true))
        .count();
    let commits: Vec<Value> = serde_json::from_str(COMMITS_JSON).expect("invalid commits.json");
    let latest = commits.last();
    Json(json!({
        "scenarioCount": scenarios.len(),
        "trackedCount": tracked,
        "latestCommitShortSha": latest.and_then(|c| c["shortSha"].as_str()),
        "latestCommitStatus": latest.and_then(|c| c["status"].as_str()),
    }))
}

async fn get_scenario(
    Path(id): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let scenarios = state.read().await;
    scenarios
        .iter()
        .find(|s| s["id"].as_u64() == Some(id))
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_commits() -> Json<Value> {
    let commits: Value = serde_json::from_str(COMMITS_JSON).expect("invalid commits.json");
    Json(commits)
}

async fn get_commit(Path(sha): Path<String>) -> Result<Json<Value>, StatusCode> {
    let commits: Vec<Value> = serde_json::from_str(COMMITS_JSON).expect("invalid commits.json");
    commits
        .into_iter()
        .find(|c| c["sha"].as_str() == Some(&sha) || c["shortSha"].as_str() == Some(&sha))
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_users() -> Json<Value> {
    let users: Value = serde_json::from_str(USERS_JSON).expect("invalid users.json");
    Json(users)
}

async fn toggle_pin(
    Path(id): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let mut scenarios = state.write().await;
    let scenario = scenarios
        .iter_mut()
        .find(|s| s["id"].as_u64() == Some(id))
        .ok_or(StatusCode::NOT_FOUND)?;

    let obj = scenario.as_object_mut().unwrap();
    let pinned = obj.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false);
    obj.insert("pinned".into(), json!(!pinned));

    Ok(Json(Value::Object(obj.clone())))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state: AppState = Arc::new(RwLock::new(assemble_scenarios()));

    let app = Router::new()
        .route("/api/overview", get(get_overview))
        .route("/api/scenarios", get(get_scenarios))
        .route("/api/scenarios/{id}", get(get_scenario))
        .route("/api/scenarios/{id}/pin", patch(toggle_pin))
        .route("/api/commits", get(get_commits))
        .route("/api/commits/{sha}", get(get_commit))
        .route("/api/users", get(get_users))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:3001";
    println!("Burrow listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
