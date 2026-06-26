use anyhow::Result;
use chrono::{DateTime, Local, LocalResult, TimeZone, Utc};
use turso::Connection;

use crate::models::event::EventType;
use crate::models::meeting::Meeting;
use crate::repositories;

pub struct MeetingService {
    conn: Connection,
}

impl MeetingService {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub async fn create_meeting(
        &mut self,
        name: &str,
        scheduled_at: DateTime<Utc>,
    ) -> Result<Meeting> {
        let tx = self.conn.transaction().await?;
        let meeting = repositories::meeting::insert(&tx, name, scheduled_at).await?;
        repositories::event::insert(&tx, meeting.id, &EventType::MeetingCreated, "{}").await?;
        tx.commit().await?;
        Ok(meeting)
    }

    pub async fn list_meetings_for_today(&self) -> Result<Vec<Meeting>> {
        let now = Local::now();
        let local_midnight = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid");
        let start = match local_midnight.and_local_timezone(Local) {
            LocalResult::Single(dt) => dt.with_timezone(&Utc),
            LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
            LocalResult::None => {
                let date = now.date_naive();
                Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
            }
        };
        let end = start + chrono::Duration::days(1);
        repositories::meeting::list_by_date(&self.conn, start, end).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::Duration;
    use turso::Builder;

    async fn setup() -> MeetingService {
        let db = Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        MeetingService::new(conn)
    }

    #[tokio::test]
    async fn create_meeting_inserts_row() {
        let mut service = setup().await;
        let scheduled_at = Utc::now();

        let meeting = service
            .create_meeting("Test Meeting", scheduled_at)
            .await
            .expect("create failed");

        assert_eq!(meeting.name, "Test Meeting");
    }

    #[tokio::test]
    async fn create_meeting_created_event() {
        let mut service = setup().await;
        let scheduled_at = Utc::now();

        let meeting = service
            .create_meeting("Event Test Meeting", scheduled_at)
            .await
            .expect("create failed");

        let conn = &service.conn;
        let mut stmt = conn
            .prepare("SELECT entity_id, event_type FROM event WHERE entity_id = ?")
            .await
            .expect("prepare failed");
        let mut rows = stmt
            .query([meeting.id.to_string()])
            .await
            .expect("query failed");

        let row = rows.next().await.expect("next failed").expect("row exists");
        let entity_id: String = row.get(0).expect("get entity_id");
        let event_type: String = row.get(1).expect("get event_type");

        assert_eq!(entity_id, meeting.id.to_string());
        assert_eq!(event_type, "MEETING_CREATED");
    }

    #[tokio::test]
    async fn create_meeting_is_atomic() {
        let mut service = setup().await;
        let scheduled_at = Utc::now();

        let meeting = service
            .create_meeting("Atomic Test Meeting", scheduled_at)
            .await
            .expect("create failed");

        let conn = &service.conn;

        let mut stmt = conn
            .prepare("SELECT meeting_id FROM meeting WHERE meeting_id = ?")
            .await
            .expect("prepare meeting failed");
        let mut rows = stmt
            .query([meeting.id.to_string()])
            .await
            .expect("query meeting failed");
        assert!(rows.next().await.expect("next failed").is_some());

        let mut stmt = conn
            .prepare("SELECT event_id FROM event WHERE entity_id = ?")
            .await
            .expect("prepare event failed");
        let mut rows = stmt
            .query([meeting.id.to_string()])
            .await
            .expect("query event failed");
        assert!(rows.next().await.expect("next failed").is_some());
    }

    #[tokio::test]
    async fn list_meetings_for_today() {
        let mut service = setup().await;
        let now = Local::now();
        let local_midnight = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid");
        let today_start = match local_midnight.and_local_timezone(Local) {
            LocalResult::Single(dt) => dt.with_timezone(&Utc),
            LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
            LocalResult::None => {
                let date = now.date_naive();
                Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
            }
        };

        let today_meeting = service
            .create_meeting("Today Meeting", today_start + Duration::hours(10))
            .await
            .expect("create today failed");

        let tomorrow_start = today_start + Duration::days(1);
        let _tomorrow_meeting = service
            .create_meeting("Tomorrow Meeting", tomorrow_start + Duration::hours(10))
            .await
            .expect("create tomorrow failed");

        let meetings = service
            .list_meetings_for_today()
            .await
            .expect("list failed");

        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0].id, today_meeting.id);
    }

    #[tokio::test]
    async fn create_meeting_with_special_chars() {
        let mut service = setup().await;
        let scheduled_at = Utc::now();
        let special_name = "Meeting with \"quotes\" & <special> chars";

        let meeting = service
            .create_meeting(special_name, scheduled_at)
            .await
            .expect("create failed");

        assert_eq!(meeting.name, special_name);
    }
}
