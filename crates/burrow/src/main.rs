mod auth;
mod db;
mod github;
pub mod github_app;
mod models;
pub mod regression;
mod routes;
mod scheduler;

use axum::{
    Json, Router,
    extract::{FromRef, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, patch, post},
};
use axum_extra::extract::CookieJar;
use clap::Parser;
use sqlx::PgPool;
use std::sync::OnceLock;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use routes::bisections::*;
use routes::branches::*;
use routes::commits::*;
use routes::forager::*;
use routes::observations::*;
use routes::pheromones::*;
use routes::projects::*;
use routes::repos::*;
use routes::setup::*;
use routes::tools::*;
use routes::webhooks::*;

#[derive(Parser)]
#[command(name = "burrow", about = "Wezel API server")]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value = "3001")]
    port: u16,
    /// Directory to cache downloaded pheromone tarballs
    #[arg(long, env = "WEZEL_CACHE_DIR", default_value = ".wezel/cache")]
    cache_dir: std::path::PathBuf,
}

static CACHE_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

pub fn cache_dir() -> &'static std::path::PathBuf {
    CACHE_DIR.get().expect("cache_dir not initialized")
}

pub type ApiResult<T> = Result<T, StatusCode>;

pub fn ise<E: std::fmt::Debug>(e: E) -> StatusCode {
    tracing::error!("internal error: {:?}", e);
    StatusCode::INTERNAL_SERVER_ERROR
}

// ── AppState ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub http: reqwest::Client,
    pub github_app: github_app::AppConfig,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

impl AppState {
    pub fn github_host(&self) -> String {
        self.github_app
            .read()
            .ok()
            .and_then(|c| c.as_ref().map(|c| c.github_host.clone()))
            .unwrap_or_else(|| "github.com".to_string())
    }

    pub fn api_base(&self) -> String {
        github_app::api_base_url(&self.github_host())
    }

    /// Resolve a GitHub token for the given repo owner, or None if no
    /// installation covers this owner.
    pub async fn github_token(&self, owner: &str) -> ApiResult<Option<String>> {
        let config = {
            let guard = self.github_app.read().map_err(ise)?;
            guard.clone()
        };
        let Some(config) = config.as_ref() else {
            return Ok(None);
        };
        github_app::resolve_token(&self.pool, &self.http, config, owner).await
    }
}

// ── Middleware ──────────────────────────────────────────────────────────────

async fn get_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let app_configured = state.github_app.read().map_err(ise)?.is_some();
    if !app_configured {
        // Setup not done yet — skip auth.
        return Ok(next.run(req).await);
    }

    let session_id = jar
        .get("session_id")
        .map(|c| c.value().to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let login = db::get_session(&state.pool, &session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(auth::AuthUser { login });
    Ok(next.run(req).await)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    CACHE_DIR.set(cli.cache_dir).expect("CACHE_DIR already set");

    let db_url = db::db_url();
    tracing::info!("connecting to database at {db_url}");
    let pool = db::connect(&db_url)
        .await
        .expect("could not connect to database");

    regression::set_detector(std::sync::Arc::new(regression::ThresholdDetector::default()));

    // Load GitHub App config from DB.
    let github_app = github_app::new_app_config();
    if let Ok(Some(config)) = github_app::load_config(&pool).await {
        tracing::info!("github app configured: {}", config.app_slug);
        *github_app.write().unwrap() = Some(config);
    }

    let state = AppState {
        pool: pool.clone(),
        http: reqwest::Client::new(),
        github_app,
    };

    if let Some(dev_dir) = get_dev_dir() {
        load_dev_pheromones(&state, &dev_dir).await;
    }

    scheduler::spawn(state.clone());

    // Protected API routes.
    let protected_api: Router<AppState> = Router::new()
        .route("/api/project", get(get_projects).post(create_project))
        .route("/api/project/{project_id}", patch(rename_project))
        .route(
            "/api/project/{project_id}/overview",
            get(get_project_overview),
        )
        .route(
            "/api/project/{project_id}/observation",
            get(get_project_observations),
        )
        .route(
            "/api/project/{project_id}/observation/{id}",
            get(get_project_observation),
        )
        .route(
            "/api/project/{project_id}/observation/{id}/pin",
            patch(toggle_project_observation_pin),
        )
        .route(
            "/api/project/{project_id}/commit",
            get(get_project_commits).post(schedule_project_commit),
        )
        .route(
            "/api/project/{project_id}/commit/{sha}",
            get(get_project_commit),
        )
        .route(
            "/api/project/{project_id}/github/commit/{sha}",
            get(get_project_github_commit),
        )
        .route("/api/project/{project_id}/user", get(get_users))
        .route(
            "/api/project/{project_id}/experiments",
            get(get_project_experiments),
        )
        .route(
            "/api/project/{project_id}/bisections",
            get(get_project_bisections),
        )
        .route(
            "/api/project/{project_id}/bisections/{bisection_id}",
            get(get_project_bisection).patch(patch_project_bisection),
        )
        .route(
            "/api/project/{project_id}/branch/{branch}/timeline",
            get(get_branch_timeline),
        )
        .route(
            "/api/project/{project_id}/compare",
            get(get_project_compare),
        )
        .route(
            "/api/project/{project_id}/experiment/pr",
            post(post_experiment_pr),
        )
        .route(
            "/api/admin/pheromone",
            get(get_admin_pheromones).post(post_admin_pheromone),
        )
        .route(
            "/api/admin/pheromone/{name}/fetch",
            post(post_admin_pheromone_fetch),
        )
        .route("/api/repo", get(get_repos))
        .route("/api/github/repos", get(get_github_repos))
        .route("/api/overview", get(get_overview))
        .route("/api/observation", get(get_observations))
        .route("/api/observation/{id}", get(get_observation))
        .route("/api/observation/{id}/pin", patch(toggle_observation_pin))
        .route("/api/commit", get(get_commits))
        .route("/api/commit/{sha}", get(get_commit))
        .route("/api/user", get(get_users))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let app = Router::<AppState>::new()
        .merge(protected_api)
        // Unauthenticated: ingest, forager, setup, and auth routes.
        .route("/api/events", post(ingest_events))
        .route("/api/forager/run", post(post_forager_run))
        .route("/api/forager/jobs", post(post_forager_jobs))
        .route("/api/forager/jobs/next", post(post_forager_jobs_next))
        .route("/api/forager/jobs/{id}", patch(patch_forager_job))
        .route("/api/pheromones", get(get_pheromones))
        .route(
            "/api/pheromone/{name}/binary/{target}",
            get(get_pheromone_binary),
        )
        .route("/api/tools", get(get_tools))
        .route("/api/tools/{name}/binary/{target}", get(get_tool_binary))
        // Setup routes.
        .route("/api/setup/status", get(get_setup_status))
        .route("/api/setup/github-app/manifest", post(post_manifest))
        .route("/api/setup/github-app/callback", get(get_app_callback))
        .route(
            "/api/setup/github-app/install-callback",
            get(get_install_callback),
        )
        // Auth routes.
        .route("/auth/github", get(auth::login))
        .route("/auth/github/callback", get(auth::callback))
        .route("/auth/me", get(auth::me))
        .route("/auth/config", get(auth::config))
        .route("/auth/logout", post(auth::logout))
        .route("/api/webhooks/github", post(post_github_webhook))
        .route("/health", get(get_health))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cli.port);
    tracing::info!("burrow listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
        .unwrap();
}
