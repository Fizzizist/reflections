use anyhow::Result;
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
    about_id uuid NOT NULL,
    file_path text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS meeting (
    meeting_id uuid PRIMARY KEY,
    name text NOT NULL,
    scheduled_at timestamp NOT NULL DEFAULT '1970-01-01T00:00:00Z',
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
";

const MIGRATIONS: &[&str] = &[
    "ALTER TABLE meeting ADD COLUMN scheduled_at timestamp NOT NULL DEFAULT '1970-01-01T00:00:00Z'",
];

pub async fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL).await?;
    Ok(())
}

pub async fn run_migrations(conn: &Connection) -> Result<()> {
    for migration in MIGRATIONS {
        if let Err(e) = conn.execute_batch(migration).await {
            let err_msg = e.to_string().to_lowercase();
            if err_msg.contains("duplicate column name") {
                continue;
            }
            return Err(e.into());
        }
    }
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
}
