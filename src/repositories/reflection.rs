#![allow(dead_code)]

use anyhow::Result;
use chrono::Utc;
use turso::{Connection, Error::QueryReturnedNoRows, transaction::Transaction};
use uuid::Uuid;

use crate::models::reflection::Reflection;
use crate::repositories::parse_timestamp;

fn row_to_reflection(row: &turso::Row) -> Result<Reflection> {
    let id_str: String = row.get(0)?;
    let id = Uuid::parse_str(&id_str)?;
    let file_path: String = row.get(2)?;
    let created_str: String = row.get(3)?;
    let created_at = parse_timestamp(&created_str)?;
    let updated_str: String = row.get(4)?;
    let updated_at = parse_timestamp(&updated_str)?;

    let about_id = match row.get_value(1)? {
        turso::Value::Text(s) => Some(Uuid::parse_str(&s)?),
        turso::Value::Null => None,
        _ => None,
    };

    Ok(Reflection {
        id,
        about_id,
        file_path,
        created_at,
        updated_at,
    })
}

pub async fn insert(
    tx: &Transaction<'_>,
    about_id: Option<Uuid>,
    file_path: &str,
) -> Result<Reflection> {
    let sql = r#"INSERT INTO reflection (reflection_id, about_id, file_path, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 RETURNING reflection_id, about_id, file_path, created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::now_v7();
    let about_id_str = about_id.map(|id| id.to_string());
    let mut rows = tx
        .query(
            sql,
            (
                id.to_string(),
                about_id_str,
                file_path.to_string(),
                now.clone(),
                now,
            ),
        )
        .await?;
    let row = rows.next().await?;
    while rows.next().await?.is_some() {}
    if let Some(row) = row {
        return row_to_reflection(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn list_ordered_by_updated_at(conn: &Connection) -> Result<Vec<Reflection>> {
    let sql = "SELECT reflection_id, about_id, file_path, created_at, updated_at FROM reflection ORDER BY updated_at DESC";
    let mut rows = conn.query(sql, ()).await?;

    let mut reflections = Vec::new();
    while let Some(row) = rows.next().await? {
        reflections.push(row_to_reflection(&row)?);
    }

    Ok(reflections)
}

pub async fn delete(tx: &Transaction<'_>, id: Uuid) -> Result<()> {
    let sql = "DELETE FROM reflection WHERE reflection_id = ?";
    tx.execute(sql, (id.to_string(),)).await?;
    Ok(())
}

pub async fn get_by_id(tx: &Transaction<'_>, id: Uuid) -> Result<Reflection> {
    let sql = "SELECT reflection_id, about_id, file_path, created_at, updated_at FROM reflection WHERE reflection_id = ?";
    let mut rows = tx.query(sql, (id.to_string(),)).await?;
    let row = rows.next().await?;
    while rows.next().await?.is_some() {}
    if let Some(row) = row {
        return row_to_reflection(&row);
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
    async fn insert_with_about_id() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");
        let about_id = Uuid::now_v7();

        let reflection = insert(&tx, Some(about_id), "/path/to/reflection.md")
            .await
            .expect("insert failed");

        assert_eq!(reflection.about_id, Some(about_id));
        assert_eq!(reflection.file_path, "/path/to/reflection.md");
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn insert_with_null_about_id() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let reflection = insert(&tx, None, "/path/to/reflection.md")
            .await
            .expect("insert failed");

        assert_eq!(reflection.about_id, None);
        assert_eq!(reflection.file_path, "/path/to/reflection.md");
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn list_ordered_by_updated_at() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let r1 = insert(&tx, None, "/path/to/first.md")
            .await
            .expect("insert r1 failed");
        tx.commit().await.expect("commit failed");

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let tx2 = conn.transaction().await.expect("tx2 begin failed");
        let r2 = insert(&tx2, None, "/path/to/second.md")
            .await
            .expect("insert r2 failed");
        tx2.commit().await.expect("commit2 failed");

        let reflections = super::list_ordered_by_updated_at(&conn)
            .await
            .expect("list failed");

        assert_eq!(reflections.len(), 2);
        assert_eq!(reflections[0].id, r2.id);
        assert_eq!(reflections[1].id, r1.id);
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let reflection = insert(&tx, None, "/path/to/reflection.md")
            .await
            .expect("insert failed");
        super::delete(&tx, reflection.id)
            .await
            .expect("delete failed");
        tx.commit().await.expect("commit failed");

        let reflections = super::list_ordered_by_updated_at(&conn)
            .await
            .expect("list failed");
        assert_eq!(reflections.len(), 0);
    }

    #[tokio::test]
    async fn get_by_id_returns_reflection() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let inserted = insert(&tx, None, "/path/to/reflection.md")
            .await
            .expect("insert failed");

        let fetched = super::get_by_id(&tx, inserted.id)
            .await
            .expect("get failed");
        assert_eq!(fetched.id, inserted.id);
        assert_eq!(fetched.file_path, inserted.file_path);
        assert_eq!(fetched.about_id, inserted.about_id);
        tx.commit().await.expect("commit failed");
    }
}
