use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::services::editable::EditableEntityRecord;

#[derive(Debug, Clone, Serialize)]
pub struct Reflection {
    pub id: Uuid,
    pub about_id: Option<Uuid>,
    pub file_path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EditableEntityRecord for Reflection {
    fn id(&self) -> Uuid {
        self.id
    }
    fn file_path(&self) -> &str {
        &self.file_path
    }
}
