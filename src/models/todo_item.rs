use chrono::{DateTime, Utc};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoStatus {
    New,
    InProgress,
    Done,
}

impl fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TodoStatus::New => write!(f, "NEW"),
            TodoStatus::InProgress => write!(f, "IN_PROGRESS"),
            TodoStatus::Done => write!(f, "DONE"),
        }
    }
}

impl FromStr for TodoStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "NEW" => Ok(TodoStatus::New),
            "IN_PROGRESS" => Ok(TodoStatus::InProgress),
            "DONE" => Ok(TodoStatus::Done),
            other => Err(anyhow::anyhow!("invalid TodoStatus: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub id: Uuid,
    pub label: String,
    pub status: TodoStatus,
    pub created_at: DateTime<Utc>,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}
