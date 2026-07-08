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

    #[cfg(test)]
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

    #[cfg(test)]
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

    fn display_string(&self) -> String {
        if self.cursor_pos >= self.buffer.len() {
            format!("{}█", self.buffer)
        } else {
            let (before, after) = self.buffer.split_at(self.cursor_pos);
            format!("{}█{}", before, after)
        }
    }

    pub fn wrapped_lines(&self, width: u16) -> Vec<String> {
        let width = usize::from(width.max(1));
        let chars: Vec<char> = self.display_string().chars().collect();
        let mut lines: Vec<String> = chars
            .chunks(width)
            .map(|chunk| chunk.iter().collect())
            .collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    pub fn line_count(&self, width: u16) -> u16 {
        u16::try_from(self.wrapped_lines(width).len())
            .unwrap_or(u16::MAX)
            .max(1)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, style: Style) {
        let lines: Vec<Line> = self
            .wrapped_lines(area.width)
            .into_iter()
            .map(Line::from)
            .collect();
        frame.render_widget(Paragraph::new(lines).style(style), area);
    }

    pub fn render_labeled(&self, frame: &mut Frame, area: Rect, label: &str, style: Style) {
        let label_width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
        let content_width = area.width.saturating_sub(label_width).max(1);
        let indent = " ".repeat(usize::from(label_width));
        let lines: Vec<Line> = self
            .wrapped_lines(content_width)
            .into_iter()
            .enumerate()
            .map(|(i, chunk)| {
                let prefix = if i == 0 {
                    label.to_string()
                } else {
                    indent.clone()
                };
                Line::from(vec![Span::raw(prefix), Span::styled(chunk, style)])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
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

    #[test]
    fn wrapped_lines_empty_buffer_is_single_cursor_line() {
        let input = InputBox::new();
        assert_eq!(input.wrapped_lines(10), vec!["█".to_string()]);
        assert_eq!(input.line_count(10), 1);
    }

    #[test]
    fn wrapped_lines_sub_width_text_is_single_line() {
        let mut input = InputBox::new();
        for c in "hello".chars() {
            input.handle_key(key_char(c));
        }
        assert_eq!(input.wrapped_lines(10), vec!["hello█".to_string()]);
        assert_eq!(input.line_count(10), 1);
    }

    #[test]
    fn cursor_glyph_at_exact_width_boundary_pushes_a_wrap() {
        let mut input = InputBox::new();
        for c in "abcde".chars() {
            input.handle_key(key_char(c));
        }
        assert_eq!(
            input.wrapped_lines(5),
            vec!["abcde".to_string(), "█".to_string()]
        );
        assert_eq!(input.line_count(5), 2);
    }

    #[test]
    fn wrapped_lines_multi_line_overflow() {
        let mut input = InputBox::new();
        for _ in 0..12 {
            input.handle_key(key_char('x'));
        }
        assert_eq!(
            input.wrapped_lines(5),
            vec!["xxxxx".to_string(), "xxxxx".to_string(), "xx█".to_string()]
        );
        assert_eq!(input.line_count(5), 3);
    }

    #[test]
    fn wrapped_lines_mid_string_cursor_splices_glyph() {
        let mut input = InputBox::new();
        for c in "abcdef".chars() {
            input.handle_key(key_char(c));
        }
        input.handle_key(key_left());
        input.handle_key(key_left());
        assert_eq!(
            input.wrapped_lines(4),
            vec!["abcd".to_string(), "█ef".to_string()]
        );
        assert_eq!(input.line_count(4), 2);
    }

    #[test]
    fn wrapped_lines_zero_width_treated_as_one() {
        let mut input = InputBox::new();
        input.handle_key(key_char('a'));
        assert_eq!(
            input.wrapped_lines(0),
            vec!["a".to_string(), "█".to_string()]
        );
        assert_eq!(input.line_count(0), 2);
    }
}
