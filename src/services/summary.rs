use anyhow::Result;
use chrono::{DateTime, Utc};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tokio::fs::{self, create_dir_all};
use uuid::Uuid;

use crate::models::DiffEntry;
use crate::models::event::EventType;
use crate::models::summary::Summary;
use crate::repositories;
use crate::repositories::summary::SummaryFilter;
use crate::services::editable::EditableEntity;
use crate::services::tag;
use crate::with_conn;
use crate::with_txn;

#[derive(Clone)]
pub struct SummaryService {
    db_path: PathBuf,
    root_dir: PathBuf,
}

impl SummaryService {
    pub fn new(db_path: PathBuf, root_dir: PathBuf) -> Self {
        Self { db_path, root_dir }
    }

    pub async fn create_summary(
        &mut self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        content: &str,
    ) -> Result<Summary> {
        if content.is_empty() {
            return Err(anyhow::anyhow!("content cannot be empty"));
        }

        let id = Uuid::now_v7();
        let now = Utc::now();
        let date_dir = now.format("%Y/%m/%d").to_string();
        let file_name = format!("{}.md", id);
        let relative_path = format!("{}/{}", date_dir, file_name);
        let full_path = self.full_path(&relative_path);

        with_txn!(&self.db_path, |tx| {
            let summary =
                repositories::summary::insert(&tx, id, &relative_path, start, end).await?;
            repositories::event::insert(&tx, summary.id, &EventType::SummaryCreated, "{}").await?;

            let dir_path = full_path.parent().expect("full_path has a parent");
            create_dir_all(dir_path).await?;
            fs::write(&full_path, content).await?;

            let labels = tag::extract_tags(content);
            if !labels.is_empty() {
                tag::sync_tags(&tx, summary.id, &labels).await?;
            }

            Ok(summary)
        })
    }

    pub fn full_path(&self, file_path: &str) -> PathBuf {
        self.root_dir.join(file_path)
    }

    pub async fn list_summaries(&self) -> Result<Vec<Summary>> {
        with_conn!(&self.db_path, |conn| {
            repositories::summary::list_summaries(conn).await
        })
    }

    pub async fn get_title(&self, summary: &Summary) -> Result<String> {
        let file = File::open(self.root_dir.join(&summary.file_path))?;
        let mut reader = BufReader::new(file);
        let mut title = String::new();
        reader.read_line(&mut title)?;
        Ok(title)
    }

    pub async fn get_content(&self, summary_id: Uuid) -> Result<String> {
        with_conn!(&self.db_path, |conn| {
            async {
                if let Some(summary) =
                    repositories::summary::find_one(conn, &SummaryFilter::new().id(summary_id))
                        .await?
                {
                    let full_path = self.root_dir.join(summary.file_path.clone());
                    if let Ok(content) = fs::read_to_string(&full_path).await {
                        return Ok(content);
                    }
                    return Err(anyhow::anyhow!(format!(
                        "Unable to retrieve summary content: No content at {}.",
                        summary.file_path
                    )));
                }
                Err(anyhow::anyhow!(
                    "Unable to retrieve summary content: Summary not found."
                ))
            }
            .await
        })
    }

    pub async fn post_edit_summary(
        &mut self,
        summary_id: &Uuid,
        diff: Vec<DiffEntry>,
    ) -> Result<()> {
        with_txn!(&self.db_path, |tx| {
            repositories::summary::touch(&tx, summary_id).await?;
            let metadata = serde_json::to_string(&diff)?;
            repositories::event::insert(&tx, *summary_id, &EventType::SummaryUpdated, &metadata)
                .await?;
            Ok(())
        })
    }
}

impl EditableEntity for SummaryService {
    type Entity = Summary;

    async fn create(&mut self, _related_id: Option<Uuid>) -> Result<Summary> {
        Err(anyhow::anyhow!(
            "Summary can't be shouldn't be created this way"
        ))
    }

    fn full_path(&self, file_path: &str) -> PathBuf {
        SummaryService::full_path(self, file_path)
    }

    async fn cleanup(&mut self, _id: Uuid) -> Result<()> {
        // right now we don't have a code path that needs this, but we may do a creation through
        // TUI in the future
        todo!();
    }

