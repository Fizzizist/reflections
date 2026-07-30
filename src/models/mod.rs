pub mod event;
pub mod meeting;
pub mod note;
pub mod reflection;
pub mod summary;
pub mod tag;
pub mod todo_item;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DiffEntry {
    pub left: Option<String>,
    pub right: Option<String>,
}
