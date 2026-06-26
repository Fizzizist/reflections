use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};
use turso::{Connection, Error::QueryReturnedNoRows, transaction::Transaction};
use uuid::Uuid;

use crate::models::meeting::Meeting;

fn parse_timestamp(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")?;
    Ok(naive.and_utc())
}

fn row_to_meeting(row: &turso::Row) -> Result<Meeting> {
    let id_str: String = row.get(0)?;
    let id = Uuid::parse_str(&id_str)?;
    let name: String = row.get(1)?;
    let scheduled_at_str: String = row.get(2)?;
    let scheduled_at = parse_timestamp(&scheduled_at_str)?;
    let created_str: String = row.get(3)?;
    let created_at = parse_timestamp(&created_str)?;
    let updated_str: String = row.get(4)?;
    let updated_at = parse_timestamp(&updated_str)?;
    Ok(Meeting {
        id,
        name,
        scheduled_at,
        created_at,
        updated_at,
    })
}

pub async fn insert(
    tx: &Transaction<'_>,
    name: &str,
    scheduled_at: DateTime<Utc>,
) -> Result<Meeting> {
    let sql = r#"INSERT INTO meeting (meeting_id, name, scheduled_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 RETURNING meeting_id, name, scheduled_at, created_at, updated_at"#;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::now_v7();
    let mut rows = tx
        .query(
            sql,
            (
                id.to_string(),
                name.to_string(),
                scheduled_at.to_rfc3339(),
                now.clone(),
                now,
            ),
        )
        .await?;
    let row = rows.next().await?;
    drop(rows);
    if let Some(row) = row {
        return row_to_meeting(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn list_by_date(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Meeting>> {
    let sql = "SELECT meeting_id, name, scheduled_at, created_at, updated_at FROM meeting WHERE scheduled_at >= ? AND scheduled_at < ? ORDER BY scheduled_at ASC";
    let mut rows = conn
        .query(
            sql,
            (
                start.format("%Y-%m-%d %H:%M:%S").to_string(),
                end.format("%Y-%m-%d %H:%M:%S").to_string(),
            ),
        )
        .await?;

    let mut meetings = Vec::new();
    while let Some(row) = rows.next().await? {
        meetings.push(row_to_meeting(&row)?);
    }

    Ok(meetings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::Duration;

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
    async fn insert_meeting() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let scheduled_at = Utc::now();
        let meeting = insert(&tx, "Test Meeting", scheduled_at)
            .await
            .expect("insert failed");

        assert_eq!(meeting.name, "Test Meeting");
        assert!(
            meeting.scheduled_at >= scheduled_at - Duration::seconds(1)
                && meeting.scheduled_at <= scheduled_at + Duration::seconds(1)
        );
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn list_by_date_returns_meetings_in_range() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let m1 = insert(&tx, "Meeting 1", start + Duration::minutes(10))
            .await
            .expect("insert m1 failed");
        let m2 = insert(&tx, "Meeting 2", start + Duration::minutes(30))
            .await
            .expect("insert m2 failed");

        tx.commit().await.expect("commit failed");

        let meetings = list_by_date(&conn, start, end).await.expect("list failed");

        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].id, m1.id);
        assert_eq!(meetings[1].id, m2.id);
    }

    #[tokio::test]
    async fn list_by_date_excludes_other_days() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let today_start = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid")
            .and_utc();
        let today_end = today_start + Duration::days(1);

        let yesterday = today_start - Duration::days(1);
        let tomorrow = today_end;

        insert(&tx, "Yesterday Meeting", yesterday)
            .await
            .expect("insert yesterday failed");
        let today_meeting = insert(&tx, "Today Meeting", today_start + Duration::hours(10))
            .await
            .expect("insert today failed");
        insert(&tx, "Tomorrow Meeting", tomorrow)
            .await
            .expect("insert tomorrow failed");

        tx.commit().await.expect("commit failed");

        let meetings = list_by_date(&conn, today_start, today_end)
            .await
            .expect("list failed");

        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0].id, today_meeting.id);
    }
}
