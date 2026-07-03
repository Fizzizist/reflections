use anyhow::Result;
use chrono::{DateTime, Utc};
use turso::{Connection, Error::QueryReturnedNoRows, transaction::Transaction};
use uuid::Uuid;

use crate::models::meeting::Meeting;
use crate::repositories::parse_timestamp;

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

const SELECT_COLUMNS: &str = "meeting_id, name, scheduled_at, created_at, updated_at FROM meeting";

pub struct MeetingFilter {
    pub id: Option<Uuid>,
    pub name: Option<String>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

impl MeetingFilter {
    pub fn new() -> Self {
        Self {
            id: None,
            name: None,
            start: None,
            end: None,
        }
    }

    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn start(mut self, start: DateTime<Utc>) -> Self {
        self.start = Some(start);
        self
    }

    pub fn end(mut self, end: DateTime<Utc>) -> Self {
        self.end = Some(end);
        self
    }

    fn build_where_clause(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if let Some(id) = self.id {
            conditions.push("meeting_id = ?");
            params.push(id.to_string());
        }
        if let Some(name) = &self.name {
            conditions.push("name = ?");
            params.push(name.clone());
        }
        if let Some(start) = self.start {
            conditions.push("scheduled_at >= ?");
            params.push(start.format("%Y-%m-%d %H:%M:%S").to_string());
        }
        if let Some(end) = self.end {
            conditions.push("scheduled_at < ?");
            params.push(end.format("%Y-%m-%d %H:%M:%S").to_string());
        }

        let clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        (clause, params)
    }
}

pub async fn insert(
    tx: &Transaction<'_>,
    name: &str,
    scheduled_at: DateTime<Utc>,
) -> Result<Meeting> {
    let sql = r#"INSERT INTO meeting (meeting_id, name, scheduled_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 RETURNING meeting_id, name, scheduled_at, created_at, updated_at"#;
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let id = Uuid::now_v7();
    let mut rows = tx
        .query(
            sql,
            (
                id.to_string(),
                name.to_string(),
                scheduled_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                now.clone(),
                now,
            ),
        )
        .await?;
    let row = rows.next().await?;
    if let Some(row) = row {
        return row_to_meeting(&row);
    }
    Err(QueryReturnedNoRows.into())
}

pub async fn find(conn: &Connection, filter: &MeetingFilter) -> Result<Vec<Meeting>> {
    let (where_clause, params) = filter.build_where_clause();
    let sql = format!(
        "SELECT {}{} ORDER BY scheduled_at ASC",
        SELECT_COLUMNS, where_clause
    );

    let mut rows = conn.query(&sql, params).await?;

    let mut meetings = Vec::new();
    while let Some(row) = rows.next().await? {
        meetings.push(row_to_meeting(&row)?);
    }

    Ok(meetings)
}

pub async fn find_one(conn: &Connection, filter: &MeetingFilter) -> Result<Option<Meeting>> {
    let (where_clause, params) = filter.build_where_clause();
    let sql = format!(
        "SELECT {}{} ORDER BY scheduled_at ASC LIMIT 1",
        SELECT_COLUMNS, where_clause
    );

    let mut rows = conn.query(&sql, params).await?;
    let row = rows.next().await?;
    while rows.next().await.is_ok_and(|r| r.is_some()) {}
    if let Some(row) = row {
        return Ok(Some(row_to_meeting(&row)?));
    }
    Ok(None)
}

pub async fn update(tx: &Transaction<'_>, meeting: &Meeting) -> Result<Meeting> {
    let sql = r#"UPDATE meeting
                 SET name = ?, scheduled_at = ?, updated_at = ?
                 WHERE meeting_id = ?
                 RETURNING meeting_id, name, scheduled_at, created_at, updated_at"#;
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut rows = tx
        .query(
            sql,
            (
                meeting.name.clone(),
                meeting.scheduled_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                now,
                meeting.id.to_string(),
            ),
        )
        .await?;
    let row = rows.next().await?;
    if let Some(row) = row {
        return row_to_meeting(&row);
    }
    Err(QueryReturnedNoRows.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::Duration;
    use chrono::Timelike;

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

    async fn make_meeting(
        tx: &Transaction<'_>,
        name: &str,
        scheduled_at: DateTime<Utc>,
    ) -> Meeting {
        insert(tx, name, scheduled_at).await.expect("insert failed")
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
    async fn find_by_date_range_returns_meetings_in_range() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let m1 = make_meeting(&tx, "Meeting 1", start + Duration::minutes(10)).await;
        let m2 = make_meeting(&tx, "Meeting 2", start + Duration::minutes(30)).await;

        tx.commit().await.expect("commit failed");

        let filter = MeetingFilter::new().start(start).end(end);
        let meetings = find(&conn, &filter).await.expect("find failed");

        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].id, m1.id);
        assert_eq!(meetings[1].id, m2.id);
    }

