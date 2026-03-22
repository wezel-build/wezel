use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::Value;
use sqlx::PgPool;

use crate::models::*;
use crate::{ApiResult, ise};

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn build_graph(pool: &PgPool, scenario_id: i64) -> ApiResult<Vec<GraphNodeJson>> {
    let nodes = sqlx::query_as::<_, GraphNodeRow>(
        "SELECT name, version, external FROM graph_nodes WHERE scenario_id = $1 ORDER BY name",
    )
    .bind(scenario_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let edges = sqlx::query_as::<_, GraphEdgeRow>(
        "SELECT src.name AS source_name, tgt.name AS dep_name, ge.kind \
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
    let mut build_deps_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut dev_deps_map: HashMap<String, Vec<String>> = HashMap::new();
    for node in &nodes {
        deps_map.entry(node.name.clone()).or_default();
        build_deps_map.entry(node.name.clone()).or_default();
        dev_deps_map.entry(node.name.clone()).or_default();
    }
    for edge in edges {
        let map = match edge.kind.as_str() {
            "build" => &mut build_deps_map,
            "dev" => &mut dev_deps_map,
            _ => &mut deps_map,
        };
        map.entry(edge.source_name).or_default().push(edge.dep_name);
    }

    let graph: Vec<GraphNodeJson> = nodes
        .into_iter()
        .map(|n| {
            let deps = deps_map.remove(&n.name).unwrap_or_default();
            let build_deps = build_deps_map.remove(&n.name).unwrap_or_default();
            let dev_deps = dev_deps_map.remove(&n.name).unwrap_or_default();
            GraphNodeJson {
                name: n.name,
                version: n.version,
                deps,
                build_deps,
                dev_deps,
                external: n.external,
            }
        })
        .collect();

    Ok(graph)
}

async fn observation_to_json(
    pool: &PgPool,
    id: i64,
    include_graph: bool,
) -> ApiResult<Option<ObservationJson>> {
    let Some(s) = sqlx::query_as::<_, Observation>(
        "SELECT id, name, profile, pinned, platform FROM observations WHERE id = $1",
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

    Ok(Some(ObservationJson {
        id: s.id,
        name: s.name,
        profile: s.profile,
        pinned: s.pinned,
        platform: s.platform,
        runs: runs_json,
        graph,
    }))
}

/// Fetch all observations (optionally scoped to a project) with their runs.
async fn fetch_observations(
    pool: &PgPool,
    project_id: Option<i64>,
) -> ApiResult<Vec<ObservationJson>> {
    let scenarios: Vec<Observation> = if let Some(pid) = project_id {
        sqlx::query_as::<_, Observation>(
            "SELECT id, name, profile, pinned, platform FROM observations \
             WHERE project_id = $1 ORDER BY id",
        )
        .bind(pid)
        .fetch_all(pool)
        .await
        .map_err(ise)?
    } else {
        sqlx::query_as::<_, Observation>(
            "SELECT id, name, profile, pinned, platform FROM observations ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .map_err(ise)?
    };

    if scenarios.is_empty() {
        return Ok(vec![]);
    }

    let scenario_ids: Vec<i64> = scenarios.iter().map(|s| s.id).collect();

    let runs = sqlx::query_as::<_, Run>(
        "SELECT id, scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms \
         FROM runs WHERE scenario_id = ANY($1) ORDER BY timestamp",
    )
    .bind(&scenario_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();

    let dirty_crates = sqlx::query_as::<_, DirtyCrate>(
        "SELECT run_id, crate_name FROM run_dirty_crates \
         WHERE run_id = ANY($1) ORDER BY crate_name",
    )
    .bind(&run_ids)
    .fetch_all(pool)
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

    let out = scenarios
        .into_iter()
        .map(|s| ObservationJson {
            id: s.id,
            name: s.name,
            profile: s.profile,
            pinned: s.pinned,
            platform: s.platform,
            runs: runs_by_scenario.remove(&s.id).unwrap_or_default(),
            graph: None,
        })
        .collect();

    Ok(out)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn get_observations(State(pool): State<PgPool>) -> ApiResult<Json<Vec<ObservationJson>>> {
    Ok(Json(fetch_observations(&pool, None).await?))
}

pub async fn get_project_observations(
    Path((project_id,)): Path<(i64,)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<ObservationJson>>> {
    Ok(Json(fetch_observations(&pool, Some(project_id)).await?))
}

pub async fn get_observation(
    Path(id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    observation_to_json(&pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn get_project_observation(
    Path((_pid, id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    observation_to_json(&pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn toggle_observation_pin(
    Path(id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    toggle_observation_pin_inner(id, &pool).await
}

pub async fn toggle_project_observation_pin(
    Path((_pid, id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    toggle_observation_pin_inner(id, &pool).await
}

async fn toggle_observation_pin_inner(id: i64, pool: &PgPool) -> ApiResult<Json<ObservationJson>> {
    let result = sqlx::query("UPDATE observations SET pinned = NOT pinned WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(ise)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    observation_to_json(pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn ingest_events(
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

        // Find or create project.
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

        // Ensure user exists.
        sqlx::query("INSERT INTO users (username) VALUES ($1) ON CONFLICT (username) DO NOTHING")
            .bind(user)
            .execute(&pool)
            .await
            .map_err(ise)?;

        // Process pheromone data if present.
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

        let benchmark_name = if packages.is_empty() {
            format!("{tool} {command}")
        } else {
            format!("{tool} {command} {}", packages.join(" "))
        };

        // Find or create scenario.
        let scenario_id: i64 = match if let Some(sp) = scenario_platform {
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM observations \
                 WHERE project_id = $1 AND name = $2 AND profile = $3 AND platform = $4",
            )
            .bind(project_id)
            .bind(&benchmark_name)
            .bind(profile)
            .bind(sp)
            .fetch_optional(&pool)
            .await
        } else {
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM observations \
                 WHERE project_id = $1 AND name = $2 AND profile = $3 AND platform IS NULL",
            )
            .bind(project_id)
            .bind(&benchmark_name)
            .bind(profile)
            .fetch_optional(&pool)
            .await
        }
        .map_err(ise)?
        {
            Some((id,)) => id,
            None => {
                sqlx::query_as::<_, IdRow>(
                    "INSERT INTO observations (project_id, name, profile, platform) \
                     VALUES ($1, $2, $3, $4) RETURNING id",
                )
                .bind(project_id)
                .bind(&benchmark_name)
                .bind(profile)
                .bind(scenario_platform)
                .fetch_one(&pool)
                .await
                .map_err(ise)?
                .id
            }
        };

        // Upsert dependency graph.
        if let Some(graph) = pheromone.get("graph").and_then(|v| v.as_array()) {
            // Clear old graph (CASCADE handles edges).
            sqlx::query("DELETE FROM graph_nodes WHERE scenario_id = $1")
                .bind(scenario_id)
                .execute(&pool)
                .await
                .map_err(ise)?;

            // Bulk-insert all nodes.
            let mut node_names: Vec<&str> = Vec::new();
            let mut node_versions: Vec<&str> = Vec::new();
            let mut node_externals: Vec<bool> = Vec::new();
            for e in graph {
                if let Some(name) = e.get("name").and_then(|v| v.as_str()) {
                    node_names.push(name);
                    node_versions.push(e.get("version").and_then(|v| v.as_str()).unwrap_or(""));
                    node_externals
                        .push(e.get("external").and_then(|v| v.as_bool()).unwrap_or(false));
                }
            }

            let inserted_nodes = sqlx::query_as::<_, IdNameRow>(
                "INSERT INTO graph_nodes (scenario_id, name, version, external) \
                 SELECT $1, unnest($2::text[]), unnest($3::text[]), unnest($4::bool[]) \
                 RETURNING id, name",
            )
            .bind(scenario_id)
            .bind(&node_names)
            .bind(&node_versions)
            .bind(&node_externals)
            .fetch_all(&pool)
            .await
            .map_err(ise)?;

            let node_ids: HashMap<&str, i64> = inserted_nodes
                .iter()
                .map(|r| (r.name.as_str(), r.id))
                .collect();

            // Collect all edges with their kind, then bulk-insert.
            let mut source_ids: Vec<i64> = Vec::new();
            let mut target_ids: Vec<i64> = Vec::new();
            let mut edge_kinds: Vec<&str> = Vec::new();

            let push_edges = |deps: Option<&serde_json::Value>,
                              kind: &'static str,
                              src_id: i64,
                              node_ids: &HashMap<&str, i64>,
                              source_ids: &mut Vec<i64>,
                              target_ids: &mut Vec<i64>,
                              edge_kinds: &mut Vec<&str>| {
                if let Some(arr) = deps.and_then(|v| v.as_array()) {
                    for dep in arr {
                        if let Some(dep_name) = dep.as_str()
                            && let Some(&tgt_id) = node_ids.get(dep_name)
                        {
                            source_ids.push(src_id);
                            target_ids.push(tgt_id);
                            edge_kinds.push(kind);
                        }
                    }
                }
            };

            for entry in graph {
                let Some(source_name) = entry.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(&src_id) = node_ids.get(source_name) else {
                    continue;
                };
                push_edges(
                    entry.get("deps"),
                    "normal",
                    src_id,
                    &node_ids,
                    &mut source_ids,
                    &mut target_ids,
                    &mut edge_kinds,
                );
                push_edges(
                    entry.get("buildDeps"),
                    "build",
                    src_id,
                    &node_ids,
                    &mut source_ids,
                    &mut target_ids,
                    &mut edge_kinds,
                );
                push_edges(
                    entry.get("devDeps"),
                    "dev",
                    src_id,
                    &node_ids,
                    &mut source_ids,
                    &mut target_ids,
                    &mut edge_kinds,
                );
            }

            if !source_ids.is_empty() {
                sqlx::query(
                    "INSERT INTO graph_edges (source_id, target_id, kind) \
                     SELECT unnest($1::bigint[]), unnest($2::bigint[]), unnest($3::text[]) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(&source_ids)
                .bind(&target_ids)
                .bind(&edge_kinds)
                .execute(&pool)
                .await
                .map_err(ise)?;
            }
        }

        // Insert run.
        let commit_short = event.get("commit").and_then(|v| v.as_str()).unwrap_or("");

        let run_row = sqlx::query_as::<_, IdRow>(
            "INSERT INTO runs \
             (scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms) \
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

        // Bulk-insert dirty crates.
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
