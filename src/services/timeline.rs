use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;
use tokio::fs;
use turso::Connection;
use uuid::Uuid;

use crate::models::event::{Event, EventType};
use crate::models::meeting::Meeting;
use crate::models::note::Note;
use crate::models::reflection::Reflection;
use crate::models::summary::Summary;
use crate::models::todo_item::TodoItem;
use crate::repositories;
use crate::repositories::event::EventFilter;
use crate::repositories::meeting::MeetingFilter;
use crate::repositories::note::NoteFilter;
use crate::repositories::reflection::ReflectionFilter;
use crate::services::editable::EditableEntityRecord;

#[derive(Serialize)]
pub struct TimelineEntry {
    pub event_id: Uuid,
    pub entity_id: Uuid,
    pub event_type: EventType,
    pub metadata: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub entity: Option<TimelineEntity>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum TimelineEntity {
    TodoItem(TodoItem),
    Meeting(Meeting),
    Reflection(ReflectionWithContent),
    Note(NoteWithContent),
    Summary(SummaryWithContent),
}

#[derive(Serialize)]
pub struct ReflectionWithContent {
    #[serde(flatten)]
    pub reflection: Reflection,
    pub content: Option<String>,
}

#[derive(Serialize)]
pub struct NoteWithContent {
    #[serde(flatten)]
    pub note: Note,
    pub content: Option<String>,
}

#[derive(Serialize)]
pub struct SummaryWithContent {
    #[serde(flatten)]
    pub summary: Summary,
    pub content: Option<String>,
}

#[derive(Clone)]
pub struct TimelineService {
    conn: Connection,
    root_dir: PathBuf,
}

impl TimelineService {
    pub fn new(conn: Connection, root_dir: PathBuf) -> Self {
        Self { conn, root_dir }
    }

