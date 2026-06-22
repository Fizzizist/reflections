use anyhow::Result;
use chrono::Utc;
use turso::Connection;
use uuid::Uuid;

use crate::models::event::EventType;
use crate::models::todo_item::{TodoItem, TodoStatus};
use crate::repositories::event::EventRepository;
use crate::repositories::todo_item::TodoItemRepository;

pub struct TodoService {
    conn: Connection,
    todo_item_repo: TodoItemRepository,
    event_repo: EventRepository,
}

impl TodoService {
    pub fn new(conn: Connection) -> Self {
        let todo_item_repo = TodoItemRepository::new(conn.clone());
        let event_repo = EventRepository::new(conn.clone());
        Self {
            conn,
            todo_item_repo,
            event_repo,
        }
    }

    pub async fn create_todo_item(&mut self, label: &str) -> Result<TodoItem> {
        let now = Utc::now();
        let todo_id = Uuid::now_v7();
        let event_id = Uuid::now_v7();

        let tx = self.conn.transaction().await?;
        self.todo_item_repo
            .insert(&tx, todo_id, label, &TodoStatus::New, &now, &now)
            .await?;
        self.event_repo
            .insert(
                &tx,
                event_id,
                todo_id,
                &EventType::TodoItemCreated,
                "{}",
                &now,
                &now,
            )
            .await?;
        tx.commit().await?;

        Ok(TodoItem {
            id: todo_id,
            label: label.to_string(),
            status: TodoStatus::New,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn list_todo_items(&self) -> Result<Vec<TodoItem>> {
        self.todo_item_repo.list_active().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    async fn setup() -> TodoService {
        let db = turso::Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        TodoService::new(conn)
    }

    #[tokio::test]
    async fn create_todo_item_inserts_row() {
        let mut svc = setup().await;
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
        let mut svc = setup().await;
        let item = svc
            .create_todo_item("test event")
            .await
            .expect("create failed");

        let mut rows = svc
            .conn
            .query(
                "SELECT event_type FROM event WHERE entity_id = ?",
                [item.id.to_string()],
            )
            .await
            .expect("query failed");

        let row = rows
            .next()
            .await
            .expect("row fetch failed")
            .expect("event not found");

        let event_type_str = row
            .get_value(0)
            .expect("value extraction failed")
            .as_text()
            .expect("expected text")
            .to_string();
        assert_eq!(event_type_str, "TODO_ITEM_CREATED");
    }

    #[tokio::test]
    async fn list_todo_items_excludes_done() {
        let mut svc = setup().await;
        svc.create_todo_item("new item")
            .await
            .expect("create failed");
        svc.create_todo_item("another new")
            .await
            .expect("create failed");

        let mut conn = svc.conn.clone();
        let now = Utc::now();
        let tx = conn.transaction().await.expect("tx begin failed");
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

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.status != TodoStatus::Done));
    }

    #[tokio::test]
    async fn list_todo_items_ordered_by_created_at() {
        let svc = setup().await;

        let earlier = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        let later = chrono::DateTime::parse_from_rfc3339("2024-06-01T00:00:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);

        let mut conn = svc.conn.clone();
        let tx = conn.transaction().await.expect("tx begin failed");
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, 'later_item', 'NEW', ?, ?)",
            (Uuid::now_v7().to_string(), later.to_rfc3339(), later.to_rfc3339()),
        )
        .await
        .expect("insert later failed");
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, 'earlier_item', 'NEW', ?, ?)",
            (Uuid::now_v7().to_string(), earlier.to_rfc3339(), earlier.to_rfc3339()),
        )
        .await
        .expect("insert earlier failed");
        tx.commit().await.expect("commit failed");

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "earlier_item");
        assert_eq!(items[1].label, "later_item");
    }

    #[tokio::test]
    async fn create_todo_item_is_atomic() {
        let mut svc = setup().await;
        let item = svc
            .create_todo_item("atomic test")
            .await
            .expect("create failed");

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "atomic test");

        let mut rows = svc
            .conn
            .query(
                "SELECT entity_id FROM event WHERE entity_id = ?",
                [item.id.to_string()],
            )
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("row fetch failed")
            .expect("event not found");
        let entity_id_str = row
            .get_value(0)
            .expect("value extraction failed")
            .as_text()
            .expect("expected text")
            .to_string();
        assert_eq!(entity_id_str, item.id.to_string());
    }

    #[tokio::test]
    async fn create_todo_item_with_special_chars() {
        let mut svc = setup().await;
        let label = "buy milk & eggs (2x) — urgent!";
        svc.create_todo_item(label).await.expect("create failed");

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, label);
    }

    #[tokio::test]
    async fn create_todo_item_with_long_label() {
        let mut svc = setup().await;
        let label = "a".repeat(250);
        svc.create_todo_item(&label).await.expect("create failed");

        let items = svc.list_todo_items().await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label.len(), 250);
        assert_eq!(items[0].label, label);
    }
}
