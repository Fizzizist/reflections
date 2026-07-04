use anyhow::Result;
use chrono::Utc;
use turso::{Connection, Error::QueryReturnedNoRows, transaction::Transaction};
use uuid::Uuid;

use crate::models::note::Note;
use crate::repositories::parse_timestamp;

const SELECT_COLUMNS: &str = "note_id, related_to_id, file_path, created_at, updated_at FROM note";

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

pub struct NoteFilter {
    pub id: Option<Uuid>,
    pub related_to_id: Option<Uuid>,
}

impl NoteFilter {
    pub fn new() -> Self {
        Self {
            id: None,
            related_to_id: None,
        }
    }

    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    pub fn related_to_id(mut self, related_to_id: Uuid) -> Self {
        self.related_to_id = Some(related_to_id);
        self
    }

    fn build_where_clause(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if let Some(id) = self.id {
            conditions.push("note_id = ?");
            params.push(id.to_string());
        }
        if let Some(related_to_id) = self.related_to_id {
            conditions.push("related_to_id = ?");
            params.push(related_to_id.to_string());
        }

        let clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        (clause, params)
    }
}

pub async fn find(conn: &Connection, filter: &NoteFilter) -> Result<Vec<Note>> {
    let (where_clause, params) = filter.build_where_clause();
    let sql = format!(
        "SELECT {}{} ORDER BY created_at ASC",
        SELECT_COLUMNS, where_clause
    );

    let mut rows = conn.query(&sql, params).await?;

    let mut notes = Vec::new();
    while let Some(row) = rows.next().await? {
        notes.push(row_to_note(&row)?);
    }

    Ok(notes)
}

pub async fn find_one(conn: &Connection, filter: &NoteFilter) -> Result<Option<Note>> {
    let (where_clause, params) = filter.build_where_clause();
    let sql = format!(
        "SELECT {}{} ORDER BY created_at ASC LIMIT 1",
        SELECT_COLUMNS, where_clause
    );

    let mut rows = conn.query(&sql, params).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return Ok(Some(row_to_note(&row)?));
    }
    Ok(None)
}

pub async fn insert(
    tx: &Transaction<'_>,
    id: Uuid,
    related_to_id: Option<Uuid>,
    file_path: &str,
) -> Result<Note> {
    let sql = r#"INSERT INTO note (note_id, related_to_id, file_path, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 RETURNING note_id, related_to_id, file_path, created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
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

        let id = Uuid::now_v7();
        let note = insert(&tx, id, Some(related_id), "/path/to/note.md")
            .await
            .expect("insert failed");

        assert_eq!(note.related_to_id, Some(related_id));
        assert_eq!(note.file_path, "/path/to/note.md");
        assert_eq!(note.id, id);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn insert_with_null_related_to_id() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        let note = insert(&tx, id, None, "/path/to/note.md")
            .await
            .expect("insert failed");

        assert_eq!(note.related_to_id, None);
        assert_eq!(note.file_path, "/path/to/note.md");
        assert_eq!(note.id, id);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        let note = insert(&tx, id, None, "/path/to/note.md")
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

        let id = Uuid::now_v7();
        let inserted = insert(&tx, id, None, "/path/to/note.md")
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
    async fn note_filter_by_related_to_id_returns_linked() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let related_id = Uuid::now_v7();
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        let id3 = Uuid::now_v7();

        insert(&tx, id1, Some(related_id), "/path/to/n1.md")
            .await
            .expect("insert n1 failed");
        insert(&tx, id2, Some(related_id), "/path/to/n2.md")
            .await
            .expect("insert n2 failed");
        insert(&tx, id3, None, "/path/to/n3.md")
            .await
            .expect("insert n3 failed");

        tx.commit().await.expect("commit failed");

        let filter = NoteFilter::new().related_to_id(related_id);
        let notes = find(&conn, &filter).await.expect("find failed");

        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|n| n.related_to_id == Some(related_id)));
    }

    #[tokio::test]
    async fn note_filter_by_related_to_id_excludes_unlinked() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let related_id = Uuid::now_v7();
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();

        insert(&tx, id1, None, "/path/to/n1.md")
            .await
            .expect("insert n1 failed");
        insert(&tx, id2, None, "/path/to/n2.md")
            .await
            .expect("insert n2 failed");

        tx.commit().await.expect("commit failed");

        let filter = NoteFilter::new().related_to_id(related_id);
        let notes = find(&conn, &filter).await.expect("find failed");

        assert_eq!(notes.len(), 0);
    }

    #[tokio::test]
    async fn note_filter_by_id_single_result() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        let inserted = insert(&tx, id, None, "/path/to/note.md")
            .await
            .expect("insert failed");

        tx.commit().await.expect("commit failed");

        let filter = NoteFilter::new().id(id);
        let notes = find(&conn, &filter).await.expect("find failed");

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, inserted.id);
    }

    #[tokio::test]
    async fn note_filter_empty_returns_all() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        let id3 = Uuid::now_v7();

        insert(&tx, id1, None, "/path/to/n1.md")
            .await
            .expect("insert n1 failed");
        insert(&tx, id2, None, "/path/to/n2.md")
            .await
            .expect("insert n2 failed");
        insert(&tx, id3, None, "/path/to/n3.md")
            .await
            .expect("insert n3 failed");

        tx.commit().await.expect("commit failed");

        let filter = NoteFilter::new();
        let notes = find(&conn, &filter).await.expect("find failed");

        assert_eq!(notes.len(), 3);
    }
}
