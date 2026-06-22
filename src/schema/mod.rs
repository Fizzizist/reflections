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

pub async fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL).await?;
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
}
