use anyhow::Result;
use chrono::Utc;
use turso::{Connection, Error::QueryReturnedNoRows, transaction::Transaction};
use uuid::Uuid;

use crate::models::note::Note;
use crate::repositories::parse_timestamp;

fn row_to_note(row: &turso::Row) -> Result<Note> {
    let id_str: String = row.get(0)?;
    let id = Uuid::parse_str(&id_str)?;
    let file_path: String = row.get(2)?;
    let created_str: String = row.get(3)?;
    let created_at = parse_timestamp(&created_str)?;
    let updated_str: String = row.get(4)?;
    let updated_at = parse_timestamp(&updated_str)?;

    let related_to_id = match row.get_value(1)? {
        turso::Value::Text(s) => Some(Uuid::parse_str(&s)?),
        turso::Value::Null => None,
        _ => None,
    };

    Ok(Note {
        id,
        related_to_id,
        file_path,
        created_at,
        updated_at,
    })
}

pub async fn insert(
    tx: &Transaction<'_>,
    related_to_id: Option<Uuid>,
    file_path: &str,
) -> Result<Note> {
    let sql = r#"INSERT INTO note (note_id, related_to_id, file_path, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 RETURNING note_id, related_to_id, file_path, created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::now_v7();
    let related_to_id_str = related_to_id.map(|id| id.to_string());
    let mut rows = tx
        .query(
            sql,
            (
                id.to_string(),
                related_to_id_str,
                file_path.to_string(),
                now.clone(),
                now,
            ),
        )
        .await?;
    let row = rows.next().await?;
    while rows.next().await?.is_some() {}
    if let Some(row) = row {
        return row_to_note(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn get_by_id(tx: &Transaction<'_>, id: Uuid) -> Result<Note> {
    let sql = "SELECT note_id, related_to_id, file_path, created_at, updated_at FROM note WHERE note_id = ?";
    let mut rows = tx.query(sql, (id.to_string(),)).await?;
    let row = rows.next().await?;
    while rows.next().await?.is_some() {}
    if let Some(row) = row {
        return row_to_note(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn delete(tx: &Transaction<'_>, id: Uuid) -> Result<()> {
    let sql = "DELETE FROM note WHERE note_id = ?";
    tx.execute(sql, (id.to_string(),)).await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn find_by_id(conn: &Connection, id: Uuid) -> Result<Option<Note>> {
    let sql = "SELECT note_id, related_to_id, file_path, created_at, updated_at FROM note WHERE note_id = ?";
    let mut rows = conn.query(sql, (id.to_string(),)).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return Ok(Some(row_to_note(&row)?));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use turso::Connection;

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
    async fn insert_with_related_to_id() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");
        let related_id = Uuid::now_v7();

        let note = insert(&tx, Some(related_id), "/path/to/note.md")
            .await
            .expect("insert failed");

        assert_eq!(note.related_to_id, Some(related_id));
        assert_eq!(note.file_path, "/path/to/note.md");
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn insert_with_null_related_to_id() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let note = insert(&tx, None, "/path/to/note.md")
            .await
            .expect("insert failed");

        assert_eq!(note.related_to_id, None);
        assert_eq!(note.file_path, "/path/to/note.md");
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let note = insert(&tx, None, "/path/to/note.md")
            .await
            .expect("insert failed");
        super::delete(&tx, note.id).await.expect("delete failed");
        tx.commit().await.expect("commit failed");

        let mut rows = conn
            .query("SELECT COUNT(*) FROM note", ())
            .await
            .expect("count query failed");
        let row = rows.next().await.expect("fetch failed").expect("no rows");
        while rows.next().await.expect("fetch failed").is_some() {}
        let note_count: i64 = row.get(0).expect("get count failed");
        assert_eq!(note_count, 0);
    }

    #[tokio::test]
    async fn get_by_id_returns_note() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let inserted = insert(&tx, None, "/path/to/note.md")
            .await
            .expect("insert failed");

        let fetched = super::get_by_id(&tx, inserted.id)
            .await
            .expect("get failed");
        assert_eq!(fetched.id, inserted.id);
        assert_eq!(fetched.file_path, inserted.file_path);
        assert_eq!(fetched.related_to_id, inserted.related_to_id);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn find_by_id_returns_some_when_found() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let inserted = insert(&tx, None, "/path/to/note.md")
            .await
            .expect("insert failed");
        tx.commit().await.expect("commit failed");

        let found = super::find_by_id(&conn, inserted.id)
            .await
            .expect("find failed");

        assert!(found.is_some());
        let found = found.expect("expected some");
        assert_eq!(found.id, inserted.id);
        assert_eq!(found.file_path, inserted.file_path);
    }

    #[tokio::test]
    async fn find_by_id_returns_none_when_not_found() {
        let mut conn = setup().await;

        let nonexistent = Uuid::now_v7();
        let found = super::find_by_id(&conn, nonexistent)
            .await
            .expect("find failed");

        assert!(found.is_none());
    }
}
