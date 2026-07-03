use crate::calendar::types::CalendarEvent;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[async_trait]
pub trait CalendarBackend: Send + Sync {
    async fn fetch_meetings(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>>;
}
