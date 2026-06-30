use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Reflection {
    pub id: Uuid,
    pub about_id: Option<Uuid>,
    pub file_path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