    async fn post_edit(&mut self, id: Uuid, diff: Vec<DiffEntry>) -> Result<()> {
        self.post_edit_summary(&id, diff).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::repositories;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    async fn setup() -> (
        SummaryService,
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
        let service = SummaryService::new(db_path.clone(), root_path.clone());
        (service, db_path, root_path, db_dir, root_dir)
    }

    #[tokio::test]
    async fn create_summary_inserts_row_and_event() {
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let summary = svc
            .create_summary(start, end, "test content")
            .await
            .expect("create failed");

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db
            .conn()
            .query(
                "SELECT summary_id FROM summary WHERE summary_id = ?",
                [summary.id.to_string()],
            )
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_some());

        let mut rows = db
            .conn()
            .query(
                "SELECT event_type FROM event WHERE entity_id = ? AND event_type = 'SUMMARY_CREATED'",
                [summary.id.to_string()],
            )
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_some());
    }

    #[tokio::test]
    async fn create_summary_writes_content_to_file() {
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let content = "This is test summary content";
        let summary = svc
            .create_summary(start, end, content)
            .await
            .expect("create failed");

        let full_path = svc.full_path(&summary.file_path);
        assert!(full_path.exists());
        let file_content = fs::read_to_string(&full_path).await.expect("read failed");
        assert_eq!(file_content, content);
    }

    #[tokio::test]
    async fn create_summary_creates_directory_structure() {
        let (mut svc, _db_path, root_dir, _db_dir, _root_dir_temp) = setup().await;

        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let _summary = svc
            .create_summary(start, end, "test content")
            .await
            .expect("create failed");

        let now = Utc::now();
        let expected_dir = root_dir.join(now.format("%Y/%m/%d").to_string());
        assert!(expected_dir.exists());
    }

    #[tokio::test]
    async fn create_summary_with_empty_content_returns_error() {
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let result = svc.create_summary(start, end, "").await;

        assert!(result.is_err());
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("content cannot be empty"));

        let db = Database::open_path(&_db_path)
            .await
            .expect("db open failed");
        let mut rows = db
            .conn()
            .query("SELECT summary_id FROM summary", ())
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

    #[tokio::test]
    async fn create_summary_file_path_uses_current_time_not_provided_timestamps() {
        let (mut svc, _db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let start = Utc.with_ymd_and_hms(2020, 1, 1, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2020, 1, 1, 11, 0, 0).unwrap();
        let summary = svc
            .create_summary(start, end, "test content")
            .await
            .expect("create failed");

        let now = Utc::now();
        let expected_date_dir = now.format("%Y/%m/%d").to_string();
        assert!(summary.file_path.starts_with(&expected_date_dir));

        let full_path = svc.full_path(&summary.file_path);
        assert!(full_path.exists());
    }

    #[tokio::test]
    async fn create_summary_rolls_back_on_file_write_failure() {
        let dir = tempdir().expect("create tempdir failed");
        let db_path = dir.path().join("test.db");
        let _db = Database::open_path(&db_path).await.expect("db open failed");

        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();

        let fake_root = root_path.join("blocker");
        fs::write(&fake_root, "not a directory")
            .await
            .expect("write failed");

        let mut svc = SummaryService::new(db_path.clone(), fake_root);

        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let result = svc.create_summary(start, end, "test content").await;

        assert!(result.is_err());

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let mut rows = db
            .conn()
            .query("SELECT summary_id FROM summary", ())
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

    #[tokio::test]
    async fn create_summary_extracts_tags() {
        let (mut svc, db_path, _root_dir, _db_dir, _root_dir_temp) = setup().await;

        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let content = "Summary with #alpha and #beta-project tags";
        let summary = svc
            .create_summary(start, end, content)
            .await
            .expect("create failed");

        let db = Database::open_path(&db_path).await.expect("db open failed");
        let tags = repositories::tag::find_tags_for_entity(db.conn(), summary.id)
            .await
            .expect("find_tags_for_entity failed");

        assert_eq!(tags.len(), 2);
        let labels: Vec<String> = tags.iter().map(|t| t.label.clone()).collect();
        assert!(labels.contains(&"alpha".to_string()));
        assert!(labels.contains(&"beta-project".to_string()));
    }
}
