use anyhow::Result;
use chrono::Utc;
use turso::Connection;

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS todo_item (
    todo_item_id uuid PRIMARY KEY,
    label text NOT NULL,
    status text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS event (
    event_id uuid PRIMARY KEY,
    entity_id uuid NOT NULL,
    event_type text NOT NULL,
    metadata jsonb NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS reflection (
    reflection_id uuid PRIMARY KEY,
    about_id uuid,
    file_path text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS meeting (
    meeting_id uuid PRIMARY KEY,
    name text NOT NULL,
    scheduled_at timestamp NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS note (
    note_id uuid PRIMARY KEY,
    related_to_id uuid NOT NULL,
    file_path text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS tag (
    tag_id uuid PRIMARY KEY,
    entity_id uuid NOT NULL,
    label text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS summary (
    summary_id uuid PRIMARY KEY,
    file_path text NOT NULL,
    start timestamp NOT NULL,
    \"end\" timestamp NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS schema_versions (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
) STRICT;
";

const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        "ALTER TABLE meeting ADD COLUMN scheduled_at timestamp NOT NULL DEFAULT '1970-01-01T00:00:00Z'",
    ),
    (
        2,
        "CREATE TABLE IF NOT EXISTS reflection_new (
            reflection_id uuid PRIMARY KEY,
            about_id uuid,
            file_path text NOT NULL,
            created_at timestamp NOT NULL,
            updated_at timestamp NOT NULL
        ) STRICT;
        INSERT INTO reflection_new (reflection_id, about_id, file_path, created_at, updated_at)
            SELECT reflection_id, about_id, file_path, created_at, updated_at FROM reflection;
        DROP TABLE reflection;
        ALTER TABLE reflection_new RENAME TO reflection;",
    ),
];

pub async fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL).await?;
    Ok(())
}

pub async fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_versions (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)").await?;

    let mut rows = conn
        .query("SELECT COUNT(*) FROM schema_versions", ())
        .await?;
    let row = rows.next().await?.expect("schema_versions count row");
    let count: i64 = *row
        .get_value(0)
        .expect("count value")
        .as_integer()
        .expect("expected integer");
    drop(rows);

    if count == 0 && meeting_has_scheduled_at(conn).await? {
        // Fresh database: init_schema already created tables with current DDL.
        // All migrations are baked in — record them as applied and skip.
        for (version, _) in MIGRATIONS {
            record_migration(conn, *version).await?;
        }
        return Ok(());
    }

    for (version, sql) in MIGRATIONS {
        let mut rows = conn
            .query(
                "SELECT version FROM schema_versions WHERE version = ?",
                [*version],
            )
            .await?;
        let already_applied = rows.next().await?.is_some();
        drop(rows);

        if !already_applied {
            conn.execute_batch(sql).await?;
            record_migration(conn, *version).await?;
        }
    }
    Ok(())
}

