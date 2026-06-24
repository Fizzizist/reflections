use crate::models::todo_item::TodoStatus;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
};
use uuid::Uuid;

pub struct StatusModal {
    active: bool,
    statuses: Vec<TodoStatus>,
    selected_index: usize,
    target_item_id: Option<Uuid>,
}

impl StatusModal {
    pub fn new() -> Self {
        Self {
            active: false,
            statuses: vec![TodoStatus::New, TodoStatus::InProgress, TodoStatus::Done],
            selected_index: 0,
            target_item_id: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self, item_id: Uuid, current_status: &TodoStatus) {
        self.target_item_id = Some(item_id);
        self.selected_index = match current_status {
            TodoStatus::New => 0,
            TodoStatus::InProgress => 1,
            TodoStatus::Done => 2,
        };
        self.active = true;
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn target_item_id(&self) -> Option<Uuid> {
        self.target_item_id
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<TodoStatus> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_index < self.statuses.len().saturating_sub(1) {
                    self.selected_index += 1;
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                None
            }
            KeyCode::Enter => {
                let status = self.statuses[self.selected_index].clone();
                self.close();
                Some(status)
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.close();
                None
            }
            _ => None,
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        if !self.active {
            return;
        }

        let area = frame.area();
        let width = 30u16;
        let height = 7u16;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Select Status");
        frame.render_widget(block.clone(), modal_area);

        let inner = block.inner(modal_area);

        let content: Vec<String> = self.statuses.iter().map(|s| s.to_string()).collect();

        for (i, status) in content.iter().enumerate() {
            let style = if i == self.selected_index {
                ratatui::style::Style::default().reversed()
            } else {
                ratatui::style::Style::default()
            };
            let paragraph = Paragraph::new(status.as_str()).style(style);
            let row_rect = Rect::new(
                inner.x + 1,
                inner.y + i as u16,
                inner.width.saturating_sub(2),
                1,
            );
            frame.render_widget(paragraph, row_rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key_char(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn key_enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    fn key_esc() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    #[test]
    fn open_pre_selects_current_status() {
        let item_id = Uuid::now_v7();

        let mut modal = StatusModal::new();
        modal.open(item_id, &TodoStatus::New);
        assert_eq!(modal.selected_index, 0);

        let mut modal = StatusModal::new();
        modal.open(item_id, &TodoStatus::InProgress);
        assert_eq!(modal.selected_index, 1);

        let mut modal = StatusModal::new();
        modal.open(item_id, &TodoStatus::Done);
        assert_eq!(modal.selected_index, 2);
    }

    #[test]
    fn j_navigates_down() {
        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::New);
        assert_eq!(modal.selected_index, 0);

        modal.handle_key(key_char('j'));
        assert_eq!(modal.selected_index, 1);

        modal.handle_key(key_char('j'));
        assert_eq!(modal.selected_index, 2);
    }

    #[test]
    fn k_navigates_up() {
        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::New);

        modal.handle_key(key_char('j'));
        modal.handle_key(key_char('j'));
        assert_eq!(modal.selected_index, 2);

        modal.handle_key(key_char('k'));
        assert_eq!(modal.selected_index, 1);
    }

    #[test]
    fn j_at_bottom_does_nothing() {
        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::Done);
        assert_eq!(modal.selected_index, 2);

        modal.handle_key(key_char('j'));
        assert_eq!(modal.selected_index, 2);
    }

    #[test]
    fn k_at_top_does_nothing() {
        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::New);
        assert_eq!(modal.selected_index, 0);

        modal.handle_key(key_char('k'));
        assert_eq!(modal.selected_index, 0);
    }

    #[test]
    fn enter_returns_selected_status() {
        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::New);

        let result = modal.handle_key(key_enter());
        assert_eq!(result, Some(TodoStatus::New));
        assert!(!modal.is_active());

        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::InProgress);
        modal.handle_key(key_char('j'));

        let result = modal.handle_key(key_enter());
        assert_eq!(result, Some(TodoStatus::Done));
        assert!(!modal.is_active());
    }

    #[test]
    fn esc_cancels() {
        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::New);
        assert!(modal.is_active());

        let result = modal.handle_key(key_esc());
        assert_eq!(result, None);
        assert!(!modal.is_active());
    }

    #[test]
    fn q_cancels() {
        let mut modal = StatusModal::new();
        modal.open(Uuid::now_v7(), &TodoStatus::New);
        assert!(modal.is_active());

        let result = modal.handle_key(key_char('q'));
        assert_eq!(result, None);
        assert!(!modal.is_active());
    }
}
