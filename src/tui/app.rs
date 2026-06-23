use crate::services::todo::TodoService;
use crate::tui::input_modal::InputModal;

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
    todo_list_view: TodoListView,
    input_modal: InputModal,
}

impl App {
    pub fn new(todo_service: TodoService) -> Self {
        Self {
            todo_list_view: TodoListView::new(todo_service),
            input_modal: InputModal::new(),
        }
    }

    pub async fn load_items(&mut self) -> Result<()> {
        self.todo_list_view.load_items().await
    }

    pub async fn submit_todo(&mut self, label: &str) -> Result<()> {
        self.todo_list_view.submit_todo(label).await
    }
}

pub fn render_app(app: &mut App, frame: &mut ratatui::Frame) {
    app.todo_list_view.render(frame);
    if app.input_modal.is_active() {
        app.input_modal.render(frame);
    }
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    todo_service: TodoService,
) -> Result<()> {
    let mut app = App::new(todo_service);
    app.load_items().await?;
    terminal.draw(|frame| render_app(&mut app, frame))?;

    let mut terminal_events = EventStream::new();
    loop {
        tokio::select! {
            event = terminal_events.next() => {
                match event {
                    Some(Ok(terminal_event)) => {
                        if let Event::Key(key) = terminal_event {
                            if app.input_modal.is_active() {
                                if let Some(input) = app.input_modal.handle_key(key) {
                                    let trimmed = input.trim();
                                    if !trimmed.is_empty() {
                                        app.submit_todo(trimmed).await?;
                                    }
                                    app.input_modal.close();
                                }
                                terminal.draw(|frame| render_app(&mut app, frame))?;
                            } else {
                                if let KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. } = key {
                                    break;
                                }
                                if let KeyCode::Char('a') = key.code {
                                    app.input_modal.open();
                                    terminal.draw(|frame| render_app(&mut app, frame))?;
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!("terminal event stream error: {e}"));
                    }
                    None => {
                        break;
                    }
                }
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
    use crate::models::todo_item::TodoItem;
    use crate::schema;
    use chrono::Utc;
    use uuid::Uuid;

    async fn test_app() -> App {
        let db = Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("trouble building the DB");
        let conn = db.connect().expect("trouble connecting to the db");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        App::new(TodoService::new(conn))
    }

    fn fixed_item(label: &str) -> TodoItem {
        let fixed_time = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        TodoItem {
            id: Uuid::now_v7(),
            label: label.to_string(),
            status: crate::models::todo_item::TodoStatus::New,
            created_at: fixed_time,
            updated_at: fixed_time,
        }
    }

    #[tokio::test]
    async fn empty_app_render() {
        let mut app = test_app().await;
        app.load_items().await.expect("load failed");
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("new default app open", terminal.backend());
    }

    #[tokio::test]
    async fn populated_list_render() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("buy groceries"), fixed_item("write tests")]);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("populated todo list", terminal.backend());
    }

    #[tokio::test]
    async fn modal_open_render() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("buy groceries")]);
        app.input_modal.open();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("modal open", terminal.backend());
    }
}
