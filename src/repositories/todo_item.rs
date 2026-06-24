use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};
use turso::{Connection, Error::QueryReturnedNoRows, transaction::Transaction};
use uuid::Uuid;

use crate::models::todo_item::{TodoItem, TodoStatus};
use std::str::FromStr;

fn parse_timestamp(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")?;
    Ok(naive.and_utc())
}

pub async fn insert(tx: &Transaction<'_>, label: &str) -> Result<TodoItem> {
    let sql = r#"INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 RETURNING todo_item_id, label, status, created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::now_v7();
    let mut rows = tx
        .query(
            sql,
            (
                id.clone().to_string(),
                label.to_string(),
                TodoStatus::New.to_string(),
                now.clone(),
                now,
            ),
        )
        .await?;
    if let Some(row) = rows.next().await? {
        let id_str: String = row.get(0)?;
        let id = Uuid::parse_str(&id_str)?;
        let label: String = row.get(1)?;
        let status_str: String = row.get(2)?;
        let status = TodoStatus::from_str(&status_str)?;
        let created_str: String = row.get(3)?;
        let created_at = parse_timestamp(&created_str)?;
        let updated_str: String = row.get(4)?;
        let updated_at = parse_timestamp(&updated_str)?;

        return Ok(TodoItem {
            id,
            label,
            status,
            created_at,
            updated_at,
        });
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn list_active(conn: &Connection) -> Result<Vec<TodoItem>> {
    let sql = "SELECT todo_item_id, label, status, created_at, updated_at FROM todo_item WHERE status != 'DONE' ORDER BY created_at ASC";
    let mut rows = conn.query(sql, ()).await?;

    let mut items = Vec::new();
    while let Some(row) = rows.next().await? {
        let id_str: String = row.get(0)?;
        let id = Uuid::parse_str(&id_str)?;
        let label: String = row.get(1)?;
        let status_str: String = row.get(2)?;
        let status = TodoStatus::from_str(&status_str)?;
        let created_str: String = row.get(3)?;
        let created_at = parse_timestamp(&created_str)?;
        let updated_str: String = row.get(4)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    async fn setup() -> Connection {
        let db = turso::Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        conn
    }

    #[tokio::test]
    async fn insert_todo_item() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");
        insert(&tx, "test item").await.expect("insert failed");
        tx.commit().await.expect("commit failed");

        let items = list_active(&conn).await.expect("list failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "test item");
    }

    #[tokio::test]
    async fn list_active_filters_done() {
        let mut conn = setup().await;

        let tx = conn.transaction().await.expect("tx begin failed");
        insert(&tx, "new item").await.expect("insert failed");
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, 'in progress item', 'IN_PROGRESS', ?, ?)",
            (Uuid::now_v7().to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()),
        )
        .await
        .expect("insert in progress failed");
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, 'done item', 'DONE', ?, ?)",
            (Uuid::now_v7().to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()),
        )
        .await
        .expect("insert done failed");
        tx.commit().await.expect("commit failed");

        let items = list_active(&conn).await.expect("list failed");
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.status != TodoStatus::Done));
    }
}
