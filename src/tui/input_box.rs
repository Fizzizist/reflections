use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

const MAX_LENGTH: usize = 200;

pub struct InputBox {
    buffer: String,
    cursor_pos: usize,
    max_length: usize,
}

impl InputBox {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor_pos: 0,
            max_length: MAX_LENGTH,
        }
    }

    pub fn with_max_length(max_length: usize) -> Self {
        Self {
            buffer: String::new(),
            cursor_pos: 0,
            max_length,
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_pos = 0;
    }

    pub fn value(&self) -> &str {
        &self.buffer
    }

    pub fn cursor_pos(&self) -> usize {
        self.cursor_pos
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) if !c.is_control() && self.buffer.len() < self.max_length => {
                self.buffer.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
            }
            KeyCode::Backspace if self.cursor_pos > 0 => {
                self.cursor_pos -= 1;
                self.buffer.remove(self.cursor_pos);
            }
            KeyCode::Left if self.cursor_pos > 0 => {
                self.cursor_pos -= 1;
            }
            KeyCode::Right if self.cursor_pos < self.buffer.len() => {
                self.cursor_pos += 1;
            }
            _ => {}
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, style: Style) {
        let display = if self.cursor_pos >= self.buffer.len() {
            format!("{}█", self.buffer)
        } else {
            let (before, after) = self.buffer.split_at(self.cursor_pos);
            format!("{}█{}", before, after)
        };
        frame.render_widget(Paragraph::new(display).style(style), area);
    }

    pub fn render_labeled(&self, frame: &mut Frame, area: Rect, label: &str, style: Style) {
        let display = if self.cursor_pos >= self.buffer.len() {
            format!("{}█", self.buffer)
        } else {
            let (before, after) = self.buffer.split_at(self.cursor_pos);
            format!("{}█{}", before, after)
        };
        let line = Line::from(vec![Span::raw(label), Span::styled(display, style)]);
        frame.render_widget(Paragraph::new(line), area);
    }
}

impl Default for InputBox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key_char(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
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
    fn char_inserts_at_cursor() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        input.handle_key(key_char('b'));
        input.handle_key(key_char('c'));
        assert_eq!(input.value(), "abc");
        assert_eq!(input.cursor_pos(), 3);
    }

    #[test]
    fn char_inserts_at_cursor_position() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        input.handle_key(key_char('c'));
        input.handle_key(key_left());
        input.handle_key(key_char('b'));
        assert_eq!(input.value(), "abc");
    }

    #[test]
    fn backspace_removes_char_before_cursor() {
        let mut input = InputBox::new();
        input.handle_key(key_char('h'));
        input.handle_key(key_char('e'));
        input.handle_key(key_char('l'));
        input.handle_key(key_char('l'));
        input.handle_key(key_char('o'));
        input.handle_key(key_backspace());
        assert_eq!(input.value(), "hell");
    }

    #[test]
    fn backspace_at_cursor_zero_does_nothing() {
        let mut input = InputBox::new();
        input.handle_key(key_backspace());
        assert_eq!(input.value(), "");
        assert_eq!(input.cursor_pos(), 0);
    }

    #[test]
    fn left_arrow_decrements_cursor() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        input.handle_key(key_char('b'));
        input.handle_key(key_char('c'));
        input.handle_key(key_left());
        assert_eq!(input.cursor_pos(), 2);
    }

    #[test]
    fn left_arrow_at_zero_does_nothing() {
        let mut input = InputBox::new();
        input.handle_key(key_left());
        assert_eq!(input.cursor_pos(), 0);
    }

    #[test]
    fn right_arrow_increments_cursor() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        input.handle_key(key_char('b'));
        input.handle_key(key_char('c'));
        input.handle_key(key_left());
        input.handle_key(key_left());
        input.handle_key(key_right());
        assert_eq!(input.cursor_pos(), 2);
    }

    #[test]
    fn right_arrow_at_end_does_nothing() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        input.handle_key(key_char('b'));
        input.handle_key(key_char('c'));
        input.handle_key(key_right());
        assert_eq!(input.cursor_pos(), 3);
    }

    #[test]
    fn rejects_control_characters() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        input.handle_key(key_char('\x01'));
        input.handle_key(key_char('\n'));
        input.handle_key(key_char('\t'));
        input.handle_key(key_char('b'));
        assert_eq!(input.value(), "ab");
    }

    #[test]
    fn enforces_max_length() {
        let mut input = InputBox::new();
        for _ in 0..201 {
            input.handle_key(key_char('a'));
        }
        assert_eq!(input.value().len(), 200);
    }

    #[test]
    fn with_max_length_enforces_custom_limit() {
        let mut input = InputBox::with_max_length(5);
        for _ in 0..10 {
            input.handle_key(key_char('a'));
        }
        assert_eq!(input.value().len(), 5);
    }

    #[test]
    fn clear_resets_buffer_and_cursor() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        input.handle_key(key_char('b'));
        input.clear();
        assert_eq!(input.value(), "");
        assert_eq!(input.cursor_pos(), 0);
    }
}
