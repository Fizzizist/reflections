use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tokio::fs::{self, create_dir_all};
use turso::Connection;
use uuid::Uuid;

use crate::models::DiffEntry;
use crate::models::event::EventType;
use crate::models::reflection::Reflection;
use crate::repositories;
use crate::repositories::meeting::MeetingFilter;
use crate::services::editable::EditableEntity;
use crate::services::tag;

#[derive(Clone)]
pub struct ReflectionService {
    conn: Connection,
    root_dir: PathBuf,
}

impl ReflectionService {
    pub fn new(conn: Connection, root_dir: PathBuf) -> Self {
        Self { conn, root_dir }
    }

    pub async fn create_reflection(&mut self, about_id: Option<Uuid>) -> Result<Reflection> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let date_dir = now.format("%Y/%m/%d").to_string();
        let file_name = format!("{}.md", id);
        let relative_path = format!("{}/{}", date_dir, file_name);
        let full_path = self.full_path(&relative_path);

        let tx = self.conn.transaction().await?;
        let reflection =
            repositories::reflection::insert(&tx, id, about_id, &relative_path).await?;
        repositories::event::insert(&tx, reflection.id, &EventType::ReflectionCreated, "{}")
            .await?;
        tx.commit().await?;

        let dir_path = full_path.parent().expect("full_path has a parent");
        create_dir_all(dir_path).await?;
        fs::write(&full_path, "").await?;

        Ok(reflection)
    }

    pub async fn cleanup_reflection(&mut self, id: Uuid) -> Result<()> {
        let reflection = {
            let tx = self.conn.transaction().await?;
            let reflection = repositories::reflection::get_by_id(&tx, id).await?;
            tx.commit().await?;
            reflection
        };

        let full_path = self.full_path(&reflection.file_path);
        let is_empty_or_missing = match fs::metadata(&full_path).await {
            Ok(meta) => meta.len() == 0,
            Err(_) => true,
        };

        if is_empty_or_missing {
            let tx = self.conn.transaction().await?;
            repositories::reflection::delete(&tx, id).await?;
            repositories::event::delete_by_entity_id(&tx, id).await?;
            tx.commit().await?;

            if let Err(e) = fs::remove_file(&full_path).await
                && e.kind() != std::io::ErrorKind::NotFound
            {
                return Err(e.into());
            }
        } else {
            tag::sync_tags_from_file(&mut self.conn, id, &full_path).await?;
        }

        Ok(())
    }

    pub async fn list_reflections(&self) -> Result<Vec<Reflection>> {
        repositories::reflection::list_ordered_by_updated_at(&self.conn).await
    }

    pub async fn resolve_label(&self, reflection: &Reflection) -> Result<String> {
        match reflection.about_id {
            None => Ok(format!(
                "General Reflection {}",
                reflection.created_at.format("%Y-%m-%d %H:%M")
            )),
            Some(id) => {
                if let Some(item) = repositories::todo_item::find_by_id(&self.conn, id).await? {
                    return Ok(format!("TODO Item Reflection {}", item.label));
                }
                if let Some(meeting) =
                    repositories::meeting::find_one(&self.conn, &MeetingFilter::new().id(id))
                        .await?
                {
                    return Ok(format!("Meeting Reflection {}", meeting.name));
                }
                Ok(format!(
                    "General Reflection {}",
                    reflection.created_at.format("%Y-%m-%d %H:%M")
                ))
            }
        }
    }

    pub fn full_path(&self, file_path: &str) -> std::path::PathBuf {
        self.root_dir.join(file_path)
    }
}

impl EditableEntity for ReflectionService {
    type Entity = Reflection;

    async fn create(&mut self, related_id: Option<Uuid>) -> Result<Reflection> {
        self.create_reflection(related_id).await
    }

    fn full_path(&self, file_path: &str) -> PathBuf {
        ReflectionService::full_path(self, file_path)
    }

    async fn cleanup(&mut self, id: Uuid) -> Result<()> {
        self.cleanup_reflection(id).await
    }

