use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tokio::fs::{self, create_dir_all};
use uuid::Uuid;

use crate::models::DiffEntry;
use crate::models::event::EventType;
use crate::models::reflection::Reflection;
use crate::repositories;
use crate::repositories::meeting::MeetingFilter;
use crate::services::editable::EditableEntity;
use crate::services::tag;
use crate::with_conn;
use crate::with_conn_mut;

#[derive(Clone)]
pub struct ReflectionService {
    db_path: PathBuf,
    root_dir: PathBuf,
}

impl ReflectionService {
    pub fn new(db_path: PathBuf, root_dir: PathBuf) -> Self {
        Self { db_path, root_dir }
    }

    pub async fn create_reflection(&mut self, about_id: Option<Uuid>) -> Result<Reflection> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let date_dir = now.format("%Y/%m/%d").to_string();
        let file_name = format!("{}.md", id);
        let relative_path = format!("{}/{}", date_dir, file_name);
        let full_path = self.full_path(&relative_path);

        with_conn_mut!(&self.db_path, |conn| {
            let tx = conn.transaction().await?;
            let reflection =
                repositories::reflection::insert(&tx, id, about_id, &relative_path).await?;
            repositories::event::insert(&tx, reflection.id, &EventType::ReflectionCreated, "{}")
                .await?;

            let dir_path = full_path.parent().expect("full_path has a parent");
            create_dir_all(dir_path).await?;
            fs::write(&full_path, "").await?;

            tx.commit().await?;
            Ok(reflection)
        })
    }

    pub async fn cleanup_reflection(&mut self, id: Uuid) -> Result<()> {
        let reflection = with_conn_mut!(&self.db_path, |conn| {
            let tx = conn.transaction().await?;
            let reflection = repositories::reflection::get_by_id(&tx, id).await?;
            tx.commit().await?;
            Ok(reflection)
        })?;

        let full_path = self.full_path(&reflection.file_path);
        let is_empty_or_missing = match fs::metadata(&full_path).await {
            Ok(meta) => meta.len() == 0,
            Err(_) => true,
        };

        if is_empty_or_missing {
            with_conn_mut!(&self.db_path, |conn| {
                let tx = conn.transaction().await?;
                repositories::reflection::delete(&tx, id).await?;
                repositories::event::delete_by_entity_id(&tx, id).await?;
                tx.commit().await?;
                Ok(())
            })?;

            if let Err(e) = fs::remove_file(&full_path).await
                && e.kind() != std::io::ErrorKind::NotFound
            {
                return Err(e.into());
            }
        } else {
            with_conn_mut!(&self.db_path, |conn| {
                tag::sync_tags_from_file(conn, id, &full_path).await?;
                Ok(())
            })?;
        }

        Ok(())
    }

    pub async fn list_reflections(&self) -> Result<Vec<Reflection>> {
        with_conn!(&self.db_path, |conn| {
            repositories::reflection::list_ordered_by_updated_at(conn).await
        })
    }

    pub async fn resolve_label(&self, reflection: &Reflection) -> Result<String> {
        with_conn!(&self.db_path, |conn| {
            match reflection.about_id {
                None => Ok(format!(
                    "General Reflection {}",
                    reflection.created_at.format("%Y-%m-%d %H:%M")
                )),
                Some(id) => {
                    if let Some(item) = repositories::todo_item::find_by_id(conn, id).await? {
                        return Ok(format!("TODO Item Reflection {}", item.label));
                    }
                    if let Some(meeting) =
                        repositories::meeting::find_one(conn, &MeetingFilter::new().id(id)).await?
                    {
                        return Ok(format!("Meeting Reflection {}", meeting.name));
                    }
                    Ok(format!(
                        "General Reflection {}",
                        reflection.created_at.format("%Y-%m-%d %H:%M")
                    ))
                }
            }
        })
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
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use chrono::Utc;
    use tempfile::tempdir;

    async fn setup() -> (
        ReflectionService,
        PathBuf,
        PathBuf,
        tempfile::TempDir,
        tempfile::TempDir,
    ) {
        let db_dir = tempdir().expect("create tempdir failed");
        let db_path = db_dir.path().join("test.db");
        let _db = Database::open_path(&db_path).await.expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();
        let service = ReflectionService::new(db_path.clone(), root_path.clone());
        (service, db_path, root_path, db_dir, root_dir)
    }

    async fn query_count(db_path: &PathBuf, sql: &str, params: impl turso::IntoParams) -> i64 {
        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db.conn().query(sql, params).await.expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        while rows.next().await.expect("fetch failed").is_some() {}
        row.get(0).expect("get count")
    }

    #[tokio::test]
    async fn create_reflection_inserts_row_and_event() {
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM reflection WHERE reflection_id = ?",
            [reflection.id.to_string()],
        )
        .await;
        assert_eq!(count, 1);

        let count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM event WHERE entity_id = ? AND event_type = 'REFLECTION_CREATED'",
            [reflection.id.to_string()],
        )
        .await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn create_reflection_with_null_about_id() {
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        assert!(reflection.about_id.is_none());
    }

    #[tokio::test]
    async fn create_reflection_creates_directory() {
        let (mut svc, _db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let _reflection = svc.create_reflection(None).await.expect("create failed");

        let now = Utc::now();
        let expected_dir = root_dir.join(now.format("%Y/%m/%d").to_string());
        assert!(expected_dir.exists());
    }

    #[tokio::test]
    async fn create_reflection_creates_empty_file() {
        let (mut svc, _db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let full_path = root_dir.join(&reflection.file_path);
        assert!(full_path.exists());
        let meta = fs::metadata(&full_path).await.expect("metadata failed");
        assert_eq!(meta.len(), 0);
    }

    #[tokio::test]
    async fn cleanup_reflection_deletes_row_and_file() {
        let (mut svc, db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let full_path = root_dir.join(&reflection.file_path);
        assert!(full_path.exists());

        svc.cleanup_reflection(reflection.id)
            .await
            .expect("cleanup failed");

        let count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM reflection WHERE reflection_id = ?",
            [reflection.id.to_string()],
        )
        .await;
        assert_eq!(count, 0);

        let count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM event WHERE entity_id = ?",
            [reflection.id.to_string()],
        )
        .await;
        assert_eq!(count, 0);

        assert!(!full_path.exists());
    }

    #[tokio::test]
    async fn cleanup_reflection_preserves_non_empty_file() {
        let (mut svc, db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

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

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let tags = repositories::tag::find_tags_for_entity(db.conn(), reflection.id)
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
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

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
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let mut db = Database::open_path(&db_path).await.expect("db open failed");
        let tx = db.conn_mut().transaction().await.expect("tx begin failed");
        let todo = repositories::todo_item::insert(&tx, "Test Todo")
            .await
            .expect("insert todo failed");
        tx.commit().await.expect("commit failed");
        drop(db);

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
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let mut db = Database::open_path(&db_path).await.expect("db open failed");
        let tx = db.conn_mut().transaction().await.expect("tx begin failed");
        let meeting = repositories::meeting::insert(&tx, "Test Meeting", Utc::now())
            .await
            .expect("insert meeting failed");
        tx.commit().await.expect("commit failed");
        drop(db);

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
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        let label = svc
            .resolve_label(&reflection)
            .await
            .expect("resolve failed");
        assert!(label.starts_with("General Reflection"));
    }

    #[tokio::test]
    async fn cleanup_with_tags_preserves_entity_and_links_tags() {
        let (mut svc, db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

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

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let entity_tags = repositories::tag::find_tags_for_entity(db.conn(), reflection.id)
            .await
            .expect("find_tags_for_entity failed");
        assert_eq!(entity_tags.len(), 2);
        let labels: Vec<&str> = entity_tags.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"alpha-project"));
        assert!(labels.contains(&"beta"));
    }

    #[tokio::test]
    async fn cleanup_empty_deletes_entity_no_tag_rows() {
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let reflection = svc.create_reflection(None).await.expect("create failed");

        svc.cleanup_reflection(reflection.id)
            .await
            .expect("cleanup failed");

        let reflections = svc.list_reflections().await.expect("list failed");
        assert_eq!(reflections.len(), 0);

        let tag_count = query_count(&db_path, "SELECT COUNT(*) FROM tag", ()).await;
        assert_eq!(tag_count, 0);

        let entity_tag_count = query_count(&db_path, "SELECT COUNT(*) FROM entity_tag", ()).await;
        assert_eq!(entity_tag_count, 0);
    }

    #[tokio::test]
    async fn create_reflection_rolls_back_on_file_write_failure() {
        let db_dir = tempdir().expect("create tempdir failed");
        let db_path = db_dir.path().join("test.db");
        let _db = Database::open_path(&db_path).await.expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();

        let fake_root = root_path.join("blocker");
        fs::write(&fake_root, "not a directory")
            .await
            .expect("write failed");

        let mut svc = ReflectionService::new(db_path.clone(), fake_root);

        let result = svc.create_reflection(None).await;
        assert!(result.is_err());

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db
            .conn()
            .query("SELECT reflection_id FROM reflection", ())
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_none());

        let mut rows = db
            .conn()
            .query("SELECT event_id FROM event", ())
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_none());
    }
}