    #[tokio::test]
    async fn find_by_date_range_excludes_other_days() {
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

        make_meeting(&tx, "Yesterday Meeting", yesterday).await;
        let today_meeting =
            make_meeting(&tx, "Today Meeting", today_start + Duration::hours(10)).await;
        make_meeting(&tx, "Tomorrow Meeting", tomorrow).await;

        tx.commit().await.expect("commit failed");

        let filter = MeetingFilter::new().start(today_start).end(today_end);
        let meetings = find(&conn, &filter).await.expect("find failed");

        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0].id, today_meeting.id);
    }

    #[tokio::test]
    async fn find_one_by_id_returns_meeting() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let inserted = make_meeting(&tx, "Find By ID", now).await;

        tx.commit().await.expect("commit failed");

        let filter = MeetingFilter::new().id(inserted.id);
        let found = find_one(&conn, &filter)
            .await
            .expect("find failed")
            .expect("meeting exists");

        assert_eq!(found.id, inserted.id);
        assert_eq!(found.name, "Find By ID");
    }

    #[tokio::test]
    async fn find_one_by_id_returns_none_when_not_found() {
        let conn = setup().await;

        let filter = MeetingFilter::new().id(Uuid::now_v7());
        let result = find_one(&conn, &filter).await.expect("find failed");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn find_one_by_name_and_date_range_returns_existing_meeting() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let inserted = make_meeting(&tx, "Target Meeting", now).await;

        let filter = MeetingFilter::new()
            .name("Target Meeting")
            .start(start)
            .end(end);
        let found = find_one(&tx, &filter)
            .await
            .expect("find failed")
            .expect("meeting exists");

        assert_eq!(found.id, inserted.id);
        assert_eq!(found.name, "Target Meeting");
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn find_one_by_name_and_date_range_returns_none_for_no_match() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let filter = MeetingFilter::new()
            .name("Nonexistent Meeting")
            .start(start)
            .end(end);
        let result = find_one(&tx, &filter).await.expect("find failed");

        assert!(result.is_none());
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn find_one_by_name_and_date_range_returns_none_for_name_match_outside_range() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let now = Utc::now();
        let start = now + Duration::hours(2);
        let end = now + Duration::hours(4);
        let scheduled_at = now - Duration::hours(2);

        make_meeting(&tx, "Outside Range Meeting", scheduled_at).await;

        let filter = MeetingFilter::new()
            .name("Outside Range Meeting")
            .start(start)
            .end(end);
        let result = find_one(&tx, &filter).await.expect("find failed");

        assert!(result.is_none());
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn update_changes_scheduled_at_and_bumps_updated_at() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let original_scheduled = Utc::now();
        let meeting = make_meeting(&tx, "Update Test Meeting", original_scheduled).await;

        let new_scheduled = original_scheduled + Duration::hours(2);
        let updated_meeting = Meeting {
            id: meeting.id,
            name: meeting.name.clone(),
            scheduled_at: new_scheduled,
            created_at: meeting.created_at,
            updated_at: meeting.updated_at,
        };
        let updated = update(&tx, &updated_meeting).await.expect("update failed");

        assert_eq!(updated.id, meeting.id);
        assert_eq!(updated.name, "Update Test Meeting");
        let updated_scheduled_truncated = updated.scheduled_at.with_nanosecond(0).expect("valid");
        let new_scheduled_truncated = new_scheduled.with_nanosecond(0).expect("valid");
        assert_eq!(updated_scheduled_truncated, new_scheduled_truncated);
        assert!(updated.updated_at >= meeting.created_at);
        tx.commit().await.expect("commit failed");
    }

    #[tokio::test]
    async fn update_changes_name() {
        let mut conn = setup().await;
        let tx = conn.transaction().await.expect("tx begin failed");

        let scheduled = Utc::now();
        let meeting = make_meeting(&tx, "Original Name", scheduled).await;

        let updated_meeting = Meeting {
            id: meeting.id,
            name: "New Name".to_string(),
            scheduled_at: meeting.scheduled_at,
            created_at: meeting.created_at,
            updated_at: meeting.updated_at,
        };
        let updated = update(&tx, &updated_meeting).await.expect("update failed");

        assert_eq!(updated.name, "New Name");
        tx.commit().await.expect("commit failed");
    }
}
