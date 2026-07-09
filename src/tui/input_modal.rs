use super::input_box::InputBox;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear},
};

pub struct InputModal {
    active: bool,
    input: InputBox,
}

impl InputModal {
    pub fn new() -> Self {
        Self {
            active: false,
            input: InputBox::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self) {
        self.active = true;
        self.input.clear();
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Enter => Some(self.input.value().to_string()),
            KeyCode::Esc => {
                self.close();
                None
            }
            _ => {
                self.input.handle_key(key);
                None
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.active {
            return;
        }
        let width = 60u16.min(area.width);
        let inner_width = width.saturating_sub(2).max(1);
        let height = (self.input.line_count(inner_width) + 2)
            .max(3)
            .min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Add Todo Item");
        frame.render_widget(block.clone(), modal_area);

        let inner = block.inner(modal_area);
        self.input
            .render(frame, inner, ratatui::style::Style::default());
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

    fn key_backspace() -> KeyEvent {
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
    }

    fn key_left() -> KeyEvent {
        KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)
    }

    fn key_right() -> KeyEvent {
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)
    }

    #[test]
    fn enter_returns_buffer() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_char('h'));
        modal.handle_key(key_char('e'));
        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('o'));
        let result = modal.handle_key(key_enter());
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn esc_closes_modal_and_returns_none() {
        let mut modal = InputModal::new();
        modal.open();
        assert!(modal.is_active());
        let result = modal.handle_key(key_esc());
        assert_eq!(result, None);
        assert!(!modal.is_active());
    }

    #[test]
    fn backspace_removes_char_before_cursor() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_char('h'));
        modal.handle_key(key_char('e'));
        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('o'));
        modal.handle_key(key_backspace());
        assert_eq!(modal.input.value(), "hell");
    }

    #[test]
    fn backspace_at_cursor_zero_does_nothing() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_backspace());
        assert_eq!(modal.input.value(), "");
        assert_eq!(modal.input.cursor_pos(), 0);
    }

    #[test]
    fn char_inserts_at_cursor() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_char('a'));
        modal.handle_key(key_char('b'));
        modal.handle_key(key_char('c'));
        assert_eq!(modal.input.value(), "abc");
        assert_eq!(modal.input.cursor_pos(), 3);
    }

    #[test]
    fn char_inserts_at_cursor_position() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_char('a'));
        modal.handle_key(key_char('c'));
        modal.handle_key(key_left());
        modal.handle_key(key_char('b'));
        assert_eq!(modal.input.value(), "abc");
    }

    #[test]
    fn left_arrow_decrements_cursor() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_char('a'));
        modal.handle_key(key_char('b'));
        modal.handle_key(key_char('c'));
        modal.handle_key(key_left());
        assert_eq!(modal.input.cursor_pos(), 2);
    }

    #[test]
    fn left_arrow_at_zero_does_nothing() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_left());
        assert_eq!(modal.input.cursor_pos(), 0);
    }

    #[test]
    fn right_arrow_increments_cursor() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_char('a'));
        modal.handle_key(key_char('b'));
        modal.handle_key(key_char('c'));
        modal.handle_key(key_left());
        modal.handle_key(key_left());
        modal.handle_key(key_right());
        assert_eq!(modal.input.cursor_pos(), 2);
    }

    #[test]
    fn right_arrow_at_end_does_nothing() {
        let mut modal = InputModal::new();
        modal.open();
        modal.handle_key(key_char('a'));
        modal.handle_key(key_char('b'));
        modal.handle_key(key_char('c'));
        modal.handle_key(key_right());
        assert_eq!(modal.input.cursor_pos(), 3);
    }

    #[test]
    fn empty_buffer_enter_returns_empty_string() {
        let mut modal = InputModal::new();
        modal.open();
        let result = modal.handle_key(key_enter());
        assert_eq!(result, Some("".to_string()));
    }
}
