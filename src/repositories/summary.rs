use anyhow::Result;
use turso::Connection;
use uuid::Uuid;

use crate::models::summary::Summary;
use crate::repositories::parse_timestamp;

fn row_to_summary(row: &turso::Row) -> Result<Summary> {
    let id_str: String = row.get(0)?;
    let id = Uuid::parse_str(&id_str)?;
    let file_path: String = row.get(1)?;
    let start_str: String = row.get(2)?;
    let start = parse_timestamp(&start_str)?;
    let end_str: String = row.get(3)?;
    let end = parse_timestamp(&end_str)?;
    let created_str: String = row.get(4)?;
    let created_at = parse_timestamp(&created_str)?;
    let updated_str: String = row.get(5)?;
    let updated_at = parse_timestamp(&updated_str)?;

    Ok(Summary {
        id,
        file_path,
        start,
        end,
        created_at,
        updated_at,
    })
}

pub async fn find_by_id(conn: &Connection, id: Uuid) -> Result<Option<Summary>> {
    let sql = "SELECT summary_id, file_path, start, \"end\", created_at, updated_at FROM summary WHERE summary_id = ?";
    let mut rows = conn.query(sql, (id.to_string(),)).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return Ok(Some(row_to_summary(&row)?));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::Utc;

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
    async fn find_by_id_returns_some_when_found() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        let ts = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        tx.execute(
            "INSERT INTO summary (summary_id, file_path, start, \"end\", created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (id.to_string(), "test.md".to_string(), ts.clone(), ts.clone(), ts.clone(), ts.clone()),
        ).await.expect("insert failed");
        tx.commit().await.expect("commit failed");

        let found = find_by_id(&conn, id).await.expect("find failed");

        assert!(found.is_some());
        let found = found.expect("expected some");
        assert_eq!(found.id, id);
        assert_eq!(found.file_path, "test.md");
    }

    #[tokio::test]
    async fn find_by_id_returns_none_when_not_found() {
        let mut conn = setup().await;

        let nonexistent = Uuid::now_v7();
        let found = find_by_id(&conn, nonexistent).await.expect("find failed");

        assert!(found.is_none());
    }
}
