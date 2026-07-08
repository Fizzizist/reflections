use anyhow::Result;
use std::path::PathBuf;
use uuid::Uuid;

use crate::models::event::EventType;
use crate::models::todo_item::{TodoItem, TodoStatus};
use crate::repositories;
use crate::with_conn;
use crate::with_txn;

pub struct TodoService {
    db_path: PathBuf,
}

impl TodoService {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub async fn create_todo_item(&mut self, label: &str) -> Result<TodoItem> {
        with_txn!(&self.db_path, |tx| {
            let todo_item = repositories::todo_item::insert(&tx, label).await?;
            repositories::event::insert(&tx, todo_item.id, &EventType::TodoItemCreated, "{}")
                .await?;
            Ok(todo_item)
        })
    }

    pub async fn list_todo_items(&self) -> Result<Vec<TodoItem>> {
        with_conn!(&self.db_path, |conn| {
            repositories::todo_item::list_active(conn).await
        })
    }

    pub async fn update_todo_status(
        &mut self,
        id: Uuid,
        new_status: TodoStatus,
    ) -> Result<TodoItem> {
        with_txn!(&self.db_path, |tx| {
            let old_item = repositories::todo_item::get_by_id(&tx, id).await?;
            let updated_item = repositories::todo_item::update_status(&tx, id, &new_status).await?;
            let metadata = serde_json::json!({
                "old_status": old_item.status.to_string(),
                "new_status": new_status.to_string()
            })
            .to_string();
            repositories::event::insert(&tx, id, &EventType::TodoItemStatusChanged, &metadata)
                .await?;
            Ok(updated_item)
        })
    }

