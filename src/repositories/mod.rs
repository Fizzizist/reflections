pub mod event;
pub mod meeting;
pub mod note;
pub mod reflection;
pub mod summary;
pub mod todo_item;

use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};

pub fn parse_timestamp(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")?;
    Ok(naive.and_utc())
}
