mod db;
mod models;

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use clap::Parser;
use models::*;
use serde_json::Value;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Parser)]
#[command(name = "burrow", about = "Wezel API server")]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value = "3001")]
    port: u16,
}

type ApiResult<T> = Result<T, StatusCode>;

fn ise<E: std::fmt::Debug>(_: E) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn build_graph(pool: &PgPool, scenario_id: i64) -> ApiResult<Vec<GraphNodeJson>> {
    let nodes = sqlx::query_as::<_, GraphNodeRow>(
        "SELECT name FROM graph_nodes WHERE scenario_id = $1 ORDER BY name",
    )
    .bind(scenario_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let edges = sqlx::query_as::<_, GraphEdgeRow>(
        "SELECT src.name AS source_name, tgt.name AS dep_name \
         FROM graph_edges ge \
         JOIN graph_nodes src ON src.id = ge.source_id \
         JOIN graph_nodes tgt ON tgt.id = ge.target_id \
         WHERE src.scenario_id = $1",
    )
    .bind(scenario_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
    for node in &nodes {
        deps_map.entry(node.name.clone()).or_default();
    }
    for edge in edges {
        deps_map
            .entry(edge.source_name)
            .or_default()
            .push(edge.dep_name);
    }

    let graph: Vec<GraphNodeJson> = nodes
        .into_iter()
        .map(|n| {
            let deps = deps_map.remove(&n.name).unwrap_or_default();
            GraphNodeJson { name: n.name, deps }
        })
        .collect();

    Ok(graph)
}

async fn scenario_to_json(
    pool: &PgPool,
    id: i64,
    include_graph: bool,
) -> ApiResult<Option<ScenarioJson>> {
    let Some(s) = sqlx::query_as::<_, Scenario>(
        "SELECT id, name, profile, pinned, platform FROM scenarios WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(ise)?
    else {
        return Ok(None);
    };

    let runs = sqlx::query_as::<_, Run>(
        "SELECT id, scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms \
         FROM runs WHERE scenario_id = $1 ORDER BY timestamp",
    )
    .bind(s.id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();

    // Fetch all dirty crates for these runs in one query
    let dirty_crates = sqlx::query_as::<_, DirtyCrate>(
        "SELECT run_id, crate_name FROM run_dirty_crates WHERE run_id = ANY($1) ORDER BY crate_name",
    )
    .bind(&run_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut dirty_map: HashMap<i64, Vec<String>> = HashMap::new();
    for dc in dirty_crates {
        dirty_map.entry(dc.run_id).or_default().push(dc.crate_name);
    }

    let runs_json: Vec<RunJson> = runs
        .into_iter()
        .map(|r| {
            let crates = dirty_map.remove(&r.id).unwrap_or_default();
            RunJson {
                user: r.user,
                platform: r.platform,
                timestamp: r.timestamp,
                commit: r.commit_short,
                build_time_ms: r.build_time_ms,
                dirty_crates: crates,
            }
        })
        .collect();

    let graph = if include_graph {
        let g = build_graph(pool, s.id).await?;
        if g.is_empty() { None } else { Some(g) }
    } else {
        None
    };

    Ok(Some(ScenarioJson {
        id: s.id,
        name: s.name,
        profile: s.profile,
        pinned: s.pinned,
        platform: s.platform,
        runs: runs_json,
        graph,
    }))
}

async fn commit_to_json(pool: &PgPool, commit_id: i64) -> ApiResult<CommitJson> {
    let c = sqlx::query_as::<_, Commit>(
        "SELECT id, sha, short_sha, author, message, timestamp, status FROM commits WHERE id = $1",
    )
    .bind(commit_id)
    .fetch_one(pool)
    .await
    .map_err(ise)?;

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, name, kind, status, value, prev_value, unit \
         FROM measurements WHERE commit_id = $1 ORDER BY id",
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();

    // Fetch all details for these measurements in one query
    let details = sqlx::query_as::<_, MeasurementDetail>(
        "SELECT measurement_id, name, value, prev_value \
         FROM measurement_details WHERE measurement_id = ANY($1) ORDER BY id",
    )
    .bind(&m_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut detail_map: HashMap<i64, Vec<MeasurementDetailJson>> = HashMap::new();
    for d in details {
        detail_map
            .entry(d.measurement_id)
            .or_default()
            .push(MeasurementDetailJson {
                name: d.name,
                value: d.value,
                prev_value: d.prev_value,
            });
    }

    let measurements_json: Vec<MeasurementJson> = measurements
        .into_iter()
        .map(|m| {
            let detail = detail_map.remove(&m.id).unwrap_or_default();
            MeasurementJson {
                id: m.id,
                name: m.name,
                kind: m.kind,
                status: m.status,
                value: m.value,
                prev_value: m.prev_value,
                unit: m.unit,
                detail,
            }
        })
        .collect();

    Ok(CommitJson {
        sha: c.sha,
        short_sha: c.short_sha,
        author: c.author,
        message: c.message,
        timestamp: c.timestamp,
        status: c.status,
        measurements: measurements_json,
    })
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn create_project(
    State(pool): State<PgPool>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Project>)> {
    let name = body["name"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let upstream = body["upstream"].as_str().ok_or(StatusCode::BAD_REQUEST)?;

    let project = sqlx::query_as::<_, Project>(
        "INSERT INTO projects (name, upstream) VALUES ($1, $2) RETURNING id, name, upstream",
    )
    .bind(name)
    .bind(upstream)
    .fetch_one(&pool)
    .await
    .map_err(ise)?;

    Ok((StatusCode::CREATED, Json(project)))
}

async fn get_projects(State(pool): State<PgPool>) -> ApiResult<Json<Vec<Project>>> {
    let projects =
        sqlx::query_as::<_, Project>("SELECT id, name, upstream FROM projects ORDER BY id")
            .fetch_all(&pool)
            .await
            .map_err(ise)?;
    Ok(Json(projects))
}

async fn get_overview(State(pool): State<PgPool>) -> ApiResult<Json<OverviewJson>> {
    let (scenario_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scenarios")
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

    let (tracked_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM scenarios WHERE pinned = TRUE")
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    let latest = sqlx::query_as::<_, LatestCommit>(
        "SELECT short_sha, status FROM commits ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(OverviewJson {
        scenario_count,
        tracked_count,
        latest_commit_short_sha: latest.as_ref().map(|l| l.short_sha.clone()),
        latest_commit_status: latest.map(|l| l.status),
    }))
}

async fn get_scenarios(State(pool): State<PgPool>) -> ApiResult<Json<Vec<ScenarioJson>>> {
    let scenarios = sqlx::query_as::<_, Scenario>(
        "SELECT id, name, profile, pinned, platform FROM scenarios ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    if scenarios.is_empty() {
        return Ok(Json(vec![]));
    }

    let scenario_ids: Vec<i64> = scenarios.iter().map(|s| s.id).collect();

    let runs = sqlx::query_as::<_, Run>(
        "SELECT id, scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms \
         FROM runs WHERE scenario_id = ANY($1) ORDER BY timestamp",
    )
    .bind(&scenario_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();

    let dirty_crates = sqlx::query_as::<_, DirtyCrate>(
        "SELECT run_id, crate_name FROM run_dirty_crates \
         WHERE run_id = ANY($1) ORDER BY crate_name",
    )
    .bind(&run_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let mut dirty_map: HashMap<i64, Vec<String>> = HashMap::new();
    for dc in dirty_crates {
        dirty_map.entry(dc.run_id).or_default().push(dc.crate_name);
    }

    let mut runs_by_scenario: HashMap<i64, Vec<RunJson>> = HashMap::new();
    for r in runs {
        let crates = dirty_map.remove(&r.id).unwrap_or_default();
        runs_by_scenario
            .entry(r.scenario_id)
            .or_default()
            .push(RunJson {
                user: r.user,
                platform: r.platform,
                timestamp: r.timestamp,
                commit: r.commit_short,
                build_time_ms: r.build_time_ms,
                dirty_crates: crates,
            });
    }

    let out: Vec<ScenarioJson> = scenarios
        .into_iter()
        .map(|s| ScenarioJson {
            id: s.id,
            name: s.name,
            profile: s.profile,
            pinned: s.pinned,
            platform: s.platform,
            runs: runs_by_scenario.remove(&s.id).unwrap_or_default(),
            graph: None,
        })
        .collect();

    Ok(Json(out))
}

async fn get_scenario(
    Path(id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ScenarioJson>> {
    scenario_to_json(&pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_scenario_p(
    Path((_pid, id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ScenarioJson>> {
    scenario_to_json(&pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_commits(State(pool): State<PgPool>) -> ApiResult<Json<Vec<CommitJson>>> {
    let commits = sqlx::query_as::<_, Commit>(
        "SELECT id, sha, short_sha, author, message, timestamp, status \
         FROM commits ORDER BY timestamp",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    if commits.is_empty() {
        return Ok(Json(vec![]));
    }

    let commit_ids: Vec<i64> = commits.iter().map(|c| c.id).collect();

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, name, kind, status, value, prev_value, unit \
         FROM measurements WHERE commit_id = ANY($1) ORDER BY id",
    )
    .bind(&commit_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();

    let details = sqlx::query_as::<_, MeasurementDetail>(
        "SELECT measurement_id, name, value, prev_value \
         FROM measurement_details WHERE measurement_id = ANY($1) ORDER BY id",
    )
    .bind(&m_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let mut detail_map: HashMap<i64, Vec<MeasurementDetailJson>> = HashMap::new();
    for d in details {
        detail_map
            .entry(d.measurement_id)
            .or_default()
            .push(MeasurementDetailJson {
                name: d.name,
                value: d.value,
                prev_value: d.prev_value,
            });
    }

    let mut measurements_by_commit: HashMap<i64, Vec<MeasurementJson>> = HashMap::new();
    for m in measurements {
        measurements_by_commit
            .entry(m.commit_id)
            .or_default()
            .push(MeasurementJson {
                id: m.id,
                name: m.name,
                kind: m.kind,
                status: m.status,
                value: m.value,
                prev_value: m.prev_value,
                unit: m.unit,
                detail: detail_map.remove(&m.id).unwrap_or_default(),
            });
    }

    let out: Vec<CommitJson> = commits
        .into_iter()
        .map(|c| CommitJson {
            sha: c.sha,
            short_sha: c.short_sha,
            author: c.author,
            message: c.message,
            timestamp: c.timestamp,
            status: c.status,
            measurements: measurements_by_commit.remove(&c.id).unwrap_or_default(),
        })
        .collect();

    Ok(Json(out))
}

async fn get_commit(
    Path(sha): Path<String>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<CommitJson>> {
    get_commit_inner(&sha, &pool).await
}

async fn get_commit_p(
    Path((_pid, sha)): Path<(i64, String)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<CommitJson>> {
    get_commit_inner(&sha, &pool).await
}

async fn get_commit_inner(sha: &str, pool: &PgPool) -> ApiResult<Json<CommitJson>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM commits WHERE sha = $1 OR short_sha = $2")
            .bind(sha)
            .bind(sha)
            .fetch_optional(pool)
            .await
            .map_err(ise)?;

    match row {
        Some((id,)) => Ok(Json(commit_to_json(pool, id).await?)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_users(State(pool): State<PgPool>) -> ApiResult<Json<Vec<String>>> {
    let rows: Vec<User> = sqlx::query_as("SELECT username FROM users ORDER BY username")
        .fetch_all(&pool)
        .await
        .map_err(ise)?;
    Ok(Json(rows.into_iter().map(|u| u.username).collect()))
}

async fn toggle_pin(
    Path(id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ScenarioJson>> {
    toggle_pin_inner(id, &pool).await
}

async fn toggle_pin_p(
    Path((_pid, id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ScenarioJson>> {
    toggle_pin_inner(id, &pool).await
}

async fn toggle_pin_inner(id: i64, pool: &PgPool) -> ApiResult<Json<ScenarioJson>> {
    let result = sqlx::query("UPDATE scenarios SET pinned = NOT pinned WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(ise)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    scenario_to_json(pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn ingest_events(
    State(pool): State<PgPool>,
    Json(events): Json<Vec<Value>>,
) -> ApiResult<StatusCode> {
    for event in &events {
        let Some(upstream) = event.get("upstream").and_then(|v| v.as_str()) else {
            continue;
        };
        let user = event
            .get("user")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let timestamp = event
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let duration_ms = event
            .get("durationMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let run_platform = event.get("platform").and_then(|v| v.as_str()).unwrap_or("");

        // Find or create project
        let name = upstream.rsplit('/').next().unwrap_or(upstream);
        let project_id: i64 =
            match sqlx::query_as::<_, (i64,)>("SELECT id FROM projects WHERE upstream = $1")
                .bind(upstream)
                .fetch_optional(&pool)
                .await
                .map_err(ise)?
            {
                Some((id,)) => id,
                None => {
                    sqlx::query_as::<_, IdRow>(
                        "INSERT INTO projects (name, upstream) VALUES ($1, $2) RETURNING id",
                    )
                    .bind(name)
                    .bind(upstream)
                    .fetch_one(&pool)
                    .await
                    .map_err(ise)?
                    .id
                }
            };

        // Ensure user exists
        sqlx::query("INSERT INTO users (username) VALUES ($1) ON CONFLICT (username) DO NOTHING")
            .bind(user)
            .execute(&pool)
            .await
            .map_err(ise)?;

        // Process pheromone data if present
        let Some(pheromone) = event.get("pheromone") else {
            continue;
        };
        let tool = pheromone
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let command = pheromone
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("build");
        let profile = pheromone
            .get("profile")
            .and_then(|v| v.as_str())
            .unwrap_or("dev");
        let packages: Vec<&str> = pheromone
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let scenario_platform: Option<&str> = pheromone.get("platform").and_then(|v| v.as_str());

        let scenario_name = if packages.is_empty() {
            format!("{tool} {command}")
        } else {
            format!("{tool} {command} {}", packages.join(" "))
        };

        // Find or create scenario
        let scenario_id: i64 = match if let Some(sp) = scenario_platform {
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM scenarios \
                 WHERE project_id = $1 AND name = $2 AND profile = $3 AND platform = $4",
            )
            .bind(project_id)
            .bind(&scenario_name)
            .bind(profile)
            .bind(sp)
            .fetch_optional(&pool)
            .await
        } else {
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM scenarios \
                 WHERE project_id = $1 AND name = $2 AND profile = $3 AND platform IS NULL",
            )
            .bind(project_id)
            .bind(&scenario_name)
            .bind(profile)
            .fetch_optional(&pool)
            .await
        }
        .map_err(ise)?
        {
            Some((id,)) => id,
            None => {
                sqlx::query_as::<_, IdRow>(
                    "INSERT INTO scenarios (project_id, name, profile, platform) \
                     VALUES ($1, $2, $3, $4) RETURNING id",
                )
                .bind(project_id)
                .bind(&scenario_name)
                .bind(profile)
                .bind(scenario_platform)
                .fetch_one(&pool)
                .await
                .map_err(ise)?
                .id
            }
        };

        // Upsert dependency graph
        if let Some(graph) = pheromone.get("graph").and_then(|v| v.as_array()) {
            // Clear old graph (CASCADE handles edges)
            sqlx::query("DELETE FROM graph_nodes WHERE scenario_id = $1")
                .bind(scenario_id)
                .execute(&pool)
                .await
                .map_err(ise)?;

            // Bulk-insert all nodes in one query
            let node_names: Vec<&str> = graph
                .iter()
                .filter_map(|e| e.get("name").and_then(|v| v.as_str()))
                .collect();

            let inserted_nodes = sqlx::query_as::<_, IdNameRow>(
                "INSERT INTO graph_nodes (scenario_id, name) \
                 SELECT $1, unnest($2::text[]) RETURNING id, name",
            )
            .bind(scenario_id)
            .bind(&node_names)
            .fetch_all(&pool)
            .await
            .map_err(ise)?;

            let node_ids: HashMap<&str, i64> = inserted_nodes
                .iter()
                .map(|r| (r.name.as_str(), r.id))
                .collect();

            // Collect all edges, then bulk-insert
            let mut source_ids: Vec<i64> = Vec::new();
            let mut target_ids: Vec<i64> = Vec::new();
            for entry in graph {
                let Some(source_name) = entry.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(&src_id) = node_ids.get(source_name) else {
                    continue;
                };
                if let Some(deps) = entry.get("deps").and_then(|v| v.as_array()) {
                    for dep in deps {
                        if let Some(dep_name) = dep.as_str() {
                            if let Some(&tgt_id) = node_ids.get(dep_name) {
                                source_ids.push(src_id);
                                target_ids.push(tgt_id);
                            }
                        }
                    }
                }
            }

            if !source_ids.is_empty() {
                sqlx::query(
                    "INSERT INTO graph_edges (source_id, target_id) \
                     SELECT unnest($1::bigint[]), unnest($2::bigint[]) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(&source_ids)
                .bind(&target_ids)
                .execute(&pool)
                .await
                .map_err(ise)?;
            }
        }

        // Insert run
        let commit_short = event.get("commit").and_then(|v| v.as_str()).unwrap_or("");

        let run_row = sqlx::query_as::<_, IdRow>(
            "INSERT INTO runs (scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(scenario_id)
        .bind(user)
        .bind(run_platform)
        .bind(timestamp)
        .bind(commit_short)
        .bind(duration_ms as i64)
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

        // Bulk-insert dirty crates
        let dirty_crates: Vec<&str> = pheromone
            .get("dirtyCrates")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        if !dirty_crates.is_empty() {
            sqlx::query(
                "INSERT INTO run_dirty_crates (run_id, crate_name) \
                 SELECT $1, unnest($2::text[]) ON CONFLICT DO NOTHING",
            )
            .bind(run_row.id)
            .bind(&dirty_crates)
            .execute(&pool)
            .await
            .map_err(ise)?;
        }
    }

    Ok(StatusCode::OK)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let db_url = db::db_url();
    println!("Connecting to database at {db_url}");
    let pool = db::connect(&db_url)
        .await
        .expect("could not connect to database");

    let app = Router::new()
        .route("/api/projects", get(get_projects).post(create_project))
        .route("/api/projects/{project_id}/overview", get(get_overview))
        .route("/api/projects/{project_id}/scenarios", get(get_scenarios))
        .route(
            "/api/projects/{project_id}/scenarios/{id}",
            get(get_scenario_p),
        )
        .route(
            "/api/projects/{project_id}/scenarios/{id}/pin",
            patch(toggle_pin_p),
        )
        .route("/api/projects/{project_id}/commits", get(get_commits))
        .route(
            "/api/projects/{project_id}/commits/{sha}",
            get(get_commit_p),
        )
        .route("/api/projects/{project_id}/users", get(get_users))
        .route("/api/overview", get(get_overview))
        .route("/api/scenarios", get(get_scenarios))
        .route("/api/scenarios/{id}", get(get_scenario))
        .route("/api/scenarios/{id}/pin", patch(toggle_pin))
        .route("/api/commits", get(get_commits))
        .route("/api/commits/{sha}", get(get_commit))
        .route("/api/users", get(get_users))
        .route("/api/events", post(ingest_events))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    let addr = format!("0.0.0.0:{}", cli.port);
    println!("Burrow listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
