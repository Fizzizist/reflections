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
    related_to_id uuid,
    file_path text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS tag (
    tag_id uuid PRIMARY KEY,
    label text NOT NULL UNIQUE,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS entity_tag (
    entity_tag_id uuid PRIMARY KEY,
    tag_id uuid NOT NULL,
    entity_id uuid NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS entity_tag_unq ON entity_tag(tag_id, entity_id)

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

pub async fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL).await?;
    Ok(())
}
