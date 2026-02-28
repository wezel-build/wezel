mod db;

use serde_json::Value;

static SCENARIOS_JSON: &str = include_str!("../data/scenarios.json");
static COMMITS_JSON: &str = include_str!("../data/commits.json");
static USERS_JSON: &str = include_str!("../data/users.json");
static PROJECTS_JSON: &str = include_str!("../data/projects.json");

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

async fn seed(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    // Seed projects
    let projects: Vec<Value> = serde_json::from_str(PROJECTS_JSON).unwrap();
    for p in &projects {
        sqlx::query("INSERT INTO projects (id, name, upstream) VALUES (?, ?, ?)")
            .bind(p["id"].as_i64().unwrap())
            .bind(p["name"].as_str().unwrap())
            .bind(p["upstream"].as_str().unwrap())
            .execute(pool)
            .await?;
    }
    println!("  Inserted {} projects", projects.len());

    // Seed users
    let users: Vec<String> = serde_json::from_str(USERS_JSON).unwrap();
    for u in &users {
        sqlx::query("INSERT INTO users (username) VALUES (?)")
            .bind(u)
            .execute(pool)
            .await?;
    }
    println!("  Inserted {} users", users.len());

    // Seed scenarios (all assigned to project 1)
    let scenarios: Vec<Value> = serde_json::from_str(SCENARIOS_JSON).unwrap();
    for s in &scenarios {
        let id = s["id"].as_i64().unwrap();
        sqlx::query(
            "INSERT INTO scenarios (id, project_id, name, profile, pinned) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(1i64)
        .bind(s["name"].as_str().unwrap())
        .bind(s["profile"].as_str().unwrap())
        .bind(s["pinned"].as_bool().unwrap_or(false))
        .execute(pool)
        .await?;

        // Insert graph
        let idx = (id as usize).saturating_sub(1).min(GRAPHS.len() - 1);
        sqlx::query("INSERT INTO scenario_graphs (scenario_id, graph_json) VALUES (?, ?)")
            .bind(id)
            .bind(GRAPHS[idx])
            .execute(pool)
            .await?;

        // Insert runs
        let runs: Vec<Value> = serde_json::from_str(RUNS[idx]).unwrap();
        for r in &runs {
            sqlx::query(
                "INSERT INTO runs (scenario_id, user, timestamp, commit_short, build_time_ms, dirty_crates_json) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(r["user"].as_str().unwrap())
            .bind(r["timestamp"].as_str().unwrap())
            .bind(r["commit"].as_str().unwrap())
            .bind(r["buildTimeMs"].as_i64().unwrap())
            .bind(serde_json::to_string(&r["dirtyCrates"]).unwrap())
            .execute(pool)
            .await?;
        }
        println!("  Scenario {id}: inserted graph + {} runs", runs.len());
    }

    // Seed commits (assigned to project 1)
    let commits: Vec<Value> = serde_json::from_str(COMMITS_JSON).unwrap();
    for c in &commits {
        let result = sqlx::query(
            "INSERT INTO commits (project_id, sha, short_sha, author, message, timestamp, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(1i64)
        .bind(c["sha"].as_str().unwrap())
        .bind(c["shortSha"].as_str().unwrap())
        .bind(c["author"].as_str().unwrap())
        .bind(c["message"].as_str().unwrap())
        .bind(c["timestamp"].as_str().unwrap())
        .bind(c["status"].as_str().unwrap())
        .execute(pool)
        .await?;

        let commit_id = result.last_insert_rowid();

        if let Some(measurements) = c["measurements"].as_array() {
            for m in measurements {
                let detail = if m["detail"].is_array() {
                    Some(serde_json::to_string(&m["detail"]).unwrap())
                } else {
                    None
                };
                sqlx::query(
                    "INSERT INTO measurements (id, commit_id, name, kind, status, value, prev_value, unit, detail_json) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(m["id"].as_i64().unwrap())
                .bind(commit_id)
                .bind(m["name"].as_str().unwrap())
                .bind(m["kind"].as_str().unwrap())
                .bind(m["status"].as_str().unwrap())
                .bind(m["value"].as_f64())
                .bind(m["prevValue"].as_f64())
                .bind(m["unit"].as_str())
                .bind(detail.as_deref())
                .execute(pool)
                .await?;
            }
        }
    }
    println!("  Inserted {} commits with measurements", commits.len());

    Ok(())
}

#[tokio::main]
async fn main() {
    let url = db::db_url();
    let file_path = url.strip_prefix("sqlite:").unwrap_or(&url);
    println!("Seeding database at {file_path}");

    if std::path::Path::new(file_path).exists() {
        std::fs::remove_file(file_path).expect("could not remove existing DB");
        println!("Removed existing database");
    }

    let pool = db::open(&url).await.expect("could not open database");
    seed(&pool).await.expect("seeding failed");
    pool.close().await;

    println!("Done! Database seeded at {file_path}");
}
