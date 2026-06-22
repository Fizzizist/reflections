use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
};

pub struct InputModal {
    active: bool,
    buffer: String,
    cursor_pos: usize,
}

impl InputModal {
    pub fn new() -> Self {
        Self {
            active: false,
            buffer: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self) {
        self.active = true;
        self.buffer.clear();
        self.cursor_pos = 0;
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Enter => Some(self.buffer.clone()),
            KeyCode::Esc => {
                self.close();
                None
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 && !self.buffer.is_empty() {
                    self.buffer.remove(self.cursor_pos - 1);
                    self.cursor_pos -= 1;
                }
                None
            }
            KeyCode::Char(c) => {
                self.buffer.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
                None
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
                None
            }
            KeyCode::Right => {
                if self.cursor_pos < self.buffer.len() {
                    self.cursor_pos += 1;
                }
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
        let width = 60u16;
        let height = 7u16;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Add Todo Item");
        frame.render_widget(block.clone(), modal_area);

        let inner = block.inner(modal_area);

        let display_text = if self.cursor_pos >= self.buffer.len() {
            format!("{}█", self.buffer)
        } else {
            let (before, after) = self.buffer.split_at(self.cursor_pos);
            format!("{}█{}", before, after)
        };

        let paragraph = Paragraph::new(display_text);
        frame.render_widget(paragraph, inner);
    }
}
