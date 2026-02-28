use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

pub async fn open(path: &str) -> sqlx::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(path)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::raw_sql(
        "
        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            upstream TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS scenarios (
            id INTEGER PRIMARY KEY,
            project_id INTEGER NOT NULL REFERENCES projects(id),
            name TEXT NOT NULL,
            profile TEXT NOT NULL CHECK(profile IN ('dev', 'release')),
            pinned INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS scenario_graphs (
            scenario_id INTEGER PRIMARY KEY REFERENCES scenarios(id),
            graph_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scenario_id INTEGER NOT NULL REFERENCES scenarios(id),
            user TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            commit_short TEXT NOT NULL,
            build_time_ms INTEGER NOT NULL,
            dirty_crates_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS commits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id),
            sha TEXT NOT NULL,
            short_sha TEXT NOT NULL,
            author TEXT NOT NULL,
            message TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            status TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS measurements (
            id INTEGER PRIMARY KEY,
            commit_id INTEGER NOT NULL REFERENCES commits(id),
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            value REAL,
            prev_value REAL,
            unit TEXT,
            detail_json TEXT
        );
        ",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub fn db_url() -> String {
    if let Ok(p) = std::env::var("BURROW_DB") {
        format!("sqlite:{p}")
    } else {
        let home = dirs::home_dir().expect("could not determine home directory");
        let dir = home.join(".wezel");
        std::fs::create_dir_all(&dir).expect("could not create ~/.wezel");
        let path = dir.join("burrow.db");
        format!("sqlite:{}", path.display())
    }
}