async fn meeting_has_scheduled_at(conn: &Connection) -> Result<bool> {
    let mut rows = conn.query("PRAGMA table_info(meeting)", ()).await?;
    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        if name == "scheduled_at" {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn record_migration(conn: &Connection, version: i64) -> Result<()> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut rows = conn
        .query(
            "INSERT INTO schema_versions (version, applied_at) VALUES (?, ?)",
            (version, now),
        )
        .await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use turso::Builder;

    async fn test_conn() -> Connection {
        let db = Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("failed to build in-memory DB");
        db.connect().expect("failed to connect to in-memory DB")
    }

    #[tokio::test]
    async fn init_schema_creates_all_tables() {
        let conn = test_conn().await;
        init_schema(&conn).await.expect("schema init failed");

        let expected_tables = [
            "todo_item",
            "event",
            "reflection",
            "meeting",
            "note",
            "tag",
            "summary",
            "schema_versions",
        ];

        for table in &expected_tables {
            let mut rows = conn
                .query(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
                    [*table],
                )
                .await
                .expect("query failed");

            let row = rows
                .next()
                .await
                .expect("row fetch failed")
                .expect("table not found");

            let name: String = row
                .get_value(0)
                .expect("value extraction failed")
                .as_text()
                .expect("expected text value")
                .to_string();
            assert_eq!(name, *table);
        }
    }

    #[tokio::test]
    async fn migration_adds_scheduled_at_column() {
        let conn = test_conn().await;

        let old_schema = "
CREATE TABLE IF NOT EXISTS meeting (
    meeting_id uuid PRIMARY KEY,
    name text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS reflection (
    reflection_id uuid PRIMARY KEY,
    about_id uuid NOT NULL,
    file_path text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;
";
        conn.execute_batch(old_schema)
            .await
            .expect("failed to create old schema");

        run_migrations(&conn).await.expect("migration failed");

        let mut rows = conn
            .query("PRAGMA table_info(meeting)", ())
            .await
            .expect("query failed");

        let mut found_scheduled_at = false;
        while let Some(row) = rows.next().await.expect("row fetch failed") {
            let name: String = row
                .get_value(1)
                .expect("value extraction failed")
                .as_text()
                .expect("expected text value")
                .to_string();
            if name == "scheduled_at" {
                found_scheduled_at = true;
                break;
            }
        }

        assert!(
            found_scheduled_at,
            "scheduled_at column not found after migration"
        );
    }

    #[tokio::test]
    async fn migrations_idempotent() {
        let conn = test_conn().await;
        init_schema(&conn).await.expect("schema init failed");

        run_migrations(&conn)
            .await
            .expect("first migration run failed");
        run_migrations(&conn)
            .await
            .expect("second migration run failed");
    }

    #[tokio::test]
    async fn migrations_skip_already_applied() {
        let conn = test_conn().await;
        init_schema(&conn).await.expect("schema init failed");

        run_migrations(&conn)
            .await
            .expect("first migration run failed");

        let mut rows = conn
            .query("SELECT COUNT(*) FROM schema_versions", ())
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let count: i64 = *row
            .get_value(0)
            .expect("get failed")
            .as_integer()
            .expect("expected int");
        drop(rows);
        assert_eq!(count, 2, "should have exactly 2 migrations recorded");

        run_migrations(&conn)
            .await
            .expect("second migration run failed");

        let mut rows = conn
            .query("SELECT COUNT(*) FROM schema_versions", ())
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let count: i64 = *row
            .get_value(0)
            .expect("get failed")
            .as_integer()
            .expect("expected int");
        assert_eq!(count, 2, "should still have exactly 2 migrations recorded");
    }

    #[tokio::test]
    async fn migration_makes_about_id_nullable() {
        let conn = test_conn().await;

        let old_schema = "
CREATE TABLE IF NOT EXISTS meeting (
    meeting_id uuid PRIMARY KEY,
    name text NOT NULL,
    scheduled_at timestamp NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS reflection (
    reflection_id uuid PRIMARY KEY,
    about_id uuid NOT NULL,
    file_path text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;
";
        conn.execute_batch(old_schema)
            .await
            .expect("failed to create old schema");

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_versions (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
        )
        .await
        .expect("create schema_versions");

        // Migration 1 already applied in this old DB
        record_migration(&conn, 1)
            .await
            .expect("record migration 1");

        conn.execute_batch(
            "INSERT INTO reflection (reflection_id, about_id, file_path, created_at, updated_at) VALUES ('019598a0-0000-7000-8000-000000000001', '019598a0-0000-7000-8000-000000000002', '2024/01/15/test.md', '2024-01-15T10:30:00Z', '2024-01-15T10:30:00Z')",
        )
        .await
        .expect("insert with NOT NULL about_id failed");

        run_migrations(&conn).await.expect("migration failed");

        conn.execute_batch(
            "INSERT INTO reflection (reflection_id, about_id, file_path, created_at, updated_at) VALUES ('019598a0-0000-7000-8000-000000000003', NULL, '2024/01/15/test2.md', '2024-01-15T11:00:00Z', '2024-01-15T11:00:00Z')",
        )
        .await
        .expect("insert with NULL about_id failed after migration");

        let mut rows = conn
            .query("SELECT COUNT(*) FROM reflection WHERE about_id IS NULL", ())
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let count: i64 = *row
            .get_value(0)
            .expect("get failed")
            .as_integer()
            .expect("expected int");
        assert_eq!(count, 1, "should have one row with NULL about_id");
    }
}
