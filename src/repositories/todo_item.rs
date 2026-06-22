use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use turso::{Connection, transaction::Transaction};
use uuid::Uuid;

use crate::models::todo_item::{TodoItem, TodoStatus};
use std::str::FromStr;

fn parse_timestamp(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")?;
    Ok(Utc.from_utc_datetime(&naive))
}

pub struct TodoItemRepository {
    conn: Connection,
}

impl TodoItemRepository {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub async fn insert(
        &self,
        tx: &Transaction<'_>,
        id: Uuid,
        label: &str,
        status: &TodoStatus,
        created_at: &DateTime<Utc>,
        updated_at: &DateTime<Utc>,
    ) -> Result<()> {
        let sql = "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?)";
        tx.execute(
            sql,
            (
                id.to_string(),
                label.to_string(),
                status.to_string(),
                created_at.to_rfc3339(),
                updated_at.to_rfc3339(),
            ),
        )
        .await?;
        Ok(())
    }

    pub async fn list_active(&self) -> Result<Vec<TodoItem>> {
        let sql = "SELECT todo_item_id, label, status, created_at, updated_at FROM todo_item WHERE status != 'Done' ORDER BY created_at ASC";
        let mut rows = self.conn.query(sql, ()).await?;

        let mut items = Vec::new();
        while let Some(row) = rows.next().await? {
            let id_str = row
                .get_value(0)?
                .as_text()
                .ok_or_else(|| anyhow::anyhow!("todo_item_id is not text"))?
                .to_string();
            let id = Uuid::parse_str(&id_str)?;
            let label = row
                .get_value(1)?
                .as_text()
                .ok_or_else(|| anyhow::anyhow!("label is not text"))?
                .to_string();
            let status_str = row
                .get_value(2)?
                .as_text()
                .ok_or_else(|| anyhow::anyhow!("status is not text"))?
                .to_string();
            let status = TodoStatus::from_str(&status_str)?;
            let created_str = row
                .get_value(3)?
                .as_text()
                .ok_or_else(|| anyhow::anyhow!("created_at is not text"))?
                .to_string();
            let created_at = parse_timestamp(&created_str)?;
            let updated_str = row
                .get_value(4)?
                .as_text()
                .ok_or_else(|| anyhow::anyhow!("updated_at is not text"))?
                .to_string();
            let updated_at = parse_timestamp(&updated_str)?;

            items.push(TodoItem {
                id,
                label,
                status,
                created_at,
                updated_at,
            });
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    async fn setup() -> (Connection, TodoItemRepository) {
        let db = turso::Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        let repo = TodoItemRepository::new(conn.clone());
        (conn, repo)
    }

    #[tokio::test]
    async fn insert_todo_item() {
        let (mut conn, repo) = setup().await;
        let now = Utc::now();
        let tx = conn.transaction().await.expect("tx begin failed");
        repo.insert(
            &tx,
            Uuid::now_v7(),
            "test item",
            &TodoStatus::New,
            &now,
            &now,
        )
        .await
        .expect("insert failed");
        tx.commit().await.expect("commit failed");

        let items = repo.list_active().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "test item");
    }

    #[tokio::test]
    async fn list_active_filters_done() {
        let (mut conn, repo) = setup().await;
        let now = Utc::now();

        let tx = conn.transaction().await.expect("tx begin failed");
        repo.insert(
            &tx,
            Uuid::now_v7(),
            "new item",
            &TodoStatus::New,
            &now,
            &now,
        )
        .await
        .expect("insert failed");
        repo.insert(
            &tx,
            Uuid::now_v7(),
            "in progress item",
            &TodoStatus::InProgress,
            &now,
            &now,
        )
        .await
        .expect("insert failed");
        repo.insert(
            &tx,
            Uuid::now_v7(),
            "done item",
            &TodoStatus::Done,
            &now,
            &now,
        )
        .await
        .expect("insert failed");
        tx.commit().await.expect("commit failed");

        let items = repo.list_active().await.expect("list failed");
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.status != TodoStatus::Done));
    }
}
