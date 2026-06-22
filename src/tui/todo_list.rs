use super::splash;
use crate::models::todo_item::{TodoItem, TodoStatus};
use chrono::Local;
use ratatui::{
    Frame,
    widgets::{Block, Borders, List, ListItem},
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
            let list_items: Vec<ListItem> = self
                .items
                .iter()
                .map(|item| {
                    let local_time = item.created_at.with_timezone(&Local);
                    let status_label = match item.status {
                        TodoStatus::New => "[New]",
                        TodoStatus::InProgress => "[InProgress]",
                        TodoStatus::Done => "[Done]",
                    };
                    ListItem::new(format!(
                        "{} {} — {}",
                        status_label,
                        item.label,
                        local_time.format("%Y-%m-%d %H:%M")
                    ))
                })
                .collect();
            let list = List::new(list_items);
            frame.render_widget(list, inner);
        }
    }
}
