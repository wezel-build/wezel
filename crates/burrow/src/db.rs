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
        CREATE TABLE IF NOT EXISTS projects (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            upstream TEXT NOT NULL UNIQUE
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
            platform TEXT
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
            project_id BIGINT NOT NULL REFERENCES projects(id),
            sha TEXT NOT NULL,
            short_sha TEXT NOT NULL,
            author TEXT NOT NULL,
            message TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            status TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS measurements (
            id BIGSERIAL PRIMARY KEY,
            commit_id BIGINT NOT NULL REFERENCES commits(id),
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            value DOUBLE PRECISION,
            prev_value DOUBLE PRECISION,
            unit TEXT
        );
        CREATE TABLE IF NOT EXISTS measurement_details (
            id BIGSERIAL PRIMARY KEY,
            measurement_id BIGINT NOT NULL REFERENCES measurements(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            value DOUBLE PRECISION NOT NULL,
            prev_value DOUBLE PRECISION NOT NULL
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
            benchmark_name TEXT NOT NULL,
            token TEXT NOT NULL UNIQUE,
            claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            expires_at TIMESTAMPTZ NOT NULL
        );
        ",
    )
    .execute(pool)
    .await?;

    // Incremental migrations for columns added after initial schema.
    sqlx::raw_sql(
        "
        ALTER TABLE graph_nodes ADD COLUMN IF NOT EXISTS version TEXT NOT NULL DEFAULT '';
        ALTER TABLE graph_nodes ADD COLUMN IF NOT EXISTS external BOOLEAN NOT NULL DEFAULT FALSE;
        ALTER TABLE graph_edges DROP CONSTRAINT IF EXISTS graph_edges_pkey;
        ALTER TABLE graph_edges ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'normal';
        ALTER TABLE graph_edges ADD PRIMARY KEY (source_id, target_id, kind);
        ALTER TABLE measurements ADD COLUMN IF NOT EXISTS step TEXT;
        ALTER TABLE observations ADD COLUMN IF NOT EXISTS pheromone_version TEXT;
        DO $$ BEGIN
            IF EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = 'public' AND table_name = 'scenarios'
            ) AND NOT EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = 'public' AND table_name = 'observations'
            ) THEN
                ALTER TABLE scenarios RENAME TO observations;
            END IF;
        END $$;
        DO $$ BEGIN
            IF EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_name = 'forager_tokens' AND column_name = 'scenario_name'
            ) THEN
                ALTER TABLE forager_tokens RENAME COLUMN scenario_name TO benchmark_name;
            END IF;
        END $$;
        ALTER TABLE pheromones ADD COLUMN IF NOT EXISTS viz_json TEXT;
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
