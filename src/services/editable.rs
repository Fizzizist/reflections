use anyhow::Result;
use std::path::PathBuf;
use uuid::Uuid;

pub trait EditableEntity {
    type Entity;

    async fn create(&mut self, related_id: Option<Uuid>) -> Result<Self::Entity>;
    fn full_path(&self, file_path: &str) -> PathBuf;
    async fn cleanup(&mut self, id: Uuid) -> Result<()>;
}

pub trait EditableEntityRecord {
    fn id(&self) -> Uuid;
    fn file_path(&self) -> &str;
}
