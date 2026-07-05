use anyhow::Result;
use chrono::{DateTime, Utc};
use turso::Connection;
use turso::Error::QueryReturnedNoRows;
use turso::transaction::Transaction;
use uuid::Uuid;

use crate::models::summary::Summary;
use crate::repositories::parse_timestamp;

const SELECT_COLUMNS: &str =
    "summary_id, file_path, start, \"end\", created_at, updated_at FROM summary";

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

pub struct SummaryFilter {
    id: Option<Uuid>,
}

impl SummaryFilter {
    pub fn new() -> Self {
        Self { id: None }
    }

    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    fn build_where_clause(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if let Some(id) = self.id {
            conditions.push("summary_id = ?");
            params.push(id.to_string());
        }
        let clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        (clause, params)
    }
}

pub async fn find_one(conn: &Connection, filter: &SummaryFilter) -> Result<Option<Summary>> {
    let (where_clause, params) = filter.build_where_clause();
    let sql = format!(
        "SELECT {}{} ORDER BY created_at ASC LIMIT 1",
        SELECT_COLUMNS, where_clause
    );

    let mut rows = conn.query(&sql, params).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return Ok(Some(row_to_summary(&row)?));
    }
    Ok(None)
}

pub async fn insert(
    tx: &Transaction<'_>,
    id: Uuid,
    file_path: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Summary> {
    let sql = r#"INSERT INTO summary (summary_id, file_path, start, "end", created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 RETURNING summary_id, file_path, start, "end", created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
    let mut rows = tx
        .query(
            sql,
            (
                id.to_string(),
                file_path.to_string(),
                start.to_rfc3339(),
                end.to_rfc3339(),
                now.clone(),
                now,
            ),
        )
        .await?;
    let row = rows.next().await?;
    while rows.next().await?.is_some() {}
    if let Some(row) = row {
        return row_to_summary(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn list_summaries(conn: &Connection) -> Result<Vec<Summary>> {
    let sql = "SELECT summary_id, file_path, start, \"end\", created_at, updated_at FROM summary ORDER BY created_at ASC;";
    let mut rows = conn.query(sql, ()).await?;

    let mut summaries = Vec::new();
    while let Some(row) = rows.next().await? {
        summaries.push(row_to_summary(&row)?);
    }
    Ok(summaries)
}

pub async fn touch(tx: &Transaction<'_>, summary_id: &Uuid) -> Result<()> {
    let sql = "UPDATE summary SET updated_at = CURRENT_TIMESTAMP WHERE summary_id = ?";
    tx.execute(sql, vec![summary_id.to_string()]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::{TimeZone, Utc};

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
    async fn insert_creates_summary_row() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let summary = insert(&tx, id, "test.md", start, end)
            .await
            .expect("insert failed");

        assert_eq!(summary.id, id);
        assert_eq!(summary.file_path, "test.md");
        tx.commit().await.expect("commit failed");

        let found = find_one(&conn, &SummaryFilter::new().id(summary.id))
            .await
            .expect("find failed");
        assert!(found.is_some());
        let found = found.expect("expected some");
        assert_eq!(found.id, summary.id);
        assert_eq!(found.file_path, "test.md");
        assert_eq!(found.start.timestamp(), start.timestamp());
        assert_eq!(found.end.timestamp(), end.timestamp());
    }

    #[tokio::test]
    async fn insert_stores_start_and_end_correctly() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let id = Uuid::now_v7();
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 11, 30, 0).unwrap();

        let summary = insert(&tx, id, "test.md", start, end)
            .await
            .expect("insert failed");
        tx.commit().await.expect("commit failed");

        let found = find_one(&conn, &SummaryFilter::new().id(summary.id))
            .await
            .expect("find failed");
        assert!(found.is_some());
        let found = found.expect("expected some");
        assert_eq!(found.start, start);
        assert_eq!(found.end, end);
    }
}
