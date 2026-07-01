use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::services::editable::EditableEntityRecord;

#[derive(Debug, Clone)]
pub struct Note {
    pub id: Uuid,
    #[allow(dead_code)]
    pub related_to_id: Option<Uuid>,
    pub file_path: String,
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}

impl EditableEntityRecord for Note {
    fn id(&self) -> Uuid {
        self.id
    }
    fn file_path(&self) -> &str {
        &self.file_path
    }
}
