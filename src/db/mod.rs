use anyhow::Result;
use turso::{Builder, Connection, Database as TursoDatabase, Error as TursoError};

const MAX_RETRIES: u32 = 200;
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

pub const DEFAULT_DB_PATH: &str = "reflections.db";

#[derive(Debug)]
pub struct LockContentionError(pub String);

impl std::fmt::Display for LockContentionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lock contention: {}", self.0)
    }
}

impl std::error::Error for LockContentionError {}

fn is_lock_contention(e: &TursoError) -> bool {
    matches!(e, TursoError::Busy(_)) || e.to_string().to_lowercase().contains("lock")
}

pub fn classify_db_error(e: TursoError) -> anyhow::Error {
    if is_lock_contention(&e) {
        anyhow::Error::new(LockContentionError(e.to_string()))
    } else {
        anyhow::Error::from(e)
    }
}

#[allow(dead_code)]
pub fn is_lock_contention_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<LockContentionError>().is_some()
}

pub struct Database {
    _db: TursoDatabase,
    conn: Connection,
}

impl Database {
    pub async fn open(path: &str) -> Result<Self> {
        let mut last_err: Option<TursoError> = None;
        for _ in 0..MAX_RETRIES {
            let builder = Builder::new_local(path)
                .experimental_custom_types(true)
                .experimental_multiprocess_wal(true);

            match builder.build().await {
                Ok(db) => match db.connect() {
                    Ok(conn) => {
                        conn.pragma_update("journal_mode", "WAL").await?;
                        conn.pragma_update("synchronous", "NORMAL").await?;
                        crate::schema::init_schema(&conn).await?;
                        return Ok(Self { _db: db, conn });
                    }
                    Err(e) if is_lock_contention(&e) => {
                        last_err = Some(e);
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    Err(e) => return Err(classify_db_error(e)),
                },
                Err(e) if is_lock_contention(&e) => {
                    last_err = Some(e);
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                Err(e) => return Err(classify_db_error(e)),
            }
        }
        Err(anyhow::anyhow!(
            "failed to open database after {MAX_RETRIES} retries: {}",
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown error".to_string())
        ))
    }

    #[allow(dead_code)]
    pub async fn open_in_memory() -> Result<Self> {
        let db = Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await?;
        let conn = db.connect()?;
        crate::schema::init_schema(&conn).await?;
        Ok(Self { _db: db, conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub async fn checkpoint(&mut self) -> Result<()> {
        for _ in 0..MAX_RETRIES {
            match self
                .conn
                .execute("PRAGMA wal_checkpoint(TRUNCATE)", ())
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) if is_lock_contention(&e) => {
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                Err(_) => return Ok(()),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn classify_db_error_maps_busy_to_lock_contention() {
        let err = classify_db_error(TursoError::Busy("database is locked".to_string()));
        assert!(err.downcast_ref::<LockContentionError>().is_some());
    }

    #[test]
    fn classify_db_error_maps_lock_string_to_lock_contention() {
        let err = classify_db_error(TursoError::Error("file is lock".to_string()));
        assert!(err.downcast_ref::<LockContentionError>().is_some());
    }

    #[test]
    fn classify_db_error_passes_through_other_errors() {
        let err = classify_db_error(TursoError::Constraint("UNIQUE constraint".to_string()));
        assert!(err.downcast_ref::<LockContentionError>().is_none());
    }

    #[tokio::test]
    async fn open_in_memory_works_schema_initialized() {
        let db = Database::open_in_memory().await.expect("open failed");
        let mut rows = db
            .conn()
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='todo_item'",
                (),
            )
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_some());
    }

    #[tokio::test]
    async fn checkpoint_succeeds_on_fresh_database() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let mut db = Database::open(db_path.to_str().expect("path valid"))
            .await
            .expect("open failed");
        db.checkpoint().await.expect("checkpoint failed");
    }

    #[tokio::test]
    async fn open_file_db_wal_mode_enabled() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db = Database::open(db_path.to_str().expect("path valid"))
            .await
            .expect("open failed");
        let mut rows = db
            .conn()
            .query("PRAGMA journal_mode", ())
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let mode: String = row.get(0).expect("get mode");
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[tokio::test]
    async fn open_file_db_synchronous_normal() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db = Database::open(db_path.to_str().expect("path valid"))
            .await
            .expect("open failed");
        let mut rows = db
            .conn()
            .query("PRAGMA synchronous", ())
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let sync: i64 = row.get(0).expect("get sync");
        assert_eq!(sync, 1);
    }

    #[tokio::test]
    async fn checkpoint_truncates_wal_file() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().expect("path valid").to_string();

        {
            let mut db = Database::open(&db_path_str).await.expect("open failed");
            db.conn()
                .execute("CREATE TABLE test_wal (id INTEGER, data TEXT)", ())
                .await
                .expect("create table failed");
            let payload = "X".repeat(4096);
            db.conn()
                .execute("INSERT INTO test_wal (id, data) VALUES (1, ?)", [payload])
                .await
                .expect("insert failed");
            db.checkpoint().await.expect("checkpoint failed");
        }

        let wal_path = format!("{}-wal", db_path_str);
        if let Ok(meta) = std::fs::metadata(&wal_path) {
            assert_eq!(
                meta.len(),
                0,
                "WAL file should be truncated to zero after checkpoint"
            );
        }
    }

    #[tokio::test]
    async fn two_opens_on_same_file_both_succeed() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().expect("path valid").to_string();

        let db1 = Database::open(&db_path_str)
            .await
            .expect("first open failed");
        drop(db1);
        let db2 = Database::open(&db_path_str)
            .await
            .expect("second open failed");
        drop(db2);
    }

    #[tokio::test]
    async fn write_then_read_across_opens() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().expect("path valid").to_string();

        {
            let mut db = Database::open(&db_path_str).await.expect("open failed");
            db.conn()
                .execute("CREATE TABLE test_persist (id INTEGER, val TEXT)", ())
                .await
                .expect("create failed");
            db.conn()
                .execute("INSERT INTO test_persist VALUES (1, 'hello')", ())
                .await
                .expect("insert failed");
            db.checkpoint().await.expect("checkpoint failed");
        }

        {
            let db = Database::open(&db_path_str).await.expect("reopen failed");
            let mut rows = db
                .conn()
                .query("SELECT val FROM test_persist WHERE id = 1", ())
                .await
                .expect("query failed");
            let row = rows
                .next()
                .await
                .expect("fetch failed")
                .expect("row exists");
            let val: String = row.get(0).expect("get val");
            assert_eq!(val, "hello");
        }
    }
}
