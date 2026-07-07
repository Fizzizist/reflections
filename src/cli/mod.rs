use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::io::{self, Read};
use std::path::PathBuf;

use crate::calendar::google::GoogleCalendarBackend;
use crate::cli::date::resolve_time_range_from_args;
use crate::services::meeting::MeetingService;
use crate::services::summary::SummaryService;
use crate::services::timeline::TimelineService;

pub mod date;

#[derive(Parser)]
#[command(name = "reflect")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    Timeline(TimelineArgs),
    Summary(SummaryArgs),
    Meeting(MeetingArgs),
}

#[derive(Args)]
pub struct SummaryArgs {
    #[command(subcommand)]
    pub command: SummaryCommand,
}

#[derive(Subcommand)]
pub enum SummaryCommand {
    Create(SummaryCreateArgs),
}

#[derive(Args)]
pub struct SummaryCreateArgs {
    /// Start of the summary timeframe (format: %Y-%m-%d or %Y-%m-%d %H:%M)
    pub start: String,
    /// End of the summary timeframe (format: %Y-%m-%d or %Y-%m-%d %H:%M)
    pub end: String,
}

#[derive(Args)]
pub struct TimelineArgs {
    pub first: String,
    pub second: Option<String>,
}

#[derive(Args)]
pub struct MeetingArgs {
    #[command(subcommand)]
    pub command: MeetingCommand,
}

#[derive(Subcommand)]
pub enum MeetingCommand {
    Sync(SyncArgs),
}

#[derive(Args)]
pub struct SyncArgs {
    pub first: String,
    pub second: Option<String>,
}

async fn create_summary_from_args(
    db_path: PathBuf,
    root_dir: PathBuf,
    content: &str,
    args: &SummaryCreateArgs,
) -> Result<crate::models::summary::Summary> {
    if content.is_empty() {
        anyhow::bail!("stdin content cannot be empty");
    }
    let (start, end) = date::parse_date_range(&args.start, &args.end)?;
    let mut service = SummaryService::new(db_path, root_dir);
    service.create_summary(start, end, content).await
}

pub async fn run_summary_create(args: SummaryCreateArgs) -> Result<()> {
    let db_path = PathBuf::from(crate::db::DEFAULT_DB_PATH);
    let root_dir = std::env::current_dir().context("failed to get current directory")?;

    let mut content = String::new();
    io::stdin()
        .read_to_string(&mut content)
        .context("failed to read stdin")?;

    let summary = create_summary_from_args(db_path, root_dir, &content, &args).await?;

    serde_json::to_writer_pretty(io::stdout(), &summary).context("failed to write JSON output")?;
    Ok(())
}

pub async fn run_timeline(args: TimelineArgs) -> Result<()> {
    let db_path = PathBuf::from(crate::db::DEFAULT_DB_PATH);
    let root_dir = std::env::current_dir().context("failed to get current directory")?;
    let service = TimelineService::new(db_path, root_dir);

    let range = resolve_time_range_from_args(&args.first, args.second.as_ref())?;
    let entries = service
        .get_timeline(range.start, range.end)
        .await
        .context("failed to get timeline entries")?;

    serde_json::to_writer_pretty(io::stdout(), &entries).context("failed to write JSON output")?;
    Ok(())
}

