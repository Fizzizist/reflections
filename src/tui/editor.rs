use crate::services::reflection::ReflectionService;
use anyhow::Result;
use std::io::{self, Write};
use std::path::Path;
use uuid::Uuid;

use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

pub type EditorFn = Box<dyn Fn(&Path) -> Result<()>>;

pub fn default_editor_fn() -> EditorFn {
    Box::new(open_editor)
}

pub async fn create_and_edit_reflection(
    reflection_service: &mut ReflectionService,
    editor_fn: &EditorFn,
    about_id: Option<Uuid>,
) -> Result<bool> {
    let reflection = reflection_service.create_reflection(about_id).await?;
    let path = reflection_service.full_path(&reflection.file_path);
    if let Err(e) = editor_fn(&path) {
        reflection_service.cleanup_reflection(reflection.id).await?;
        return Err(e);
    }
    reflection_service.cleanup_reflection(reflection.id).await?;
    Ok(true)
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
