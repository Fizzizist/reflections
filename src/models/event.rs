use chrono::{DateTime, Utc};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    TodoItemCreated,
    TodoItemStatusChanged,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventType::TodoItemCreated => write!(f, "TODO_ITEM_CREATED"),
            EventType::TodoItemStatusChanged => write!(f, "TODO_ITEM_STATUS_CHANGED"),
        }
    }
}

impl FromStr for EventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "TODO_ITEM_CREATED" => Ok(EventType::TodoItemCreated),
            "TODO_ITEM_STATUS_CHANGED" => Ok(EventType::TodoItemStatusChanged),
            other => Err(anyhow::anyhow!("invalid EventType: {other}")),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Event {
    pub event_id: Uuid,
    pub entity_id: Uuid,
    pub event_type: EventType,
    pub metadata: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
