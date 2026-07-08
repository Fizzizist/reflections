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

#[macro_export]
macro_rules! with_conn {
    ($db_path:expr, |$conn:ident| $body:expr) => {{
        let db = $crate::db::Database::open_path($db_path).await?;
        let $conn = db.conn();
        $body
    }};
}

#[macro_export]
macro_rules! with_conn_mut {
    ($db_path:expr, |$conn:ident| $body:expr) => {{
        let mut db = $crate::db::Database::open_path($db_path).await?;
        let $conn = db.conn_mut();
        let result: ::anyhow::Result<_> = async { $body }.await;
        db.checkpoint().await.ok();
        result
    }};
}

pub struct Database {
    _db: TursoDatabase,
    conn: Connection,
}

impl Database {
    pub async fn open_path(path: &std::path::Path) -> Result<Self> {
        let path_str = path.to_str().ok_or_else(|| {
            anyhow::anyhow!("database path contains invalid UTF-8: {}", path.display())
        })?;
        Self::open(path_str).await
    }

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

    #[cfg(test)]
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
            match self.conn.query("PRAGMA wal_checkpoint(TRUNCATE)", ()).await {
                Ok(mut rows) => {
                    while rows.next().await?.is_some() {
                        // consume all rows from the checkpoint pragma
                    }
                    return Ok(());
                }
                Err(e) if is_lock_contention(&e) => {
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                Err(e) => return Err(classify_db_error(e)),
            }
        }
        Err(anyhow::anyhow!(
            "checkpoint failed after {MAX_RETRIES} retries: lock contention persisted"
        ))
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
        let mut db = Database::open_path(&db_path).await.expect("open failed");
        db.checkpoint().await.expect("checkpoint failed");
    }

    #[tokio::test]
    async fn open_file_db_wal_mode_enabled() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db = Database::open_path(&db_path).await.expect("open failed");
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
        let db = Database::open_path(&db_path).await.expect("open failed");
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

    #[tokio::test]
    async fn concurrent_access_two_databases_open_simultaneously() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().expect("path valid").to_string();

        let db1 = Database::open(&db_path_str)
            .await
            .expect("first open failed");
        db1.conn()
            .execute("CREATE TABLE test_concurrent (id INTEGER, val TEXT)", ())
            .await
            .expect("create table failed");

        let mut db2 = Database::open(&db_path_str)
            .await
            .expect("second open failed");
        db2.conn()
            .execute("INSERT INTO test_concurrent VALUES (1, 'from_db2')", ())
            .await
            .expect("insert from db2 failed");
        db2.checkpoint().await.expect("checkpoint db2 failed");
        drop(db2);

        let mut rows = db1
            .conn()
            .query("SELECT val FROM test_concurrent WHERE id = 1", ())
            .await
            .expect("query on db1 failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let val: String = row.get(0).expect("get val");
        assert_eq!(val, "from_db2");
    }

    #[tokio::test]
    async fn concurrent_write_then_read_interleaved() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().expect("path valid").to_string();

        let mut db_write = Database::open(&db_path_str)
            .await
            .expect("writer open failed");
        db_write
            .conn()
            .execute("CREATE TABLE test_interleave (id INTEGER, val TEXT)", ())
            .await
            .expect("create table failed");
        db_write
            .conn()
            .execute("INSERT INTO test_interleave VALUES (1, 'writer_data')", ())
            .await
            .expect("insert failed");
        db_write.checkpoint().await.expect("checkpoint failed");
        drop(db_write);

        let db_read = Database::open(&db_path_str)
            .await
            .expect("reader open failed");
        let mut rows = db_read
            .conn()
            .query("SELECT val FROM test_interleave WHERE id = 1", ())
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let val: String = row.get(0).expect("get val");
        assert_eq!(val, "writer_data");
    }

    #[tokio::test]
    async fn retry_loop_succeeds_after_lock_released() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().expect("path valid").to_string();

        let db1 = Database::open(&db_path_str)
            .await
            .expect("first open failed");
        db1.conn()
            .execute("CREATE TABLE test_retry (id INTEGER)", ())
            .await
            .expect("create table failed");

        let join = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            drop(db1);
        });

        let db2 = Database::open(&db_path_str)
            .await
            .expect("retry open should succeed");
        let mut rows = db2
            .conn()
            .query("SELECT COUNT(*) FROM test_retry", ())
            .await
            .expect("query failed");
        let row = rows
            .next()
            .await
            .expect("fetch failed")
            .expect("row exists");
        let count: i64 = row.get(0).expect("get count");
        assert_eq!(count, 0);

        join.await.expect("join failed");
    }

    #[tokio::test]
    async fn checkpoint_returns_error_on_nonexistent_table() {
        let dir = tempdir().expect("tempdir failed");
        let db_path = dir.path().join("test.db");
        let mut db = Database::open_path(&db_path).await.expect("open failed");
        db.conn()
            .execute("CREATE TABLE test_cp (id INTEGER)", ())
            .await
            .expect("create failed");
        db.checkpoint()
            .await
            .expect("checkpoint should succeed on valid db");
    }
}
