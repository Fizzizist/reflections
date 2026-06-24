use super::splash;
use crate::models::todo_item::{TodoItem, TodoStatus};
use crate::services::todo::TodoService;
use crate::tui::input_modal::InputModal;
use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Constraint,
    widgets::{Block, Borders, Cell, Row, Table},
};

pub struct TodoListView {
    items: Vec<TodoItem>,
    service: TodoService,
    input_modal: InputModal,
}

impl TodoListView {
    pub fn new(service: TodoService) -> Self {
        Self {
            items: Vec::new(),
            service,
            input_modal: InputModal::new(),
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.load_items().await
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.input_modal.is_active() {
            if let Some(input) = self.input_modal.handle_key(key) {
                let trimmed = input.trim();
                if !trimmed.is_empty() {
                    self.submit_todo(trimmed).await?;
                }
                self.input_modal.close();
            }
        } else if let KeyCode::Char('a') = key.code {
            self.input_modal.open();
        }
        Ok(())
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let block = Block::default().borders(Borders::ALL).title("TODO List");
        let inner = block.inner(frame.area());

        frame.render_widget(block, frame.area());

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.items.is_empty() {
            let screen = hjkl_splash::start_screen::StartScreen::build(env!("CARGO_PKG_VERSION"));
            splash::render(frame, inner, &screen);
        } else {
            let rows: Vec<Row> = self
                .items
                .iter()
                .map(|item| {
                    let local_time = item.created_at.with_timezone(&Local);
                    let status_cell = match item.status {
                        TodoStatus::New => Cell::new("NEW"),
                        TodoStatus::InProgress => Cell::new("IN_PROGRESS"),
                        TodoStatus::Done => Cell::new("DONE"),
                    };
                    Row::new(vec![
                        status_cell,
                        Cell::new(item.label.clone()),
                        Cell::new(local_time.format("%Y-%m-%d %H:%M").to_string()),
                    ])
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Length(12),
                    Constraint::Min(20),
                    Constraint::Length(18),
                ],
            )
            .header(
                Row::new(vec![
                    Cell::new("Status"),
                    Cell::new("Label"),
                    Cell::new("Created"),
                ])
                .style(ratatui::style::Style::default().bold()),
            );

            frame.render_widget(table, inner);
        }

        if self.input_modal.is_active() {
            self.input_modal.render(frame);
        }
    }

    async fn load_items(&mut self) -> Result<()> {
        self.items = self.service.list_todo_items().await?;
        Ok(())
    }

    async fn submit_todo(&mut self, label: &str) -> Result<()> {
        self.service.create_todo_item(label).await?;
        self.load_items().await?;
        Ok(())
    }

    #[cfg(test)]
    pub fn set_items(&mut self, items: Vec<TodoItem>) {
        self.items = items;
    }

    #[cfg(test)]
    pub fn open_modal(&mut self) {
        self.input_modal.open();
    }
}
