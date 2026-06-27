use anyhow::Result;
use std::io::{self, Write};
use std::path::Path;

use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

#[allow(dead_code)]
pub fn open_editor(file_path: &Path) -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste)?;

    let editor_result = run_editor(file_path);

    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    enable_raw_mode()?;

    editor_result
}

#[allow(dead_code)]
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
