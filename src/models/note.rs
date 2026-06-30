use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Note {
    pub id: Uuid,
    pub related_to_id: Option<Uuid>,
    pub file_path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
