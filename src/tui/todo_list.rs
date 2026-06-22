use super::splash;
use crate::models::todo_item::{TodoItem, TodoStatus};
use chrono::Local;
use ratatui::{
    Frame,
    layout::Constraint,
    widgets::{Block, Borders, Cell, Row, Table},
};

pub struct TodoListView {
    items: Vec<TodoItem>,
}

impl TodoListView {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn set_items(&mut self, items: Vec<TodoItem>) {
        self.items = items;
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
    }
}
