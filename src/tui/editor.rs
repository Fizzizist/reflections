use crate::services::editable::{EditableEntity, EditableEntityRecord};
use crate::services::reflection::ReflectionService;
use anyhow::Result;
use std::io::{self, Write};
use std::path::Path;
use uuid::Uuid;

use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

use std::sync::Arc;

pub type EditorFn = Arc<dyn Fn(&Path) -> Result<()>>;

pub fn default_editor_fn() -> EditorFn {
    Arc::new(open_editor)
}

pub async fn create_and_edit<T: EditableEntity>(
    service: &mut T,
    editor_fn: &EditorFn,
    related_id: Option<Uuid>,
) -> Result<bool>
where
    T::Entity: EditableEntityRecord,
{
    let entity = service.create(related_id).await?;
    let path = service.full_path(entity.file_path());
    if let Err(e) = editor_fn(&path) {
        service.cleanup(entity.id()).await?;
        return Err(e);
    }
    service.cleanup(entity.id()).await?;
    Ok(true)
}

pub async fn create_and_edit_reflection(
    reflection_service: &mut ReflectionService,
    editor_fn: &EditorFn,
    about_id: Option<Uuid>,
) -> Result<bool> {
    create_and_edit(reflection_service, editor_fn, about_id).await
}

pub fn open_editor(file_path: &Path) -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste)?;

    let editor_result = run_editor(file_path);

    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    enable_raw_mode()?;

    editor_result
}

fn run_editor(file_path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
    let status = std::process::Command::new(&editor)
        .arg(file_path)
        .status()?;

    if !status.success() {
        // End-of-process user-facing notice: editor exited abnormally
        writeln!(io::stderr(), "editor exited with non-zero status")?;
        return Err(anyhow::anyhow!("editor exited with non-zero status"));
    }

    Ok(())
}