    async fn post_edit(&mut self, _id: Uuid, _diff: Vec<DiffEntry>) -> Result<()> {
        // emit updated event and touch reflection
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::Utc;
    use tempfile::tempdir;

    async fn setup() -> (ReflectionService, PathBuf) {
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
        let service = ReflectionService::new(conn, root_path.clone());
        (service, root_path)
    }

    #[tokio::test]
    async fn create_reflection_inserts_row_and_event() {
        let (mut svc, _root_dir) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let mut rows = svc
            .conn
            .query(
                "SELECT reflection_id FROM reflection WHERE reflection_id = ?",
                [reflection.id.to_string()],
            )
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_some());

        let mut rows = svc
            .conn
            .query("SELECT event_type FROM event WHERE entity_id = ? AND event_type = 'REFLECTION_CREATED'", [reflection.id.to_string()])
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_some());
    }

    #[tokio::test]
    async fn create_reflection_with_null_about_id() {
        let (mut svc, _root_dir) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        assert!(reflection.about_id.is_none());
    }

    #[tokio::test]
    async fn create_reflection_creates_directory() {
        let (mut svc, root_dir) = setup().await;

        let _reflection = svc.create_reflection(None).await.expect("create failed");

        let now = Utc::now();
        let expected_dir = root_dir.join(now.format("%Y/%m/%d").to_string());
        assert!(expected_dir.exists());
    }

    #[tokio::test]
    async fn create_reflection_creates_empty_file() {
        let (mut svc, root_dir) = setup().await;

        let _reflection = svc.create_reflection(None).await.expect("create failed");

        let full_path = root_dir.join(&_reflection.file_path);
        assert!(full_path.exists());
        let meta = fs::metadata(&full_path).await.expect("metadata failed");
        assert_eq!(meta.len(), 0);
    }

    #[tokio::test]
    async fn cleanup_reflection_deletes_row_and_file() {
        let (mut svc, root_dir) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let full_path = root_dir.join(&reflection.file_path);
        assert!(full_path.exists());

        svc.cleanup_reflection(reflection.id)
            .await
            .expect("cleanup failed");

        let mut rows = svc
            .conn
            .query(
                "SELECT reflection_id FROM reflection WHERE reflection_id = ?",
                [reflection.id.to_string()],
            )
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_none());

        let mut rows = svc
            .conn
            .query(
                "SELECT event_id FROM event WHERE entity_id = ?",
                [reflection.id.to_string()],
            )
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_none());

        assert!(!full_path.exists());
    }

    #[tokio::test]
    async fn cleanup_reflection_preserves_non_empty_file() {
        let (mut svc, root_dir) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let full_path = root_dir.join(&reflection.file_path);
        fs::write(&full_path, "reflection content")
            .await
            .expect("write failed");

        svc.cleanup_reflection(reflection.id)
            .await
            .expect("cleanup failed");

        let reflections = svc.list_reflections().await.expect("list failed");
        assert_eq!(reflections.len(), 1);
        assert_eq!(reflections[0].id, reflection.id);

        let content = fs::read_to_string(&full_path).await.expect("read failed");
        assert_eq!(content, "reflection content");

        let tags = repositories::tag::find_tags_for_entity(&svc.conn, reflection.id)
            .await
            .expect("find_tags_for_entity failed");
        assert_eq!(
            tags.len(),
            0,
            "no tags should be linked for content without tags"
        );
    }

    #[tokio::test]
    async fn list_reflections_ordered_by_updated_at() {
        let (mut svc, _root_dir) = setup().await;

        let r1 = svc.create_reflection(None).await.expect("create r1 failed");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let r2 = svc.create_reflection(None).await.expect("create r2 failed");

        let reflections = svc.list_reflections().await.expect("list failed");

        assert_eq!(reflections.len(), 2);
        assert_eq!(reflections[0].id, r2.id);
        assert_eq!(reflections[1].id, r1.id);
    }

    #[tokio::test]
    async fn resolve_label_for_todo_reflection() {
        let (mut svc, _root_dir) = setup().await;

        let mut conn = svc.conn.clone();
        let tx = conn.transaction().await.expect("tx begin failed");
        let todo = repositories::todo_item::insert(&tx, "Test Todo")
            .await
            .expect("insert todo failed");
        tx.commit().await.expect("commit failed");

        let reflection = svc
            .create_reflection(Some(todo.id))
            .await
            .expect("create failed");

        let label = svc
            .resolve_label(&reflection)
            .await
            .expect("resolve failed");
        assert!(label.starts_with("TODO Item Reflection"));
        assert!(label.contains("Test Todo"));
    }

    #[tokio::test]
    async fn resolve_label_for_meeting_reflection() {
        let (mut svc, _root_dir) = setup().await;

        let mut conn = svc.conn.clone();
        let tx = conn.transaction().await.expect("tx begin failed");
        let meeting = repositories::meeting::insert(&tx, "Test Meeting", Utc::now())
            .await
            .expect("insert meeting failed");
        tx.commit().await.expect("commit failed");

        let reflection = svc
            .create_reflection(Some(meeting.id))
            .await
            .expect("create failed");

        let label = svc
            .resolve_label(&reflection)
            .await
            .expect("resolve failed");
        assert!(label.starts_with("Meeting Reflection"));
        assert!(label.contains("Test Meeting"));
    }

    #[tokio::test]
    async fn resolve_label_for_general_reflection() {
        let (mut svc, _root_dir) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let label = svc
            .resolve_label(&reflection)
            .await
            .expect("resolve failed");
        assert!(label.starts_with("General Reflection"));
    }

    #[tokio::test]
    async fn cleanup_with_tags_preserves_entity_and_links_tags() {
        let (mut svc, root_dir) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let full_path = root_dir.join(&reflection.file_path);
        fs::write(&full_path, "Working on #alpha-project and #beta")
            .await
            .expect("write failed");

        svc.cleanup_reflection(reflection.id)
            .await
            .expect("cleanup failed");

        let reflections = svc.list_reflections().await.expect("list failed");
        assert_eq!(reflections.len(), 1);
        assert_eq!(reflections[0].id, reflection.id);

        let entity_tags = repositories::tag::find_tags_for_entity(&svc.conn, reflection.id)
            .await
            .expect("find_tags_for_entity failed");
        assert_eq!(entity_tags.len(), 2);
        let labels: Vec<&str> = entity_tags.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"alpha-project"));
        assert!(labels.contains(&"beta"));
    }

    #[tokio::test]
    async fn cleanup_empty_deletes_entity_no_tag_rows() {
        let (mut svc, _root_dir) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        svc.cleanup_reflection(reflection.id)
            .await
            .expect("cleanup failed");

        let reflections = svc.list_reflections().await.expect("list failed");
        assert_eq!(reflections.len(), 0);

        let mut rows = svc
            .conn
            .query("SELECT COUNT(*) FROM tag", ())
            .await
            .expect("query failed");
        let row = rows.next().await.expect("fetch failed").expect("no rows");
        while rows.next().await.expect("fetch failed").is_some() {}
        let count: i64 = row.get(0).expect("get count failed");
        assert_eq!(count, 0);

        let mut rows = svc
            .conn
            .query("SELECT COUNT(*) FROM entity_tag", ())
            .await
            .expect("query failed");
        let row = rows.next().await.expect("fetch failed").expect("no rows");
        while rows.next().await.expect("fetch failed").is_some() {}
        let count: i64 = row.get(0).expect("get count failed");
        assert_eq!(count, 0);
    }
}
