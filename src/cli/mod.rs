use anyhow::{Context, Result};
use chrono::{
    DateTime, Datelike, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use clap::{Args, Parser, Subcommand};
use std::io;
use turso::Builder;

use crate::schema;
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

    let start = local_to_utc(start_naive);
    let end = local_to_utc(end_naive);

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

    let start = local_to_utc(start_naive);
    let end = local_to_utc(end_naive);

    Ok(TimeRange { start, end })
}

fn resolve_explicit_range(start_str: &str, end_str: Option<&String>) -> Result<TimeRange> {
    let end_str = end_str.context("end date required for explicit range")?;

    let (start_date, start_time) = parse_date_input(start_str)?;
    let (end_date, end_time) = parse_date_input(end_str)?;

    let start_time = start_time.unwrap_or(NaiveTime::from_hms_opt(0, 0, 0).expect("valid time"));
    let end_time = end_time.unwrap_or(NaiveTime::from_hms_opt(23, 59, 0).expect("valid time"));

    let start_naive = NaiveDateTime::new(start_date, start_time);
    let end_naive = NaiveDateTime::new(end_date, end_time);

    let start = local_to_utc(start_naive);
    let end = local_to_utc(end_naive);

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

fn local_to_utc(naive: NaiveDateTime) -> DateTime<Utc> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
        LocalResult::None => Utc.from_utc_datetime(&naive),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

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
        assert_eq!(end_local.second(), 0);
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
}