    pub async fn list_all_todo_items(&self) -> Result<Vec<TodoItem>> {
        with_conn!(&self.db_path, |conn| {
            repositories::todo_item::list_all(conn).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::todo_item::TodoStatus;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    async fn setup() -> (TodoService, PathBuf, tempfile::TempDir) {
        let dir = tempdir().expect("create tempdir failed");
        let db_path = dir.path().join("test.db");
        let _db = Database::open_path(&db_path).await.expect("db open failed");
        let svc = TodoService::new(db_path.clone());
        (svc, db_path, dir)
    }

    async fn query_string(db_path: &PathBuf, sql: &str, params: impl turso::IntoParams) -> String {
        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db.conn().query(sql, params).await.expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        row.get::<String>(0).expect("get string")
    }

    #[tokio::test]
    async fn create_todo_item_inserts_row() {
        let (mut svc, _db_path, _dir) = setup().await;
        let item = svc
            .create_todo_item("buy groceries")
            .await
            .expect("create failed");
        assert_eq!(item.label, "buy groceries");
        assert_eq!(item.status, TodoStatus::New);

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "buy groceries");
        assert_eq!(items[0].status, TodoStatus::New);
    }

    #[tokio::test]
    async fn create_todo_item_creates_event() {
        let (mut svc, db_path, _dir) = setup().await;
        let item = svc
            .create_todo_item("test event")
            .await
            .expect("create failed");

        let event_type = query_string(
            &db_path,
            "SELECT event_type FROM event WHERE entity_id = ?",
            [item.id.to_string()],
        )
        .await;
        assert_eq!(event_type, "TODO_ITEM_CREATED");
    }

    #[tokio::test]
    async fn list_todo_items_excludes_done() {
        let (mut svc, db_path, _dir) = setup().await;
        svc.create_todo_item("new item")
            .await
            .expect("create failed");
        svc.create_todo_item("another new")
            .await
            .expect("create failed");

        let now = Utc::now();
        let mut db = Database::open_path(&db_path).await.expect("db open failed");
        let tx = db.conn_mut().transaction().await.expect("tx begin failed");
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, ?, 'DONE', ?, ?)",
            (
                Uuid::now_v7().to_string(),
                "done item".to_string(),
                now.to_rfc3339(),
                now.to_rfc3339(),
            ),
        )
        .await
        .expect("insert done item failed");
        tx.commit().await.expect("commit failed");
        drop(db);

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.status != TodoStatus::Done));
    }

    #[tokio::test]
    async fn list_todo_items_ordered_by_created_at() {
        let (svc, db_path, _dir) = setup().await;

        let earlier = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        let later = chrono::DateTime::parse_from_rfc3339("2024-06-01T00:00:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);

        let mut db = Database::open_path(&db_path).await.expect("db open failed");
        let tx = db.conn_mut().transaction().await.expect("tx begin failed");
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, 'later_item', 'NEW', ?, ?)",
            (
                Uuid::now_v7().to_string(),
                later.to_rfc3339(),
                later.to_rfc3339(),
            ),
        )
        .await
        .expect("insert later failed");
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, 'earlier_item', 'NEW', ?, ?)",
            (
                Uuid::now_v7().to_string(),
                earlier.to_rfc3339(),
                earlier.to_rfc3339(),
            ),
        )
        .await
        .expect("insert earlier failed");
        tx.commit().await.expect("commit failed");
        drop(db);

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "earlier_item");
        assert_eq!(items[1].label, "later_item");
    }

    #[tokio::test]
    async fn create_todo_item_is_atomic() {
        let (mut svc, db_path, _dir) = setup().await;
        let item = svc
            .create_todo_item("atomic test")
            .await
            .expect("create failed");

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "atomic test");

        let entity_id = query_string(
            &db_path,
            "SELECT entity_id FROM event WHERE entity_id = ?",
            [item.id.to_string()],
        )
        .await;
        assert_eq!(entity_id, item.id.to_string());
    }

    #[tokio::test]
    async fn create_todo_item_with_special_chars() {
        let (mut svc, _db_path, _dir) = setup().await;
        let label = "buy milk & eggs (2x) — urgent!";
        svc.create_todo_item(label).await.expect("create failed");

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, label);
    }

    #[tokio::test]
    async fn create_todo_item_with_long_label() {
        let (mut svc, _db_path, _dir) = setup().await;
        let label = "a".repeat(250);
        svc.create_todo_item(&label).await.expect("create failed");

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label.len(), 250);
        assert_eq!(items[0].label, label);
    }

    #[tokio::test]
    async fn update_todo_status_changes_status() {
        let (mut svc, _db_path, _dir) = setup().await;
        let item = svc
            .create_todo_item("test item")
            .await
            .expect("create failed");
        assert_eq!(item.status, TodoStatus::New);

        let updated = svc
            .update_todo_status(item.id, TodoStatus::InProgress)
            .await
            .expect("update failed");
        assert_eq!(updated.status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn update_todo_status_creates_event_with_metadata() {
        let (mut svc, db_path, _dir) = setup().await;
        let item = svc
            .create_todo_item("test item")
            .await
            .expect("create failed");

        svc.update_todo_status(item.id, TodoStatus::InProgress)
            .await
            .expect("update failed");

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db
            .conn()
            .query(
                "SELECT event_type, metadata FROM event WHERE entity_id = ? AND event_type = 'TODO_ITEM_STATUS_CHANGED'",
                [item.id.to_string()],
            )
            .await
            .expect("query failed");

        let row = rows
            .next()
            .await
            .expect("row fetch failed")
            .expect("status change event not found");

        let event_type_str: String = row.get(0).expect("get event_type");
        assert_eq!(event_type_str, "TODO_ITEM_STATUS_CHANGED");

        let metadata_str: String = row.get(1).expect("get metadata");
        let metadata: serde_json::Value =
            serde_json::from_str(&metadata_str).expect("invalid json");
        assert_eq!(metadata["old_status"], "NEW");
        assert_eq!(metadata["new_status"], "IN_PROGRESS");
    }

    #[tokio::test]
    async fn update_todo_status_updates_updated_at() {
        let (mut svc, _db_path, _dir) = setup().await;
        let item = svc
            .create_todo_item("test item")
            .await
            .expect("create failed");
        let original_updated = item.updated_at;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let updated = svc
            .update_todo_status(item.id, TodoStatus::InProgress)
            .await
            .expect("update failed");
        assert!(updated.updated_at > original_updated);
    }

    #[tokio::test]
    async fn update_todo_status_reverse_transition() {
        let (mut svc, _db_path, _dir) = setup().await;
        let item = svc
            .create_todo_item("test item")
            .await
            .expect("create failed");
        assert_eq!(item.status, TodoStatus::New);

        let done = svc
            .update_todo_status(item.id, TodoStatus::Done)
            .await
            .expect("update to Done failed");
        assert_eq!(done.status, TodoStatus::Done);

        let back_to_new = svc
            .update_todo_status(item.id, TodoStatus::New)
            .await
            .expect("update to New failed");
        assert_eq!(back_to_new.status, TodoStatus::New);
    }
}

#[cfg(test)]
impl TodoService {
    pub async fn create_todo_with_fixed_time(
        &mut self,
        label: &str,
        id: Uuid,
        timestamp: &str,
    ) -> Result<TodoItem> {
        with_txn!(&self.db_path, |tx| {
            tx.execute(
                "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, ?, 'NEW', ?, ?)",
                (id.to_string(), label.to_string(), timestamp.to_string(), timestamp.to_string()),
            )
            .await?;
            tx.execute(
                "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, 'TODO_ITEM_CREATED', '{}', ?, ?)",
                (Uuid::now_v7().to_string(), id.to_string(), timestamp.to_string(), timestamp.to_string()),
            )
            .await?;
            Ok(())
        })?;

        with_conn!(&self.db_path, |conn| {
            let item = repositories::todo_item::find_by_id(conn, id)
                .await?
                .expect("todo should exist after insert");
            Ok(item)
        })
    }
}
