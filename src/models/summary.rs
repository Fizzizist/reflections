use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::services::editable::EditableEntityRecord;

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub id: Uuid,
    pub file_path: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EditableEntityRecord for Summary {
    fn id(&self) -> Uuid {
        self.id
    }
    fn file_path(&self) -> &str {
        &self.file_path
    }
}