pub async fn run_meeting_sync(args: SyncArgs) -> Result<()> {
    let db_path = PathBuf::from(crate::db::DEFAULT_DB_PATH);

    let range = resolve_time_range_from_args(&args.first, args.second.as_ref())?;
    let backend =
        GoogleCalendarBackend::new().context("failed to initialize Google Calendar backend")?;
    let mut service = MeetingService::new(db_path);
    let results = service
        .sync_meetings(&backend, range.start, range.end)
        .await
        .context("failed to sync meetings")?;

    serde_json::to_writer_pretty(io::stdout(), &results).context("failed to write JSON output")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::tempdir;

    #[test]
    fn summary_create_args_validation() {
        let result =
            Cli::try_parse_from(["reflect", "summary", "create", "2026-01-01", "2026-01-31"])
                .expect("parse failed");

        match result.command {
            Some(Command::Summary(args)) => {
                let SummaryCommand::Create(create_args) = args.command;
                assert_eq!(create_args.start, "2026-01-01");
                assert_eq!(create_args.end, "2026-01-31");
            }
            _ => panic!("Expected Command::Summary"),
        }
    }

    #[test]
    fn meeting_sync_today_parses_correctly() {
        let result =
            Cli::try_parse_from(["reflect", "meeting", "sync", "today"]).expect("parse failed");

        match result.command {
            Some(Command::Meeting(args)) => match args.command {
                MeetingCommand::Sync(sync_args) => {
                    assert_eq!(sync_args.first, "today");
                    assert!(sync_args.second.is_none());
                }
            },
            _ => panic!("Expected Command::Meeting"),
        }
    }

    #[test]
    fn meeting_sync_week_parses_correctly() {
        let result =
            Cli::try_parse_from(["reflect", "meeting", "sync", "week"]).expect("parse failed");

        match result.command {
            Some(Command::Meeting(args)) => match args.command {
                MeetingCommand::Sync(sync_args) => {
                    assert_eq!(sync_args.first, "week");
                    assert!(sync_args.second.is_none());
                }
            },
            _ => panic!("Expected Command::Meeting"),
        }
    }

    #[test]
    fn meeting_sync_explicit_range_parses_correctly() {
        let result =
            Cli::try_parse_from(["reflect", "meeting", "sync", "2026-01-01", "2026-01-31"])
                .expect("parse failed");

        match result.command {
            Some(Command::Meeting(args)) => match args.command {
                MeetingCommand::Sync(sync_args) => {
                    assert_eq!(sync_args.first, "2026-01-01");
                    assert_eq!(
                        sync_args.second.as_ref().expect("should be some"),
                        "2026-01-31"
                    );
                }
            },
            _ => panic!("Expected Command::Meeting"),
        }
    }

    #[test]
    fn meeting_sync_explicit_range_with_time_parses_correctly() {
        let result = Cli::try_parse_from([
            "reflect",
            "meeting",
            "sync",
            "2026-01-01 14:30",
            "2026-01-31 09:00",
        ])
        .expect("parse failed");

        match result.command {
            Some(Command::Meeting(args)) => match args.command {
                MeetingCommand::Sync(sync_args) => {
                    assert_eq!(sync_args.first, "2026-01-01 14:30");
                    assert_eq!(
                        sync_args.second.as_ref().expect("should be some"),
                        "2026-01-31 09:00"
                    );
                }
            },
            _ => panic!("Expected Command::Meeting"),
        }
    }

    #[tokio::test]
    async fn create_summary_from_args_full_flow() {
        let dir = tempdir().expect("create tempdir failed");
        let db_path = dir.path().join("test.db");
        let _db = Database::open(db_path.to_str().expect("path is valid utf-8"))
            .await
            .expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();

        let args = SummaryCreateArgs {
            start: "2026-01-01".to_string(),
            end: "2026-01-31".to_string(),
        };
        let content = "# Summary\n\nThis is test content.";
        let summary = create_summary_from_args(db_path.clone(), root_path.clone(), content, &args)
            .await
            .expect("create failed");

        let db = Database::open(db_path.to_str().expect("path is valid utf-8"))
            .await
            .expect("db open failed");
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

        let full_path = root_path.join(&summary.file_path);
        assert!(full_path.exists());
        let file_content = tokio::fs::read_to_string(&full_path)
            .await
            .expect("read failed");
        assert_eq!(file_content, content);
    }

    #[tokio::test]
    async fn create_summary_from_args_empty_content_returns_error() {
        let dir = tempdir().expect("create tempdir failed");
        let db_path = dir.path().join("test.db");
        let _db = Database::open(db_path.to_str().expect("path is valid utf-8"))
            .await
            .expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();

        let args = SummaryCreateArgs {
            start: "2026-01-01".to_string(),
            end: "2026-01-31".to_string(),
        };
        let result = create_summary_from_args(db_path.clone(), root_path, "", &args).await;

        assert!(result.is_err());
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("stdin content cannot be empty"));

        let db = Database::open(db_path.to_str().expect("path is valid utf-8"))
            .await
            .expect("db open failed");
        let mut rows = db
            .conn()
            .query("SELECT summary_id FROM summary", ())
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_none());
    }

    #[tokio::test]
    async fn create_summary_from_args_json_output_is_valid() {
        let dir = tempdir().expect("create tempdir failed");
        let db_path = dir.path().join("test.db");
        let _db = Database::open(db_path.to_str().expect("path is valid utf-8"))
            .await
            .expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let root_path = root_dir.path().to_path_buf();

        let args = SummaryCreateArgs {
            start: "2026-01-01".to_string(),
            end: "2026-01-31".to_string(),
        };
        let summary = create_summary_from_args(db_path, root_path, "test content", &args)
            .await
            .expect("create failed");

        let json = serde_json::to_string(&summary).expect("serialize failed");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse failed");

        assert!(parsed.get("id").is_some());
        assert!(parsed.get("file_path").is_some());
        assert!(parsed.get("start").is_some());
        assert!(parsed.get("end").is_some());
        assert!(parsed.get("created_at").is_some());
        assert!(parsed.get("updated_at").is_some());
    }
}
