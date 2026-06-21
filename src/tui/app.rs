use crate::services::todo::TodoService;

use super::todo_list::TodoListView;
use anyhow::Result;
use futures::stream::StreamExt;
use std::io;

use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
    KeyModifiers,
};
use ratatui::Terminal;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::CrosstermBackend;

pub struct App {
    todo_service: TodoService,
    todo_list_view: TodoListView,
}

impl App {
    pub fn new(todo_service: TodoService) -> Self {
        Self {
            todo_service,
            todo_list_view: TodoListView::default(),
        }
    }
}

pub fn render_app(app: &mut App, frame: &mut ratatui::Frame) {
    app.todo_list_view.render(frame);
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    todo_service: TodoService,
) -> Result<()> {
    let mut app = App::new(todo_service);
    terminal.draw(|frame| render_app(&mut app, frame))?;
    let mut terminal_events = EventStream::new();
    loop {
        tokio::select! {
            Some(Ok(terminal_event)) = terminal_events.next() => {
                if let Event::Key(key) = terminal_event
                    && let KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. } = key { break; }
            }
        }
    }

    Ok(())
}

pub async fn run(todo_service: TodoService) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal, todo_service).await;
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;
    result
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use turso::Builder;

    use super::*;

    #[tokio::test]
    async fn empty_app_render() {
        let db = Builder::new_local(":memory:")
            .build()
            .await
            .expect("trouble building the DB");
        let conn = db.connect().expect("trouble connecting to the db");
        let todo_service = TodoService::new(conn.clone());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        let mut app = App::new(todo_service);
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw?");
        insta::assert_snapshot!("new default app open", terminal.backend());
    }
}
