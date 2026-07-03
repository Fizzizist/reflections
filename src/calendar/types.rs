use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEvent {
    pub name: String,
    pub scheduled_at: DateTime<Utc>,
}
