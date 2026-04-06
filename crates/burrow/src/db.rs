use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn connect(url: &str) -> sqlx::Result<PgPool> {
    let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::raw_sql(
        "
        CREATE TABLE IF NOT EXISTS repos (
            id BIGSERIAL PRIMARY KEY,
            upstream TEXT NOT NULL UNIQUE,
            webhook_secret TEXT,
            webhook_registered BOOLEAN NOT NULL DEFAULT FALSE,
            enqueue_interval_secs INT NOT NULL DEFAULT 3600
        );
        CREATE TABLE IF NOT EXISTS projects (
            id BIGSERIAL PRIMARY KEY,
            repo_id BIGINT NOT NULL REFERENCES repos(id),
            name TEXT NOT NULL,
            subdir TEXT NOT NULL DEFAULT '',
            upstream TEXT NOT NULL,
            UNIQUE(repo_id, name)
        );
        CREATE TABLE IF NOT EXISTS users (
            id BIGSERIAL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS observations (
            id BIGSERIAL PRIMARY KEY,
            project_id BIGINT NOT NULL REFERENCES projects(id),
            name TEXT NOT NULL,
            profile TEXT NOT NULL CHECK(profile IN ('dev', 'release')),
            pinned BOOLEAN NOT NULL DEFAULT FALSE,
            platform TEXT,
            pheromone_version TEXT
        );
        CREATE TABLE IF NOT EXISTS graph_nodes (
            id BIGSERIAL PRIMARY KEY,
            scenario_id BIGINT NOT NULL REFERENCES observations(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            version TEXT NOT NULL DEFAULT '',
            external BOOLEAN NOT NULL DEFAULT FALSE,
            UNIQUE(scenario_id, name)
        );
        CREATE TABLE IF NOT EXISTS graph_edges (
            source_id BIGINT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
            target_id BIGINT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
            kind TEXT NOT NULL DEFAULT 'normal' CHECK(kind IN ('normal', 'build', 'dev')),
            PRIMARY KEY (source_id, target_id, kind)
        );
        CREATE TABLE IF NOT EXISTS runs (
            id BIGSERIAL PRIMARY KEY,
            scenario_id BIGINT NOT NULL REFERENCES observations(id),
            \"user\" TEXT NOT NULL,
            platform TEXT NOT NULL DEFAULT '',
            timestamp TEXT NOT NULL,
            commit_short TEXT NOT NULL,
            build_time_ms BIGINT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS run_dirty_crates (
            run_id BIGINT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
            crate_name TEXT NOT NULL,
            PRIMARY KEY (run_id, crate_name)
        );
        CREATE TABLE IF NOT EXISTS commits (
            id BIGSERIAL PRIMARY KEY,
            repo_id BIGINT NOT NULL REFERENCES repos(id),
            sha TEXT NOT NULL,
            short_sha TEXT NOT NULL,
            parent_sha TEXT,
            author TEXT NOT NULL DEFAULT '',
            message TEXT NOT NULL DEFAULT '',
            timestamp TEXT NOT NULL DEFAULT '',
            UNIQUE(repo_id, sha)
        );
        CREATE TABLE IF NOT EXISTS measurements (
            id BIGSERIAL PRIMARY KEY,
            commit_id BIGINT NOT NULL REFERENCES commits(id),
            project_id BIGINT NOT NULL REFERENCES projects(id),
            name TEXT NOT NULL,
            status TEXT NOT NULL,
            value DOUBLE PRECISION,
            unit TEXT,
            step TEXT
        );
        CREATE TABLE IF NOT EXISTS measurement_details (
            id BIGSERIAL PRIMARY KEY,
            measurement_id BIGINT NOT NULL REFERENCES measurements(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            value DOUBLE PRECISION NOT NULL
        );
        CREATE TABLE IF NOT EXISTS measurement_tags (
            measurement_id BIGINT NOT NULL REFERENCES measurements(id) ON DELETE CASCADE,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            identity BOOLEAN NOT NULL DEFAULT false,
            PRIMARY KEY (measurement_id, key)
        );
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            github_login TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS pheromones (
            id          BIGSERIAL PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            github_repo TEXT NOT NULL,
            version     TEXT NOT NULL,
            schema_json TEXT NOT NULL,
            viz_json    TEXT,
            fetched_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        );
        CREATE TABLE IF NOT EXISTS pheromone_schema_history (
            id            BIGSERIAL PRIMARY KEY,
            pheromone_id  BIGINT NOT NULL REFERENCES pheromones(id),
            version       TEXT NOT NULL,
            schema_json   TEXT NOT NULL,
            fetched_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (pheromone_id, version)
        );
        CREATE TABLE IF NOT EXISTS forager_tokens (
            id BIGSERIAL PRIMARY KEY,
            commit_id BIGINT NOT NULL REFERENCES commits(id),
            experiment_name TEXT NOT NULL,
            token TEXT NOT NULL UNIQUE,
            claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            expires_at TIMESTAMPTZ NOT NULL
        );
        CREATE TABLE IF NOT EXISTS branches (
            id BIGSERIAL PRIMARY KEY,
            repo_id BIGINT NOT NULL REFERENCES repos(id),
            name TEXT NOT NULL,
            head_sha TEXT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE(repo_id, name)
        );
        CREATE TABLE IF NOT EXISTS bisections (
            id                BIGSERIAL PRIMARY KEY,
            project_id        BIGINT NOT NULL REFERENCES projects(id),
            experiment_name    TEXT NOT NULL,
            measurement_name  TEXT NOT NULL,
            branch            TEXT NOT NULL,
            good_sha          TEXT NOT NULL,
            bad_sha           TEXT NOT NULL,
            good_value        DOUBLE PRECISION NOT NULL,
            bad_value         DOUBLE PRECISION NOT NULL,
            status            TEXT NOT NULL DEFAULT 'active'
                              CHECK(status IN ('active', 'complete', 'abandoned')),
            culprit_sha       TEXT,
            identity_tags     JSONB NOT NULL DEFAULT '{}',
            created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
            completed_at      TIMESTAMPTZ
        );
        CREATE TABLE IF NOT EXISTS forager_queue (
            id             BIGSERIAL PRIMARY KEY,
            project_id     BIGINT NOT NULL REFERENCES projects(id),
            commit_sha     TEXT NOT NULL,
            experiment_name TEXT NOT NULL,
            branch         TEXT NOT NULL DEFAULT 'main',
            bisection_id   BIGINT REFERENCES bisections(id),
            status         TEXT NOT NULL DEFAULT 'pending'
                           CHECK(status IN ('pending', 'running', 'complete', 'failed')),
            claimed_at     TIMESTAMPTZ,
            completed_at   TIMESTAMPTZ,
            error_text     TEXT,
            created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
        );
        CREATE TABLE IF NOT EXISTS bisection_measurements (
            bisection_id   BIGINT NOT NULL REFERENCES bisections(id),
            measurement_id BIGINT NOT NULL REFERENCES measurements(id),
            PRIMARY KEY (bisection_id, measurement_id)
        );
        ",
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn create_session(pool: &PgPool, session_id: &str, login: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO sessions (id, github_login) VALUES ($1, $2)")
        .bind(session_id)
        .bind(login)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_session(pool: &PgPool, session_id: &str) -> sqlx::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT github_login FROM sessions WHERE id = $1")
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(login,)| login))
}

pub async fn delete_session(pool: &PgPool, session_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .or_else(|_| std::env::var("BURROW_DB"))
        .unwrap_or_else(|_| "postgres://localhost/burrow".to_string())
}
