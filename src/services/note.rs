use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tokio::fs::{self, create_dir_all};
use uuid::Uuid;

use crate::models::DiffEntry;
use crate::models::event::EventType;
use crate::models::note::Note;
use crate::repositories;
use crate::services::editable::EditableEntity;
use crate::services::tag;
use crate::with_conn_mut;

#[derive(Clone)]
pub struct NoteService {
    db_path: PathBuf,
    root_dir: PathBuf,
}

impl NoteService {
    pub fn new(db_path: PathBuf, root_dir: PathBuf) -> Self {
        Self { db_path, root_dir }
    }

    pub async fn create_note(&mut self, related_to_id: Option<Uuid>) -> Result<Note> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let date_dir = now.format("%Y/%m/%d").to_string();
        let file_name = format!("{}.md", id);
        let relative_path = format!("{}/{}", date_dir, file_name);
        let full_path = self.full_path(&relative_path);

        with_conn_mut!(&self.db_path, |conn| {
            let tx = conn.transaction().await?;
            let note = repositories::note::insert(&tx, id, related_to_id, &relative_path).await?;
            repositories::event::insert(&tx, note.id, &EventType::NoteCreated, "{}").await?;

            let dir_path = full_path.parent().expect("full_path has a parent");
            create_dir_all(dir_path).await?;
            fs::write(&full_path, "").await?;

            tx.commit().await?;
            Ok(note)
        })
    }

    pub async fn cleanup_note(&mut self, id: Uuid) -> Result<()> {
        let (_file_path, full_path) = with_conn_mut!(&self.db_path, |conn| {
            let tx = conn.transaction().await?;
            let note = repositories::note::get_by_id(&tx, id).await?;
            let file_path = note.file_path.clone();
            tx.commit().await?;
            let full_path = self.full_path(&file_path);
            Ok((file_path, full_path))
        })?;

        let is_empty_or_missing = match fs::metadata(&full_path).await {
            Ok(meta) => meta.len() == 0,
            Err(_) => true,
        };

        if is_empty_or_missing {
            with_conn_mut!(&self.db_path, |conn| {
                let tx = conn.transaction().await?;
                repositories::note::delete(&tx, id).await?;
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

    pub fn full_path(&self, file_path: &str) -> std::path::PathBuf {
        self.root_dir.join(file_path)
    }

    #[cfg(test)]
    pub async fn note_count(&self) -> i64 {
        let db = crate::db::Database::open_path(&self.db_path)
            .await
            .expect("db open failed");
        let mut rows = db
            .conn()
            .query("SELECT COUNT(*) FROM note", ())
            .await
            .expect("query failed");
        let row = rows.next().await.expect("fetch failed").expect("no rows");
        while rows.next().await.expect("fetch failed").is_some() {}
        row.get(0).expect("get count failed")
    }

    #[cfg(test)]
    pub async fn list_notes_for_test(&self) -> Vec<Note> {
        let db = crate::db::Database::open_path(&self.db_path)
            .await
            .expect("db open failed");
        let mut rows = db
            .conn()
            .query(
                "SELECT note_id, related_to_id, file_path FROM note ORDER BY created_at",
                (),
            )
            .await
            .expect("query failed");
        let mut notes = Vec::new();
        while let Some(row) = rows.next().await.expect("fetch failed") {
            let id_str: String = row.get(0).expect("get id failed");
            let id = Uuid::parse_str(&id_str).expect("parse id failed");
            let file_path: String = row.get(2).expect("get file_path failed");
            let related_to_id = match row.get_value(1).expect("get related_to_id failed") {
                turso::Value::Text(s) => {
                    Some(Uuid::parse_str(&s).expect("parse related_to_id failed"))
                }
                turso::Value::Null => None,
                _ => None,
            };
            notes.push(Note {
                id,
                related_to_id,
                file_path,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }
        notes
    }
}

impl EditableEntity for NoteService {
    type Entity = Note;

    async fn create(&mut self, related_id: Option<Uuid>) -> Result<Note> {
        self.create_note(related_id).await
    }

    fn full_path(&self, file_path: &str) -> std::path::PathBuf {
        NoteService::full_path(self, file_path)
    }

    async fn cleanup(&mut self, id: Uuid) -> Result<()> {
        self.cleanup_note(id).await
    }

    async fn post_edit(&mut self, _id: Uuid, _diff: Vec<DiffEntry>) -> Result<()> {
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::tempdir;

    async fn setup() -> (
        NoteService,
        PathBuf,
        PathBuf,
        tempfile::TempDir,
        tempfile::TempDir,
    ) {
        let db_dir = tempdir().expect("create tempdir failed");
        let db_path = db_dir.path().join("test.db");
        let _db = Database::open_path(&db_path).await.expect("db open failed");
        let root_dir = tempdir().expect("create root_dir tempdir failed");
        let root_path = root_dir.path().to_path_buf();
        let service = NoteService::new(db_path.clone(), root_path.clone());
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
    async fn create_note_inserts_row_and_event() {
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        let count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM note WHERE note_id = ?",
            [note.id.to_string()],
        )
        .await;
        assert_eq!(count, 1);

        let event_count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM event WHERE entity_id = ? AND event_type = 'NOTE_CREATED'",
            [note.id.to_string()],
        )
        .await;
        assert_eq!(event_count, 1);
    }

    #[tokio::test]
    async fn create_note_with_null_related_to_id() {
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        assert!(note.related_to_id.is_none());
    }

    #[tokio::test]
    async fn create_note_with_related_to_id() {
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let related_id = Uuid::now_v7();
        let note = svc
            .create_note(Some(related_id))
            .await
            .expect("create failed");

        assert_eq!(note.related_to_id, Some(related_id));
    }

    #[tokio::test]
    async fn create_note_creates_directory() {
        let (mut svc, _db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let _note = svc.create_note(None).await.expect("create failed");

        let now = Utc::now();
        let expected_dir = root_dir.join(now.format("%Y/%m/%d").to_string());
        assert!(expected_dir.exists());
    }

    #[tokio::test]
    async fn create_note_creates_empty_file() {
        let (mut svc, _db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        let full_path = root_dir.join(&note.file_path);
        assert!(full_path.exists());
        let meta = fs::metadata(&full_path).await.expect("metadata failed");
        assert_eq!(meta.len(), 0);
    }

    #[tokio::test]
    async fn cleanup_note_deletes_row_and_file() {
        let (mut svc, db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        let full_path = root_dir.join(&note.file_path);
        assert!(full_path.exists());

        svc.cleanup_note(note.id).await.expect("cleanup failed");

        assert_eq!(svc.note_count().await, 0);

        assert!(!full_path.exists());

        let event_count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM event WHERE entity_id = ?",
            [note.id.to_string()],
        )
        .await;
        assert_eq!(event_count, 0);
    }

    #[tokio::test]
    async fn cleanup_note_preserves_non_empty_file() {
        let (mut svc, db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        let full_path = root_dir.join(&note.file_path);
        fs::write(&full_path, "note content")
            .await
            .expect("write failed");

        svc.cleanup_note(note.id).await.expect("cleanup failed");

        assert_eq!(svc.note_count().await, 1);

        let content = fs::read_to_string(&full_path).await.expect("read failed");
        assert_eq!(content, "note content");

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let tags = repositories::tag::find_tags_for_entity(db.conn(), note.id)
            .await
            .expect("find_tags_for_entity failed");
        assert_eq!(
            tags.len(),
            0,
            "no tags should be linked for content without tags"
        );
    }

    #[tokio::test]
    async fn cleanup_note_succeeds_when_file_missing() {
        let (mut svc, db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        let full_path = root_dir.join(&note.file_path);
        fs::remove_file(&full_path)
            .await
            .expect("remove file failed");

        svc.cleanup_note(note.id)
            .await
            .expect("cleanup should succeed when file is missing");

        assert_eq!(svc.note_count().await, 0);

        let event_count = query_count(
            &db_path,
            "SELECT COUNT(*) FROM event WHERE entity_id = ?",
            [note.id.to_string()],
        )
        .await;
        assert_eq!(event_count, 0);
    }

    #[tokio::test]
    async fn cleanup_note_with_tags_preserves_entity_and_links_tags() {
        let (mut svc, db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        let full_path = root_dir.join(&note.file_path);
        fs::write(&full_path, "Notes about #alpha and #beta-project")
            .await
            .expect("write failed");

        svc.cleanup_note(note.id).await.expect("cleanup failed");

        assert_eq!(svc.note_count().await, 1);

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let tags = repositories::tag::find_tags_for_entity(db.conn(), note.id)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(tags.len(), 2);
        let labels: Vec<&str> = tags.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"alpha"));
        assert!(labels.contains(&"beta-project"));
    }

    #[tokio::test]
    async fn cleanup_note_empty_deletes_entity_no_tag_rows() {
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let note = svc.create_note(None).await.expect("create failed");

        svc.cleanup_note(note.id).await.expect("cleanup failed");

        assert_eq!(svc.note_count().await, 0);

        let tag_count = query_count(&db_path, "SELECT COUNT(*) FROM tag", ()).await;
        assert_eq!(tag_count, 0);

        let entity_tag_count = query_count(&db_path, "SELECT COUNT(*) FROM entity_tag", ()).await;
        assert_eq!(entity_tag_count, 0);
    }

    #[tokio::test]
    async fn create_note_rolls_back_on_file_write_failure() {
        let db_dir = tempdir().expect("create tempdir failed");
        let db_path = db_dir.path().join("test.db");
        let _db = Database::open_path(&db_path).await.expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();

        let fake_root = root_path.join("blocker");
        fs::write(&fake_root, "not a directory")
            .await
            .expect("write failed");

        let mut svc = NoteService::new(db_path.clone(), fake_root);

        let result = svc.create_note(None).await;
        assert!(result.is_err());

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db
            .conn()
            .query("SELECT note_id FROM note", ())
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