    pub async fn get_timeline(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimelineEntry>> {
        let events = repositories::event::list_by_date_range(&self.conn, start, end).await?;
        let mut entries = Vec::new();
        for event in events {
            let entity = self.resolve_entity(&event).await.unwrap_or(None);
            entries.push(TimelineEntry {
                event_id: event.event_id,
                entity_id: event.entity_id,
                event_type: event.event_type,
                metadata: event.metadata,
                created_at: event.created_at,
                updated_at: event.updated_at,
                entity,
            });
        }
        Ok(entries)
    }

    pub async fn get_entity_timeline(&self, entity_id: Uuid) -> Result<Vec<TimelineEntry>> {
        let mut entity_ids = vec![entity_id];
        let linked_reflections = repositories::reflection::find(
            &self.conn,
            &ReflectionFilter::new().about_id(entity_id),
        )
        .await?;

        entity_ids.extend(
            linked_reflections
                .iter()
                .map(|r| r.id)
                .collect::<Vec<Uuid>>(),
        );

        let linked_notes =
            repositories::note::find(&self.conn, &NoteFilter::new().related_to_id(entity_id))
                .await?;

        entity_ids.extend(linked_notes.iter().map(|n| n.id).collect::<Vec<Uuid>>());

        let mut events =
            repositories::event::find(&self.conn, &EventFilter::new().entity_id_in(entity_ids))
                .await?;

        events.sort_by_key(|a| a.created_at);

        let mut entries = Vec::new();
        for event in events {
            let entity = self.resolve_entity(&event).await.unwrap_or(None);
            entries.push(TimelineEntry {
                event_id: event.event_id,
                entity_id: event.entity_id,
                event_type: event.event_type,
                metadata: event.metadata,
                created_at: event.created_at,
                updated_at: event.updated_at,
                entity,
            });
        }
        Ok(entries)
    }

    async fn resolve_entity(&self, event: &Event) -> Result<Option<TimelineEntity>> {
        match event.event_type {
            EventType::TodoItemCreated | EventType::TodoItemStatusChanged => {
                let item = repositories::todo_item::find_by_id(&self.conn, event.entity_id).await?;
                Ok(item.map(TimelineEntity::TodoItem))
            }
            EventType::MeetingCreated => {
                let meeting = repositories::meeting::find_one(
                    &self.conn,
                    &MeetingFilter::new().id(event.entity_id),
                )
                .await?;
                Ok(meeting.map(TimelineEntity::Meeting))
            }
            EventType::ReflectionCreated => {
                let reflection = repositories::reflection::find_one(
                    &self.conn,
                    &ReflectionFilter::new().id(event.entity_id),
                )
                .await?;
                Ok(self
                    .attach_content(reflection, |r, content| {
                        TimelineEntity::Reflection(ReflectionWithContent {
                            reflection: r,
                            content,
                        })
                    })
                    .await)
            }
            EventType::NoteCreated => {
                let note = repositories::note::find_one(
                    &self.conn,
                    &NoteFilter::new().id(event.entity_id),
                )
                .await?;
                Ok(self
                    .attach_content(note, |n, content| {
                        TimelineEntity::Note(NoteWithContent { note: n, content })
                    })
                    .await)
            }
            EventType::SummaryCreated => {
                let summary = repositories::summary::find_one(
                    &self.conn,
                    &repositories::summary::SummaryFilter::new().id(event.entity_id),
                )
                .await?;
                Ok(self
                    .attach_content(summary, |s, content| {
                        TimelineEntity::Summary(SummaryWithContent {
                            summary: s,
                            content,
                        })
                    })
                    .await)
            }
        }
    }

    async fn attach_content<T: EditableEntityRecord>(
        &self,
        entity: Option<T>,
        wrap: impl Fn(T, Option<String>) -> TimelineEntity,
    ) -> Option<TimelineEntity> {
        match entity {
            Some(e) => {
                let content = self.read_file_content(e.file_path()).await;
                Some(wrap(e, content))
            }
            None => None,
        }
    }

    async fn read_file_content(&self, file_path: &str) -> Option<String> {
        let full_path = self.root_dir.join(file_path);
        fs::read_to_string(&full_path).await.ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use crate::services::meeting::MeetingService;
    use crate::services::note::NoteService;
    use crate::services::reflection::ReflectionService;
    use crate::services::todo::TodoService;
    use chrono::Duration;
    use tempfile::tempdir;

    async fn setup() -> (TimelineService, Connection, PathBuf, tempfile::TempDir) {
        let db = turso::Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("db build failed");
        let conn = db.connect().expect("db connect failed");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();
        let service = TimelineService::new(conn.clone(), root_path.clone());
        (service, conn, root_path, root_dir)
    }

    #[tokio::test]
    async fn get_timeline_returns_events_with_entities() {
        let (svc, conn, root_path, _root_dir) = setup().await;

        let mut todo_svc = TodoService::new(conn.clone());
        let todo = todo_svc
            .create_todo_item("test todo")
            .await
            .expect("create todo failed");

        let mut meeting_svc = MeetingService::new(conn.clone());
        let meeting = meeting_svc
            .create_meeting("test meeting", Utc::now())
            .await
            .expect("create meeting failed");

        let mut reflection_svc = ReflectionService::new(conn.clone(), root_path.clone());
        let reflection = reflection_svc
            .create_reflection(None)
            .await
            .expect("create reflection failed");

        let mut note_svc = NoteService::new(conn.clone(), root_path.clone());
        let note = note_svc
            .create_note(None)
            .await
            .expect("create note failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        assert!(entries.len() >= 4);

        let mut found_todo = false;
        let mut found_meeting = false;
        let mut found_reflection = false;
        let mut found_note = false;

        for entry in &entries {
            match &entry.entity {
                Some(TimelineEntity::TodoItem(item)) if item.id == todo.id => {
                    found_todo = true;
                    assert_eq!(item.label, "test todo");
                }
                Some(TimelineEntity::Meeting(m)) if m.id == meeting.id => {
                    found_meeting = true;
                    assert_eq!(m.name, "test meeting");
                }
                Some(TimelineEntity::Reflection(r)) if r.reflection.id == reflection.id => {
                    found_reflection = true;
                }
                Some(TimelineEntity::Note(n)) if n.note.id == note.id => {
                    found_note = true;
                }
                _ => {}
            }
        }

        assert!(found_todo, "should find todo item");
        assert!(found_meeting, "should find meeting");
        assert!(found_reflection, "should find reflection");
        assert!(found_note, "should find note");
    }

    #[tokio::test]
    async fn get_timeline_includes_file_content() {
        let (svc, conn, root_path, _root_dir) = setup().await;

        let mut reflection_svc = ReflectionService::new(conn.clone(), root_path.clone());
        let reflection = reflection_svc
            .create_reflection(None)
            .await
            .expect("create reflection failed");

        let full_path = root_path.join(&reflection.file_path);
        fs::write(&full_path, "reflection content")
            .await
            .expect("write failed");

        let mut note_svc = NoteService::new(conn.clone(), root_path.clone());
        let note = note_svc
            .create_note(None)
            .await
            .expect("create note failed");

        let note_full_path = root_path.join(&note.file_path);
        fs::write(&note_full_path, "note content")
            .await
            .expect("write failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        let mut found_reflection_content = false;
        let mut found_note_content = false;

        for entry in &entries {
            match &entry.entity {
                Some(TimelineEntity::Reflection(r)) if r.reflection.id == reflection.id => {
                    found_reflection_content = true;
                    assert_eq!(r.content, Some("reflection content".to_string()));
                }
                Some(TimelineEntity::Note(n)) if n.note.id == note.id => {
                    found_note_content = true;
                    assert_eq!(n.content, Some("note content".to_string()));
                }
                _ => {}
            }
        }

        assert!(
            found_reflection_content,
            "should find reflection with content"
        );
        assert!(found_note_content, "should find note with content");
    }

    #[tokio::test]
    async fn get_timeline_handles_missing_file() {
        let (svc, conn, root_path, _root_dir) = setup().await;

        let mut reflection_svc = ReflectionService::new(conn.clone(), root_path.clone());
        let reflection = reflection_svc
            .create_reflection(None)
            .await
            .expect("create reflection failed");

        let full_path = root_path.join(&reflection.file_path);
        fs::write(&full_path, "content to delete")
            .await
            .expect("write failed");

        fs::remove_file(&full_path).await.expect("remove failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        let mut found_reflection = false;
        for entry in &entries {
            if let Some(TimelineEntity::Reflection(r)) = &entry.entity
                && r.reflection.id == reflection.id
            {
                found_reflection = true;
                assert_eq!(r.content, None);
            }
        }

        assert!(
            found_reflection,
            "should find reflection even with missing file"
        );
    }

    #[tokio::test]
    async fn get_timeline_handles_deleted_entity() {
        let (svc, mut conn, _root_path, _root_dir) = setup().await;

        let fake_id = Uuid::now_v7();
        let tx = conn.transaction().await.expect("tx begin failed");
        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (
                Uuid::now_v7().to_string(),
                fake_id.to_string(),
                "TODO_ITEM_CREATED",
                "{}",
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
            ),
        )
        .await
        .expect("insert failed");
        tx.commit().await.expect("commit failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        let mut found_fake_event = false;
        for entry in &entries {
            if entry.entity_id == fake_id {
                found_fake_event = true;
                assert!(
                    entry.entity.is_none(),
                    "entity should be null for deleted entity"
                );
            }
        }

        assert!(
            found_fake_event,
            "should find event referencing deleted entity"
        );
    }

    #[tokio::test]
    async fn get_timeline_orders_oldest_first() {
        let (svc, mut conn, _root_path, _root_dir) = setup().await;

        let now = Utc::now();
        let t1 = now - Duration::minutes(30);
        let t2 = now - Duration::minutes(20);
        let t3 = now - Duration::minutes(10);

        let tx = conn.transaction().await.expect("tx begin failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), Uuid::now_v7().to_string(), "TODO_ITEM_CREATED", "{}", t3.to_rfc3339(), t3.to_rfc3339()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), Uuid::now_v7().to_string(), "TODO_ITEM_CREATED", "{}", t1.to_rfc3339(), t1.to_rfc3339()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), Uuid::now_v7().to_string(), "TODO_ITEM_CREATED", "{}", t2.to_rfc3339(), t2.to_rfc3339()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[0]
                .created_at
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            t1.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            entries[1]
                .created_at
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            t2.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            entries[2]
                .created_at
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            t3.format("%Y-%m-%d %H:%M:%S").to_string()
        );
    }

    #[tokio::test]
    async fn get_timeline_excludes_out_of_range() {
        let (svc, mut conn, _root_path, _root_dir) = setup().await;

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let tx = conn.transaction().await.expect("tx begin failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), Uuid::now_v7().to_string(), "TODO_ITEM_CREATED", "{}", (start - Duration::hours(2)).to_rfc3339(), (start - Duration::hours(2)).to_rfc3339()),
        ).await.expect("insert e1 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), Uuid::now_v7().to_string(), "TODO_ITEM_CREATED", "{}", (start + Duration::minutes(30)).to_rfc3339(), (start + Duration::minutes(30)).to_rfc3339()),
        ).await.expect("insert e2 failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), Uuid::now_v7().to_string(), "TODO_ITEM_CREATED", "{}", (end + Duration::hours(1)).to_rfc3339(), (end + Duration::hours(1)).to_rfc3339()),
        ).await.expect("insert e3 failed");

        tx.commit().await.expect("commit failed");

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn get_timeline_empty_range() {
        let (svc, _conn, _root_path, _root_dir) = setup().await;

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn get_timeline_status_change_event_has_todo_entity() {
        let (svc, conn, _root_path, _root_dir) = setup().await;

        let mut todo_svc = TodoService::new(conn.clone());
        let todo = todo_svc
            .create_todo_item("status test")
            .await
            .expect("create failed");
        todo_svc
            .update_todo_status(todo.id, crate::models::todo_item::TodoStatus::InProgress)
            .await
            .expect("update failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        let mut found_status_change = false;
        for entry in &entries {
            if entry.event_type == EventType::TodoItemStatusChanged {
                found_status_change = true;
                assert!(
                    matches!(&entry.entity, Some(TimelineEntity::TodoItem(item)) if item.id == todo.id),
                    "status change event should have todo entity"
                );
            }
        }

        assert!(found_status_change, "should find status change event");
    }

    #[tokio::test]
    async fn get_timeline_resolves_summary_entity_with_file_content() {
        let (svc, mut conn, root_path, _root_dir) = setup().await;

        let summary_id = Uuid::now_v7();
        let ts = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let tx = conn.transaction().await.expect("tx begin failed");
        tx.execute(
            "INSERT INTO summary (summary_id, file_path, start, \"end\", created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (summary_id.to_string(), "test_summary.md".to_string(), ts.clone(), ts.clone(), ts.clone(), ts.clone()),
        ).await.expect("insert summary failed");

        tx.execute(
            "INSERT INTO event (event_id, entity_id, event_type, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            (Uuid::now_v7().to_string(), summary_id.to_string(), "SUMMARY_CREATED", "{}", ts.clone(), ts.clone()),
        ).await.expect("insert event failed");
        tx.commit().await.expect("commit failed");

        fs::write(root_path.join("test_summary.md"), "summary content")
            .await
            .expect("write failed");

        let now = Utc::now();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);

        let entries = svc
            .get_timeline(start, end)
            .await
            .expect("get_timeline failed");

        let mut found_summary = false;
        for entry in &entries {
            if entry.event_type == EventType::SummaryCreated
                && let Some(TimelineEntity::Summary(s)) = &entry.entity
            {
                found_summary = true;
                assert_eq!(s.summary.id, summary_id);
                assert_eq!(s.content, Some("summary content".to_string()));
            }
        }

        assert!(found_summary, "should find summary with content");
    }

    #[tokio::test]
    async fn get_entity_timeline_returns_direct_events_for_todo() {
        let (svc, conn, _root_path, _root_dir) = setup().await;

        let mut todo_svc = TodoService::new(conn.clone());
        let todo = todo_svc
            .create_todo_item("test todo")
            .await
            .expect("create todo failed");

        let entries = svc
            .get_entity_timeline(todo.id)
            .await
            .expect("get_entity_timeline failed");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_type, EventType::TodoItemCreated);
        assert_eq!(entries[0].entity_id, todo.id);
        assert!(
            matches!(&entries[0].entity, Some(TimelineEntity::TodoItem(item)) if item.id == todo.id)
        );
    }

    #[tokio::test]
    async fn get_entity_timeline_includes_linked_reflections() {
        let (svc, conn, root_path, _root_dir) = setup().await;

        let mut todo_svc = TodoService::new(conn.clone());
        let todo = todo_svc
            .create_todo_item("test todo")
            .await
            .expect("create todo failed");

        let mut reflection_svc = ReflectionService::new(conn.clone(), root_path.clone());
        let _reflection = reflection_svc
            .create_reflection(Some(todo.id))
            .await
            .expect("create reflection failed");

        let entries = svc
            .get_entity_timeline(todo.id)
            .await
            .expect("get_entity_timeline failed");

        assert_eq!(entries.len(), 2);
        let mut found_todo_created = false;
        let mut found_reflection_created = false;

        for entry in &entries {
            if entry.event_type == EventType::TodoItemCreated && entry.entity_id == todo.id {
                found_todo_created = true;
            }
            if entry.event_type == EventType::ReflectionCreated {
                found_reflection_created = true;
            }
        }

        assert!(found_todo_created, "should find TodoItemCreated event");
        assert!(
            found_reflection_created,
            "should find ReflectionCreated event"
        );
    }

    #[tokio::test]
    async fn get_entity_timeline_includes_linked_notes() {
        let (svc, conn, root_path, _root_dir) = setup().await;

        let mut todo_svc = TodoService::new(conn.clone());
        let todo = todo_svc
            .create_todo_item("test todo")
            .await
            .expect("create todo failed");

        let mut note_svc = NoteService::new(conn.clone(), root_path.clone());
        let _note = note_svc
            .create_note(Some(todo.id))
            .await
            .expect("create note failed");

        let entries = svc
            .get_entity_timeline(todo.id)
            .await
            .expect("get_entity_timeline failed");

        assert_eq!(entries.len(), 2);
        let mut found_todo_created = false;
        let mut found_note_created = false;

        for entry in &entries {
            if entry.event_type == EventType::TodoItemCreated && entry.entity_id == todo.id {
                found_todo_created = true;
            }
            if entry.event_type == EventType::NoteCreated {
                found_note_created = true;
            }
        }

        assert!(found_todo_created, "should find TodoItemCreated event");
        assert!(found_note_created, "should find NoteCreated event");
    }

    #[tokio::test]
    async fn get_entity_timeline_includes_file_content_for_linked() {
        let (svc, conn, root_path, _root_dir) = setup().await;

        let mut todo_svc = TodoService::new(conn.clone());
        let todo = todo_svc
            .create_todo_item("test todo")
            .await
            .expect("create todo failed");

        let mut reflection_svc = ReflectionService::new(conn.clone(), root_path.clone());
        let reflection = reflection_svc
            .create_reflection(Some(todo.id))
            .await
            .expect("create reflection failed");

        let full_path = root_path.join(&reflection.file_path);
        fs::write(&full_path, "reflection content")
            .await
            .expect("write failed");

        let entries = svc
            .get_entity_timeline(todo.id)
            .await
            .expect("get_entity_timeline failed");

        let mut found_reflection_with_content = false;
        for entry in &entries {
            if let Some(TimelineEntity::Reflection(r)) = &entry.entity {
                if r.reflection.id == reflection.id {
                    found_reflection_with_content = true;
                    assert_eq!(r.content, Some("reflection content".to_string()));
                }
            }
        }

        assert!(
            found_reflection_with_content,
            "should find reflection with file content"
        );
    }

    #[tokio::test]
    async fn get_entity_timeline_for_nonexistent_entity_returns_empty() {
        let (svc, _conn, _root_path, _root_dir) = setup().await;

        let nonexistent = Uuid::now_v7();
        let entries = svc
            .get_entity_timeline(nonexistent)
            .await
            .expect("get_entity_timeline failed");

        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn get_entity_timeline_orders_oldest_first() {
        let (svc, conn, _root_path, _root_dir) = setup().await;

        let mut todo_svc = TodoService::new(conn.clone());
        let todo = todo_svc
            .create_todo_item("test todo")
            .await
            .expect("create todo failed");

        todo_svc
            .update_todo_status(todo.id, crate::models::todo_item::TodoStatus::InProgress)
            .await
            .expect("update status failed");

        let entries = svc
            .get_entity_timeline(todo.id)
            .await
            .expect("get_entity_timeline failed");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].event_type, EventType::TodoItemCreated);
        assert_eq!(entries[1].event_type, EventType::TodoItemStatusChanged);
        assert!(entries[0].created_at <= entries[1].created_at);
    }

    #[tokio::test]
    async fn get_entity_timeline_for_meeting() {
        let (svc, conn, root_path, _root_dir) = setup().await;

        let mut meeting_svc = MeetingService::new(conn.clone());
        let meeting = meeting_svc
            .create_meeting("test meeting", Utc::now())
            .await
            .expect("create meeting failed");

        let mut reflection_svc = ReflectionService::new(conn.clone(), root_path.clone());
        let _reflection = reflection_svc
            .create_reflection(Some(meeting.id))
            .await
            .expect("create reflection failed");

        let mut note_svc = NoteService::new(conn.clone(), root_path.clone());
        let _note = note_svc
            .create_note(Some(meeting.id))
            .await
            .expect("create note failed");

        let entries = svc
            .get_entity_timeline(meeting.id)
            .await
            .expect("get_entity_timeline failed");

        assert_eq!(entries.len(), 3);
        let mut found_meeting_created = false;
        let mut found_reflection_created = false;
        let mut found_note_created = false;

        for entry in &entries {
            if entry.event_type == EventType::MeetingCreated && entry.entity_id == meeting.id {
                found_meeting_created = true;
            }
            if entry.event_type == EventType::ReflectionCreated {
                found_reflection_created = true;
            }
            if entry.event_type == EventType::NoteCreated {
                found_note_created = true;
            }
        }

        assert!(found_meeting_created, "should find MeetingCreated event");
        assert!(
            found_reflection_created,
            "should find ReflectionCreated event"
        );
        assert!(found_note_created, "should find NoteCreated event");
    }
}
