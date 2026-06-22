use anyhow::Result;
use chrono::{DateTime, Utc};
use turso::transaction::Transaction;
use uuid::Uuid;

use crate::models::event::EventType;

pub struct EventRepository {
    #[allow(dead_code)]
    conn: turso::Connection,
}

impl EventRepository {
    pub fn new(conn: turso::Connection) -> Self {
        Self { conn }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert(
        &self,
        tx: &Transaction<'_>,
        event_id: Uuid,
        entity_id: Uuid,
        event_type: &EventType,
        metadata: &str,
        created_at: &DateTime<Utc>,
        updated_at: &DateTime<Utc>,
    ) -> Result<()> {
        let sql = "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)";
        tx.execute(
            sql,
            (
                event_id.to_string(),
                entity_id.to_string(),
                event_type.to_string(),
                metadata.to_string(),
                created_at.to_rfc3339(),
                updated_at.to_rfc3339(),
            ),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    async fn setup() -> (turso::Connection, EventRepository) {
        let db = turso::Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        let repo = EventRepository::new(conn.clone());
        (conn, repo)
    }

    #[tokio::test]
    async fn insert_event() {
        let (mut conn, repo) = setup().await;
        let now = Utc::now();
        let entity_id = Uuid::now_v7();
        let event_id = Uuid::now_v7();

        let tx = conn.transaction().await.expect("tx begin failed");
        repo.insert(
            &tx,
            event_id,
            entity_id,
            &EventType::TodoItemCreated,
            "{}",
            &now,
            &now,
        )
        .await
        .expect("insert failed");
        tx.commit().await.expect("commit failed");

        let mut rows = conn
            .query(
                "SELECT event_id, entity_id, event_type FROM event WHERE entity_id = ?",
                [entity_id.to_string()],
            )
            .await
            .expect("query failed");

        let row = rows
            .next()
            .await
            .expect("row fetch failed")
            .expect("event not found");

        let event_type_str = row
            .get_value(2)
            .expect("value extraction failed")
            .as_text()
            .expect("expected text")
            .to_string();
        assert_eq!(event_type_str, "TODO_ITEM_CREATED");
    }
}
