use anyhow::Result;
use chrono::{DateTime, Local, LocalResult, TimeZone, Utc};
use serde::Serialize;
use std::path::PathBuf;

use crate::calendar::backend::CalendarBackend;
use crate::models::event::EventType;
use crate::models::meeting::Meeting;
use crate::repositories;
use crate::repositories::meeting::MeetingFilter;
use crate::with_conn;
use crate::with_txn;

pub struct MeetingService {
    db_path: PathBuf,
}

impl MeetingService {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub async fn create_meeting(
        &mut self,
        name: &str,
        scheduled_at: DateTime<Utc>,
    ) -> Result<Meeting> {
        with_txn!(&self.db_path, |tx| {
            let meeting = repositories::meeting::insert(&tx, name, scheduled_at).await?;
            repositories::event::insert(&tx, meeting.id, &EventType::MeetingCreated, "{}").await?;
            Ok(meeting)
        })
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
        let filter = MeetingFilter::new().start(start).end(end);
        with_conn!(&self.db_path, |conn| {
            repositories::meeting::find(conn, &filter).await
        })
    }

    pub async fn sync_meetings(
        &mut self,
        backend: &dyn CalendarBackend,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<SyncResult>> {
        with_txn!(&self.db_path, |tx| {
            let calendar_events = backend.fetch_meetings(start, end).await?;

            let names: Vec<String> = calendar_events.iter().map(|e| e.name.clone()).collect();
            let existing_meetings =
                repositories::meeting::find_by_names_in_range(&tx, &names, start, end).await?;
            let existing_map: std::collections::HashMap<&str, &Meeting> = existing_meetings
                .iter()
                .map(|m| (m.name.as_str(), m))
                .collect();

            let mut results = Vec::new();

            for event in calendar_events {
                match existing_map.get(event.name.as_str()) {
                    Some(meeting) if meeting.scheduled_at == event.scheduled_at => {
                        continue;
                    }
                    Some(meeting) => {
                        let updated_meeting = Meeting {
                            id: meeting.id,
                            name: meeting.name.clone(),
                            scheduled_at: event.scheduled_at,
                            created_at: meeting.created_at,
                            updated_at: meeting.updated_at,
                        };
                        let updated = repositories::meeting::update(&tx, &updated_meeting).await?;
                        results.push(SyncResult {
                            action: SyncAction::Updated,
                            meeting: updated,
                        });
                    }
                    None => {
                        let new_meeting =
                            repositories::meeting::insert(&tx, &event.name, event.scheduled_at)
                                .await?;
                        repositories::event::insert(
                            &tx,
                            new_meeting.id,
                            &EventType::MeetingCreated,
                            "{}",
                        )
                        .await?;
                        results.push(SyncResult {
                            action: SyncAction::Created,
                            meeting: new_meeting,
                        });
                    }
                }
            }

            Ok(results)
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum SyncAction {
    Created,
    Updated,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub action: SyncAction,
    pub meeting: Meeting,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::types::CalendarEvent;
    use crate::db::Database;
    use async_trait::async_trait;
    use chrono::Duration;
    use chrono::Timelike;
    use tempfile::{TempDir, tempdir};

    struct MockCalendarBackend {
        events: Vec<CalendarEvent>,
    }

    #[async_trait]
    impl CalendarBackend for MockCalendarBackend {
        async fn fetch_meetings(
            &self,
            _start: DateTime<Utc>,
            _end: DateTime<Utc>,
        ) -> Result<Vec<CalendarEvent>> {
            Ok(self.events.clone())
        }
    }

    struct FailingCalendarBackend;

    #[async_trait]
    impl CalendarBackend for FailingCalendarBackend {
        async fn fetch_meetings(
            &self,
            _start: DateTime<Utc>,
            _end: DateTime<Utc>,
        ) -> Result<Vec<CalendarEvent>> {
            Err(anyhow::anyhow!("backend failure"))
        }
    }

    async fn setup() -> (MeetingService, PathBuf, TempDir) {
        let dir = tempdir().expect("create tempdir failed");
        let db_path = dir.path().join("test.db");
        let _db = Database::open_path(&db_path).await.expect("db open failed");
        let svc = MeetingService::new(db_path.clone());
        (svc, db_path, dir)
    }

    async fn query_count(db_path: &PathBuf, sql: &str, params: impl turso::IntoParams) -> i64 {
        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db.conn().query(sql, params).await.expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        row.get(0).expect("get count")
    }

    async fn query_string(db_path: &PathBuf, sql: &str, params: impl turso::IntoParams) -> String {
        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db.conn().query(sql, params).await.expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        row.get::<String>(0).expect("get string")
    }

    #[tokio::test]
    async fn sync_meetings_creates_new_meetings() {
        let (mut service, _db_path, _dir) = setup().await;
        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let mock = MockCalendarBackend {
            events: vec![
                CalendarEvent {
                    name: "New Meeting 1".to_string(),
                    scheduled_at: now,
                },
                CalendarEvent {
                    name: "New Meeting 2".to_string(),
                    scheduled_at: now + Duration::minutes(30),
                },
            ],
        };

        let results = service
            .sync_meetings(&mock, start, end)
            .await
            .expect("sync failed");

        assert_eq!(results.len(), 2);
        assert!(matches!(results[0].action, SyncAction::Created));
        assert!(matches!(results[1].action, SyncAction::Created));
        assert_eq!(results[0].meeting.name, "New Meeting 1");
        assert_eq!(results[1].meeting.name, "New Meeting 2");
    }

    #[tokio::test]
    async fn sync_meetings_updates_existing_meeting() {
        let (mut service, _db_path, _dir) = setup().await;
        let now = Utc::now().with_nanosecond(0).expect("valid");
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);
        let original_time = now;
        let new_time = now + Duration::hours(2);

        let original = service
            .create_meeting("Existing Meeting", original_time)
            .await
            .expect("create failed");

        let mock = MockCalendarBackend {
            events: vec![CalendarEvent {
                name: "Existing Meeting".to_string(),
                scheduled_at: new_time,
            }],
        };

        let results = service
            .sync_meetings(&mock, start, end)
            .await
            .expect("sync failed");

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].action, SyncAction::Updated));
        assert_eq!(results[0].meeting.id, original.id);
        assert_eq!(results[0].meeting.scheduled_at, new_time);
    }

    #[tokio::test]
    async fn sync_meetings_idempotent_when_time_matches() {
        let (mut service, _db_path, _dir) = setup().await;
        let now = Utc::now().with_nanosecond(0).expect("valid");
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);
        let scheduled_at = now;

        service
            .create_meeting("Same Time Meeting", scheduled_at)
            .await
            .expect("create failed");

        let mock = MockCalendarBackend {
            events: vec![CalendarEvent {
                name: "Same Time Meeting".to_string(),
                scheduled_at,
            }],
        };

        let results = service
            .sync_meetings(&mock, start, end)
            .await
            .expect("sync failed");

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn sync_meetings_creates_meeting_created_events_only_for_new() {
        let (mut service, db_path, _dir) = setup().await;
        let now = Utc::now().with_nanosecond(0).expect("valid");
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let original = service
            .create_meeting("Pre-existing Meeting", now)
            .await
            .expect("create failed");

        let mock = MockCalendarBackend {
            events: vec![
                CalendarEvent {
                    name: "Pre-existing Meeting".to_string(),
                    scheduled_at: now + Duration::minutes(30),
                },
                CalendarEvent {
                    name: "Brand New Meeting".to_string(),
                    scheduled_at: now,
                },
            ],
        };

        let results = service
            .sync_meetings(&mock, start, end)
            .await
            .expect("sync failed");

        assert_eq!(results.len(), 2);
        assert!(matches!(results[0].action, SyncAction::Updated));
        assert!(matches!(results[1].action, SyncAction::Created));

        let count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM event WHERE entity_id = ?",
            [original.id.to_string()],
        )
        .await;
        assert_eq!(count, 1);

        let event_type = query_string(
            &db_path,
            "SELECT event_type FROM event WHERE entity_id = ?",
            [results[1].meeting.id.to_string()],
        )
        .await;
        assert_eq!(event_type, "MEETING_CREATED");
    }

