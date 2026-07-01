use anyhow::{Context, Result};
use chrono::{
    DateTime, Datelike, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use clap::{Args, Parser, Subcommand};
use std::io::{self, Read};
use turso::Builder;

use crate::models::summary::Summary;
use crate::schema;
use crate::services::summary::SummaryService;
use crate::services::timeline::TimelineService;

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

struct TimeRange {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

fn parse_date_range(start_str: &str, end_str: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let (start_date, start_time) = parse_date_input(start_str)?;
    let (end_date, end_time) = parse_date_input(end_str)?;
    let start_time = start_time.unwrap_or(NaiveTime::from_hms_opt(0, 0, 0).expect("valid time"));
    let end_time = end_time.unwrap_or(NaiveTime::from_hms_opt(23, 59, 59).expect("valid time"));
    let start = local_to_utc(NaiveDateTime::new(start_date, start_time))?;
    let end = local_to_utc(NaiveDateTime::new(end_date, end_time))?;
    Ok((start, end))
}

async fn create_summary_from_args(
    conn: turso::Connection,
    root_dir: std::path::PathBuf,
    content: &str,
    args: &SummaryCreateArgs,
) -> Result<Summary> {
    if content.is_empty() {
        anyhow::bail!("stdin content cannot be empty");
    }
    let (start, end) = parse_date_range(&args.start, &args.end)?;
    let mut service = SummaryService::new(conn, root_dir);
    service.create_summary(start, end, content).await
}

pub async fn run_summary_create(args: SummaryCreateArgs) -> Result<()> {
    let db = Builder::new_local("reflections.db")
        .experimental_custom_types(true)
        .build()
        .await
        .context("failed to build database")?;
    let conn = db.connect().context("failed to connect to database")?;
    schema::init_schema(&conn).await?;
    let root_dir = std::env::current_dir().context("failed to get current directory")?;

    let mut content = String::new();
    io::stdin()
        .read_to_string(&mut content)
        .context("failed to read stdin")?;

    let summary = create_summary_from_args(conn, root_dir, &content, &args).await?;

    serde_json::to_writer_pretty(io::stdout(), &summary).context("failed to write JSON output")?;
    Ok(())
}

pub async fn run_timeline(args: TimelineArgs) -> Result<()> {
    let db = Builder::new_local("reflections.db")
        .experimental_custom_types(true)
        .build()
        .await
        .context("failed to build database")?;
    let conn = db.connect().context("failed to connect to database")?;
    schema::init_schema(&conn).await?;
    let root_dir = std::env::current_dir().context("failed to get current directory")?;
    let service = TimelineService::new(conn, root_dir);

    let range = resolve_time_range(&args)?;
    let entries = service
        .get_timeline(range.start, range.end)
        .await
        .context("failed to get timeline entries")?;

    serde_json::to_writer_pretty(io::stdout(), &entries).context("failed to write JSON output")?;
    Ok(())
}

fn resolve_time_range(args: &TimelineArgs) -> Result<TimeRange> {
    match args.first.as_str() {
        "today" if args.second.is_none() => resolve_today(),
        "week" if args.second.is_none() => resolve_week(),
        _ => resolve_explicit_range(&args.first, args.second.as_ref()),
    }
}

fn resolve_today() -> Result<TimeRange> {
    let now = Local::now();
    let today = now.date_naive();
    let tomorrow = today.succ_opt().context("failed to compute tomorrow")?;

    let start_naive =
        NaiveDateTime::new(today, NaiveTime::from_hms_opt(0, 0, 0).expect("valid time"));
    let end_naive = NaiveDateTime::new(
        tomorrow,
        NaiveTime::from_hms_opt(0, 0, 0).expect("valid time"),
    );

    let start = local_to_utc(start_naive)?;
    let end = local_to_utc(end_naive)?;

    Ok(TimeRange { start, end })
}

fn resolve_week() -> Result<TimeRange> {
    let now = Local::now();
    let today = now.date_naive();

    let days_since_monday = today.weekday().num_days_from_monday();
    let monday = today
        .checked_sub_days(chrono::Days::new(days_since_monday as u64))
        .context("failed to compute Monday")?;
    let next_monday = monday
        .succ_opt()
        .and_then(|d| d.checked_add_days(chrono::Days::new(6)))
        .context("failed to compute next Monday")?;

    let start_naive = NaiveDateTime::new(
        monday,
        NaiveTime::from_hms_opt(0, 0, 0).expect("valid time"),
    );
    let end_naive = NaiveDateTime::new(
        next_monday,
        NaiveTime::from_hms_opt(0, 0, 0).expect("valid time"),
    );

    let start = local_to_utc(start_naive)?;
    let end = local_to_utc(end_naive)?;

    Ok(TimeRange { start, end })
}

fn resolve_explicit_range(start_str: &str, end_str: Option<&String>) -> Result<TimeRange> {
    let end_str = end_str.context("end date required for explicit range")?;
    let (start, end) = parse_date_range(start_str, end_str)?;
    Ok(TimeRange { start, end })
}

fn parse_date_input(input: &str) -> Result<(NaiveDate, Option<NaiveTime>)> {
    if let Some((date_part, time_part)) = input.split_once(' ') {
        let date = NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
            .with_context(|| format!("invalid date format: {}", date_part))?;
        let time = NaiveTime::parse_from_str(time_part, "%H:%M")
            .with_context(|| format!("invalid time format: {}", time_part))?;
        return Ok((date, Some(time)));
    }

    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .with_context(|| format!("invalid date format: {}", input))?;
    Ok((date, None))
}

fn local_to_utc(naive: NaiveDateTime) -> Result<DateTime<Utc>> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(dt, _) => Ok(dt.with_timezone(&Utc)),
        LocalResult::None => Err(anyhow::anyhow!(
            "local time does not exist (DST spring-forward gap)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use chrono::{Datelike, Timelike};
    use tempfile::tempdir;
    use turso::Builder;

    #[test]
    fn parse_date_only_defaults_to_midnight() {
        let (date, time) = parse_date_input("2026-01-25").expect("parse failed");
        assert_eq!(
            date,
            NaiveDate::from_ymd_opt(2026, 1, 25).expect("valid date")
        );
        assert!(time.is_none());
    }

    #[test]
    fn parse_datetime_with_time() {
        let (date, time) = parse_date_input("2026-01-25 14:30").expect("parse failed");
        assert_eq!(
            date,
            NaiveDate::from_ymd_opt(2026, 1, 25).expect("valid date")
        );
        assert_eq!(
            time,
            Some(NaiveTime::from_hms_opt(14, 30, 0).expect("valid time"))
        );
    }

    #[test]
    fn parse_invalid_date_returns_error() {
        let result = parse_date_input("garbage");
        assert!(result.is_err());
    }

    #[test]
    fn today_range_resolves_correctly() {
        let range = resolve_today().expect("resolve_today failed");

        let start_local = range.start.with_timezone(&Local);
        let end_local = range.end.with_timezone(&Local);

        assert_eq!(start_local.hour(), 0);
        assert_eq!(start_local.minute(), 0);
        assert_eq!(start_local.second(), 0);

        assert_eq!(end_local.hour(), 0);
        assert_eq!(end_local.minute(), 0);
        assert_eq!(end_local.second(), 0);

        let duration = range.end - range.start;
        assert_eq!(duration.num_seconds(), 24 * 60 * 60);
    }

    #[test]
    fn week_range_resolves_monday_to_sunday() {
        let range = resolve_week().expect("resolve_week failed");

        let start_local = range.start.with_timezone(&Local);
        let end_local = range.end.with_timezone(&Local);

        assert_eq!(start_local.hour(), 0);
        assert_eq!(start_local.minute(), 0);
        assert_eq!(start_local.second(), 0);
        assert_eq!(start_local.weekday(), chrono::Weekday::Mon);

        assert_eq!(end_local.hour(), 0);
        assert_eq!(end_local.minute(), 0);
        assert_eq!(end_local.second(), 0);
        assert_eq!(end_local.weekday(), chrono::Weekday::Mon);

        let duration = range.end - range.start;
        assert_eq!(duration.num_seconds(), 7 * 24 * 60 * 60);
    }

    #[test]
    fn explicit_date_range_defaults_to_midnight_and_end_of_day() {
        let args = TimelineArgs {
            first: "2026-01-25".to_string(),
            second: Some("2026-02-03".to_string()),
        };
        let range =
            resolve_explicit_range(&args.first, args.second.as_ref()).expect("resolve failed");

        let start_local = range.start.with_timezone(&Local);
        let end_local = range.end.with_timezone(&Local);

        assert_eq!(start_local.hour(), 0);
        assert_eq!(start_local.minute(), 0);
        assert_eq!(start_local.second(), 0);

        assert_eq!(end_local.hour(), 23);
        assert_eq!(end_local.minute(), 59);
        assert_eq!(end_local.second(), 59);
    }

    #[test]
    fn explicit_datetime_range() {
        let args = TimelineArgs {
            first: "2026-01-25 14:30".to_string(),
            second: Some("2026-02-03 09:00".to_string()),
        };
        let range =
            resolve_explicit_range(&args.first, args.second.as_ref()).expect("resolve failed");

        let start_local = range.start.with_timezone(&Local);
        let end_local = range.end.with_timezone(&Local);

        assert_eq!(start_local.hour(), 14);
        assert_eq!(start_local.minute(), 30);
        assert_eq!(start_local.second(), 0);

        assert_eq!(end_local.hour(), 9);
        assert_eq!(end_local.minute(), 0);
        assert_eq!(end_local.second(), 0);
    }

    #[test]
    fn resolve_explicit_range_without_end_date_returns_error() {
        let result = resolve_explicit_range("2026-01-25", None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_date_input_with_invalid_time_returns_error() {
        let result = parse_date_input("2026-01-25 25:99");
        assert!(result.is_err());
    }

    #[test]
    fn parse_summary_timestamps_valid() {
        let (start_date, start_time) = parse_date_input("2026-01-01").expect("parse failed");
        assert_eq!(
            start_date,
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date")
        );
        assert!(start_time.is_none());

        let (end_date, end_time) = parse_date_input("2026-01-31").expect("parse failed");
        assert_eq!(
            end_date,
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("valid date")
        );
        assert!(end_time.is_none());
    }

    #[test]
    fn summary_create_args_validation() {
        let result =
            Cli::try_parse_from(["reflect", "summary", "create", "2026-01-01", "2026-01-31"])
                .expect("parse failed");

        match result.command {
            Some(Command::Summary(args)) => {
                let create_args = match args.command {
                    SummaryCommand::Create(ca) => ca,
                };
                assert_eq!(create_args.start, "2026-01-01");
                assert_eq!(create_args.end, "2026-01-31");
            }
            _ => panic!("Expected Command::Summary"),
        }
    }

    #[tokio::test]
    async fn create_summary_from_args_full_flow() {
        let db = Builder::new_local(":memory:")
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

        let args = SummaryCreateArgs {
            start: "2026-01-01".to_string(),
            end: "2026-01-31".to_string(),
        };
        let content = "# Summary\n\nThis is test content.";
        let summary = create_summary_from_args(conn.clone(), root_path.clone(), content, &args)
            .await
            .expect("create failed");

        let mut rows = conn
            .query(
                "SELECT summary_id FROM summary WHERE summary_id = ?",
                [summary.id.to_string()],
            )
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_some());

        let mut rows = conn
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
        let db = Builder::new_local(":memory:")
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

        let args = SummaryCreateArgs {
            start: "2026-01-01".to_string(),
            end: "2026-01-31".to_string(),
        };
        let result = create_summary_from_args(conn.clone(), root_path, "", &args).await;

        assert!(result.is_err());
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("stdin content cannot be empty"));

        let mut rows = conn
            .query("SELECT summary_id FROM summary", ())
            .await
            .expect("query failed");
        assert!(rows.next().await.expect("fetch failed").is_none());
    }

    #[tokio::test]
    async fn create_summary_from_args_json_output_is_valid() {
        let db = Builder::new_local(":memory:")
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

        let args = SummaryCreateArgs {
            start: "2026-01-01".to_string(),
            end: "2026-01-31".to_string(),
        };
        let summary = create_summary_from_args(conn, root_path, "test content", &args)
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

    #[test]
    fn parse_date_range_defaults_to_midnight_and_end_of_day() {
        let (start, end) = parse_date_range("2026-01-25", "2026-02-03").expect("parse failed");

        let start_local = start.with_timezone(&Local);
        let end_local = end.with_timezone(&Local);

        assert_eq!(start_local.hour(), 0);
        assert_eq!(start_local.minute(), 0);
        assert_eq!(start_local.second(), 0);

        assert_eq!(end_local.hour(), 23);
        assert_eq!(end_local.minute(), 59);
        assert_eq!(end_local.second(), 59);
    }
}
