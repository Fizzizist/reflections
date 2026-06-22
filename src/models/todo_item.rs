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
            TodoStatus::New => write!(f, "New"),
            TodoStatus::InProgress => write!(f, "InProgress"),
            TodoStatus::Done => write!(f, "Done"),
        }
    }
}

impl FromStr for TodoStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "New" => Ok(TodoStatus::New),
            "InProgress" => Ok(TodoStatus::InProgress),
            "Done" => Ok(TodoStatus::Done),
            other => Err(anyhow::anyhow!("invalid TodoStatus: {other}")),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub id: Uuid,
    pub label: String,
    pub status: TodoStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
