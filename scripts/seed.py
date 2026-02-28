#!/usr/bin/env python3
"""Seed the burrow SQLite database from JSON fixture files."""

import json
import os
import sqlite3
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = REPO_ROOT / "crates" / "burrow" / "data"

SCHEMA = """\
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
"""


def load_json(path: Path):
    with open(path) as f:
        return json.load(f)


def db_path() -> Path:
    env = os.environ.get("BURROW_DB")
    if env:
        return Path(env)
    home = Path.home() / ".wezel"
    home.mkdir(parents=True, exist_ok=True)
    return home / "burrow.db"


def seed(conn: sqlite3.Connection):
    cur = conn.cursor()

    # Projects
    projects = load_json(DATA_DIR / "projects.json")
    for p in projects:
        cur.execute(
            "INSERT INTO projects (id, name, upstream) VALUES (?, ?, ?)",
            (p["id"], p["name"], p["upstream"]),
        )
    print(f"  Inserted {len(projects)} projects")

    # Users
    users = load_json(DATA_DIR / "users.json")
    for u in users:
        cur.execute("INSERT INTO users (username) VALUES (?)", (u,))
    print(f"  Inserted {len(users)} users")

    # Scenarios, graphs, runs
    scenarios = load_json(DATA_DIR / "scenarios.json")
    for s in scenarios:
        sid = s["id"]
        cur.execute(
            "INSERT INTO scenarios (id, project_id, name, profile, pinned) VALUES (?, ?, ?, ?, ?)",
            (sid, 1, s["name"], s["profile"], int(s.get("pinned", False))),
        )

        idx = min(max(sid, 1), 8)
        graph_json = load_json(DATA_DIR / "graphs" / f"{idx}.json")
        cur.execute(
            "INSERT INTO scenario_graphs (scenario_id, graph_json) VALUES (?, ?)",
            (sid, json.dumps(graph_json)),
        )

        runs = load_json(DATA_DIR / "runs" / f"{idx}.json")
        for r in runs:
            cur.execute(
                "INSERT INTO runs (scenario_id, user, timestamp, commit_short, build_time_ms, dirty_crates_json) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (sid, r["user"], r["timestamp"], r["commit"], r["buildTimeMs"], json.dumps(r["dirtyCrates"])),
            )
        print(f"  Scenario {sid}: inserted graph + {len(runs)} runs")

    # Commits + measurements
    commits = load_json(DATA_DIR / "commits.json")
    for c in commits:
        cur.execute(
            "INSERT INTO commits (project_id, sha, short_sha, author, message, timestamp, status) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (1, c["sha"], c["shortSha"], c["author"], c["message"], c["timestamp"], c["status"]),
        )
        commit_id = cur.lastrowid

        for m in c.get("measurements", []):
            detail = json.dumps(m["detail"]) if isinstance(m.get("detail"), list) else None
            cur.execute(
                "INSERT INTO measurements (id, commit_id, name, kind, status, value, prev_value, unit, detail_json) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (m["id"], commit_id, m["name"], m["kind"], m["status"],
                 m.get("value"), m.get("prevValue"), m.get("unit"), detail),
            )
    print(f"  Inserted {len(commits)} commits with measurements")

    conn.commit()


def main():
    path = db_path()
    print(f"Seeding database at {path}")

    if path.exists():
        path.unlink()
        print("Removed existing database")

    conn = sqlite3.connect(str(path))
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    conn.executescript(SCHEMA)

    try:
        seed(conn)
    except Exception:
        conn.close()
        raise

    conn.close()
    print(f"Done! Database seeded at {path}")


if __name__ == "__main__":
    main()