    #[tokio::test]
    async fn sync_meetings_empty_backend_returns_empty_vec() {
        let (mut service, _db_path, _dir) = setup().await;
        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let mock = MockCalendarBackend { events: vec![] };

        let results = service
            .sync_meetings(&mock, start, end)
            .await
            .expect("sync failed");

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn sync_meetings_correct_sync_result_actions() {
        let (mut service, _db_path, _dir) = setup().await;
        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        service
            .create_meeting("To Update", now)
            .await
            .expect("create failed");

        let mock = MockCalendarBackend {
            events: vec![
                CalendarEvent {
                    name: "To Update".to_string(),
                    scheduled_at: now + Duration::hours(1),
                },
                CalendarEvent {
                    name: "To Create".to_string(),
                    scheduled_at: now,
                },
            ],
        };

        let results = service
            .sync_meetings(&mock, start, end)
            .await
            .expect("sync failed");

        assert_eq!(results.len(), 2);
        assert!(matches!(results[0].action, SyncAction::Updated));
        assert!(matches!(results[1].action, SyncAction::Created));
    }

    #[tokio::test]
    async fn create_meeting_inserts_row() {
        let (mut service, _db_path, _dir) = setup().await;
        let scheduled_at = Utc::now();

        let meeting = service
            .create_meeting("Test Meeting", scheduled_at)
            .await
            .expect("create failed");

        assert_eq!(meeting.name, "Test Meeting");
    }

    #[tokio::test]
    async fn create_meeting_created_event() {
        let (mut service, db_path, _dir) = setup().await;
        let scheduled_at = Utc::now();

        let meeting = service
            .create_meeting("Event Test Meeting", scheduled_at)
            .await
            .expect("create failed");

        let event_type = query_string(
            &db_path,
            "SELECT event_type FROM event WHERE entity_id = ?",
            [meeting.id.to_string()],
        )
        .await;
        assert_eq!(event_type, "MEETING_CREATED");
    }

    #[tokio::test]
    async fn create_meeting_is_atomic() {
        let (mut service, db_path, _dir) = setup().await;
        let scheduled_at = Utc::now();

        let meeting = service
            .create_meeting("Atomic Test Meeting", scheduled_at)
            .await
            .expect("create failed");

        let meeting_exists = query_count(
            &db_path,
            "SELECT COUNT(*) FROM meeting WHERE meeting_id = ?",
            [meeting.id.to_string()],
        )
        .await;
        assert_eq!(meeting_exists, 1);

        let event_exists = query_count(
            &db_path,
            "SELECT COUNT(*) FROM event WHERE entity_id = ?",
            [meeting.id.to_string()],
        )
        .await;
        assert_eq!(event_exists, 1);
    }

    #[tokio::test]
    async fn list_meetings_for_today() {
        let (mut service, _db_path, _dir) = setup().await;
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
        let (mut service, _db_path, _dir) = setup().await;
        let scheduled_at = Utc::now();
        let special_name = "Meeting with \"quotes\" & <special> chars";

        let meeting = service
            .create_meeting(special_name, scheduled_at)
            .await
            .expect("create failed");

        assert_eq!(meeting.name, special_name);
    }

    #[tokio::test]
    async fn sync_meetings_rolls_back_on_backend_failure() {
        let (mut service, db_path, _dir) = setup().await;
        let backend = FailingCalendarBackend;
        let start = Utc::now();
        let end = start + Duration::days(1);

        let result = service.sync_meetings(&backend, start, end).await;
        assert!(result.is_err());

        let count = query_count(&db_path, "SELECT COUNT(*) FROM meeting", ()).await;
        assert_eq!(count, 0);
    }
}
