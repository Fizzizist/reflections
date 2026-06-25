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

fn row_to_todo_item(row: &turso::Row) -> Result<TodoItem> {
    let id_str: String = row.get(0)?;
    let id = Uuid::parse_str(&id_str)?;
    let label: String = row.get(1)?;
    let status_str: String = row.get(2)?;
    let status = TodoStatus::from_str(&status_str)?;
    let created_str: String = row.get(3)?;
    let created_at = parse_timestamp(&created_str)?;
    let updated_str: String = row.get(4)?;
    let updated_at = parse_timestamp(&updated_str)?;
    Ok(TodoItem {
        id,
        label,
        status,
        created_at,
        updated_at,
    })
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
    let row = rows.next().await?;
    drop(rows);
    if let Some(row) = row {
        return row_to_todo_item(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn list_active(conn: &Connection) -> Result<Vec<TodoItem>> {
    let sql = "SELECT todo_item_id, label, status, created_at, updated_at FROM todo_item WHERE status != 'DONE' ORDER BY created_at ASC";
    let mut rows = conn.query(sql, ()).await?;

    let mut items = Vec::new();
    while let Some(row) = rows.next().await? {
        items.push(row_to_todo_item(&row)?);
    }

    Ok(items)
}

pub async fn list_all(conn: &Connection) -> Result<Vec<TodoItem>> {
    let sql = "SELECT todo_item_id, label, status, created_at, updated_at FROM todo_item ORDER BY created_at ASC";
    let mut rows = conn.query(sql, ()).await?;

    let mut items = Vec::new();
    while let Some(row) = rows.next().await? {
        items.push(row_to_todo_item(&row)?);
    }

    Ok(items)
}

pub async fn update_status(
    tx: &Transaction<'_>,
    id: Uuid,
    new_status: &TodoStatus,
) -> Result<TodoItem> {
    let sql = r#"UPDATE todo_item SET status = ?, updated_at = ? WHERE todo_item_id = ?
                 RETURNING todo_item_id, label, status, created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
    let mut rows = tx
        .query(sql, (new_status.to_string(), now.clone(), id.to_string()))
        .await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return row_to_todo_item(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn get_by_id(tx: &Transaction<'_>, id: Uuid) -> Result<TodoItem> {
    let sql = "SELECT todo_item_id, label, status, created_at, updated_at FROM todo_item WHERE todo_item_id = ?";
    let mut rows = tx.query(sql, (id.to_string(),)).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return row_to_todo_item(&row);
    }
    Err(QueryReturnedNoRows.into())
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

    #[tokio::test]
    async fn update_status_changes_status() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");
        let item = insert(&tx, "test item").await.expect("insert failed");
        assert_eq!(item.status, TodoStatus::New);

        let updated = update_status(&tx, item.id, &TodoStatus::InProgress)
            .await
            .expect("update failed");
        assert_eq!(updated.status, TodoStatus::InProgress);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn update_status_updates_updated_at() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");
        let item = insert(&tx, "test item").await.expect("insert failed");
        let original_updated = item.updated_at;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let updated = update_status(&tx, item.id, &TodoStatus::InProgress)
            .await
            .expect("update failed");
        assert!(updated.updated_at > original_updated);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn list_all_includes_done() {
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

        let items = list_all(&conn).await.expect("list failed");
        assert_eq!(items.len(), 3);
        assert!(items.iter().any(|i| i.status == TodoStatus::Done));
    }

    #[tokio::test]
    async fn get_by_id_returns_correct_item() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");
        let inserted = insert(&tx, "test item").await.expect("insert failed");

        let fetched = get_by_id(&tx, inserted.id).await.expect("get failed");
        assert_eq!(fetched.id, inserted.id);
        assert_eq!(fetched.label, "test item");
        assert_eq!(fetched.status, TodoStatus::New);
        assert_eq!(fetched.created_at, inserted.created_at);
        assert_eq!(fetched.updated_at, inserted.updated_at);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn update_status_done_to_new() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, ?, 'DONE', ?, ?)",
            (id.to_string(), "test item".to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()),
        )
        .await
        .expect("insert failed");

        let item = get_by_id(&tx, id).await.expect("get failed");
        assert_eq!(item.status, TodoStatus::Done);

        let updated = update_status(&tx, id, &TodoStatus::New)
            .await
            .expect("update failed");
        assert_eq!(updated.status, TodoStatus::New);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn update_status_in_progress_to_new() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        tx.execute(
            "INSERT INTO todo_item (todo_item_id, label, status, created_at, updated_at) VALUES (?, ?, 'IN_PROGRESS', ?, ?)",
            (id.to_string(), "test item".to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()),
        )
        .await
        .expect("insert failed");

        let item = get_by_id(&tx, id).await.expect("get failed");
        assert_eq!(item.status, TodoStatus::InProgress);

        let updated = update_status(&tx, id, &TodoStatus::New)
            .await
            .expect("update failed");
        assert_eq!(updated.status, TodoStatus::New);
        tx.commit().await.expect("commit failed");
    }
}
