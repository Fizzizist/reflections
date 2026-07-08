use super::input_box::InputBox;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldFocus {
    Name,
    Datetime,
}

const DATETIME_SECTIONS: [&str; 5] = ["Year", "Month", "Day", "Hour", "Minute"];

pub struct MeetingModal {
    active: bool,
    field_focus: FieldFocus,
    name_input: InputBox,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    dt_section: usize,
    dt_typing: Option<String>,
}

impl MeetingModal {
    pub fn new() -> Self {
        Self {
            active: false,
            field_focus: FieldFocus::Name,
            name_input: InputBox::new(),
            year: 0,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            dt_section: 0,
            dt_typing: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self) {
        self.active = true;
        self.field_focus = FieldFocus::Name;
        self.name_input.clear();
        self.dt_section = 0;
        self.dt_typing = None;

        let now = Local::now();
        self.year = now.year();
        self.month = now.month();
        self.day = now.day();
        self.hour = now.hour();
        self.minute = now.minute();
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<(String, DateTime<Utc>)> {
        match self.field_focus {
            FieldFocus::Name => self.handle_name_key(key),
            FieldFocus::Datetime => self.handle_datetime_key(key),
        }
    }

    fn handle_name_key(&mut self, key: KeyEvent) -> Option<(String, DateTime<Utc>)> {
        match key.code {
            KeyCode::Tab => {
                self.field_focus = FieldFocus::Datetime;
                self.dt_section = 0;
                None
            }
            KeyCode::BackTab => {
                self.field_focus = FieldFocus::Datetime;
                self.dt_section = DATETIME_SECTIONS.len() - 1;
                None
            }
            KeyCode::Char('q') => {
                self.close();
                None
            }
            KeyCode::Enter => {
                let trimmed = self.name_input.value().trim().to_string();
                if trimmed.is_empty() {
                    self.close();
                    None
                } else {
                    let dt = self.build_datetime();
                    self.close();
                    Some((trimmed, dt))
                }
            }
            KeyCode::Esc => {
                self.close();
                None
            }
            _ => {
                self.name_input.handle_key(key);
                None
            }
        }
    }

    fn handle_datetime_key(&mut self, key: KeyEvent) -> Option<(String, DateTime<Utc>)> {
        match key.code {
            KeyCode::Tab => {
                self.commit_dt_typing();
                self.field_focus = FieldFocus::Datetime;
                self.dt_section = (self.dt_section + 1) % DATETIME_SECTIONS.len();
                None
            }
            KeyCode::BackTab => {
                self.commit_dt_typing();
                self.field_focus = FieldFocus::Name;
                None
            }
            KeyCode::Char('q') => {
                self.close();
                None
            }
            KeyCode::Char('h') => {
                self.commit_dt_typing();
                if self.dt_section > 0 {
                    self.dt_section -= 1;
                } else {
                    self.field_focus = FieldFocus::Name;
                }
                None
            }
            KeyCode::Char('l') => {
                self.commit_dt_typing();
                if self.dt_section < DATETIME_SECTIONS.len() - 1 {
                    self.dt_section += 1;
                } else {
                    self.field_focus = FieldFocus::Name;
                }
                None
            }
            KeyCode::Char('j') => {
                self.dt_typing = None;
                self.increment_datetime_section();
                None
            }
            KeyCode::Char('k') => {
                self.dt_typing = None;
                self.decrement_datetime_section();
                None
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let max_width = self.dt_section_max_width();
                let typing = self.dt_typing.get_or_insert_with(String::new);
                if typing.len() >= max_width {
                    typing.clear();
                }
                typing.push(c);
                if typing.len() >= max_width {
                    self.commit_dt_typing();
                }
                None
            }
            KeyCode::Enter => {
                self.commit_dt_typing();
                let trimmed = self.name_input.value().trim().to_string();
                if trimmed.is_empty() {
                    self.close();
                    None
                } else {
                    let dt = self.build_datetime();
                    self.close();
                    Some((trimmed, dt))
                }
            }
            KeyCode::Esc => {
                self.close();
                None
            }
            _ => {
                self.dt_typing = None;
                None
            }
        }
    }

    fn increment_datetime_section(&mut self) {
        match self.dt_section {
            0 => self.year += 1,
            1 => {
                self.month += 1;
                if self.month > 12 {
                    self.month = 1;
                }
                self.clamp_day();
            }
            2 => {
                let max_day = self.days_in_month();
                self.day += 1;
                if self.day > max_day {
                    self.day = 1;
                }
            }
            3 => {
                self.hour += 1;
                if self.hour > 23 {
                    self.hour = 0;
                }
            }
            4 => {
                self.minute += 1;
                if self.minute > 59 {
                    self.minute = 0;
                }
            }
            _ => {}
        }
    }

    fn decrement_datetime_section(&mut self) {
        match self.dt_section {
            0 if self.year > 1900 => {
                self.year -= 1;
            }
            1 if self.month > 1 => {
                self.month -= 1;
            }
            1 => {
                self.month = 12;
                self.clamp_day();
            }
            2 => {
                if self.day > 1 {
                    self.day -= 1;
                } else {
                    self.day = self.days_in_month();
                }
            }
            3 => {
                if self.hour > 0 {
                    self.hour -= 1;
                } else {
                    self.hour = 23;
                }
            }
            4 => {
                if self.minute > 0 {
                    self.minute -= 1;
                } else {
                    self.minute = 59;
                }
            }
            _ => {}
        }
    }

    fn days_in_month(&self) -> u32 {
        let year = self.year.clamp(1900, 9999);
        chrono::NaiveDate::from_ymd_opt(year, self.month, 1)
            .map(|d| d + chrono::Months::new(1) - chrono::Duration::days(1))
            .map(|d| d.day())
            .unwrap_or(28)
    }

    fn clamp_day(&mut self) {
        let max_day = self.days_in_month();
        if self.day > max_day {
            self.day = max_day;
        }
    }

    fn dt_section_max_width(&self) -> usize {
        match self.dt_section {
            0 => 4,
            _ => 2,
        }
    }

    fn commit_dt_typing(&mut self) {
        if let Some(typed) = self.dt_typing.take() {
            if typed.is_empty() {
                return;
            }
            match self.dt_section {
                0 => {
                    self.year = typed.parse().unwrap_or(self.year);
                }
                1 => {
                    let parsed: u32 = typed.parse().unwrap_or(self.month);
                    self.month = parsed.clamp(1, 12);
                    self.clamp_day();
                }
                2 => {
                    let parsed: u32 = typed.parse().unwrap_or(self.day);
                    let max = self.days_in_month();
                    self.day = parsed.clamp(1, max);
                }
                3 => {
                    let parsed: u32 = typed.parse().unwrap_or(self.hour);
                    self.hour = parsed.min(23);
                }
                4 => {
                    let parsed: u32 = typed.parse().unwrap_or(self.minute);
                    self.minute = parsed.min(59);
                }
                _ => {}
            }
        }
    }

    fn build_datetime(&self) -> DateTime<Utc> {
        let local_dt =
            Local.with_ymd_and_hms(self.year, self.month, self.day, self.hour, self.minute, 0);
        match local_dt {
            chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
            chrono::LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
            chrono::LocalResult::None => {
                let utc = Utc.with_ymd_and_hms(
                    self.year,
                    self.month,
                    self.day,
                    self.hour,
                    self.minute,
                    0,
                );
                match utc {
                    chrono::LocalResult::Single(dt) => dt,
                    _ => Utc::now(),
                }
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let name_label = "Name: ";
        let modal_width = 60u16.min(area.width);
        let field_width = modal_width.saturating_sub(4).max(1);
        let name_content_width = field_width
            .saturating_sub(u16::try_from(name_label.chars().count()).unwrap_or(u16::MAX))
            .max(1);
        let name_lines = self.name_input.line_count(name_content_width);
        let modal_height = (name_lines + 6).max(7).min(area.height);

        let modal_area = Rect::new(
            area.x + (area.width.saturating_sub(modal_width)) / 2,
            area.y + (area.height.saturating_sub(modal_height)) / 2,
            modal_width,
            modal_height,
        );

        frame.render_widget(Clear, modal_area);

        let block = Block::default().borders(Borders::ALL).title("Add Meeting");

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let name_style = if self.field_focus == FieldFocus::Name {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };

        let name_area = Rect::new(
            inner.x + 1,
            inner.y + 1,
            inner.width.saturating_sub(2),
            name_lines,
        );
        self.name_input
            .render_labeled(frame, name_area, name_label, name_style);

        let datetime_line = if self.field_focus == FieldFocus::Datetime {
            self.render_datetime_spans()
        } else {
            ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("When: "),
                ratatui::text::Span::raw(self.render_datetime_parts()),
            ])
        };

        let datetime_area = Rect::new(
            inner.x + 1,
            inner.y + 2 + name_lines,
            inner.width.saturating_sub(2),
            1,
        );
        frame.render_widget(
            ratatui::widgets::Paragraph::new(datetime_line),
            datetime_area,
        );
    }

    fn render_datetime_parts(&self) -> String {
        let parts = [
            format!("{:04}", self.year),
            format!("{:02}", self.month),
            format!("{:02}", self.day),
            format!("{:02}", self.hour),
            format!("{:02}", self.minute),
        ];

        let separators = ["-", "-", " ", ":", ""];

        let mut result = String::new();
        for (i, part) in parts.iter().enumerate() {
            result.push_str(part);
            if i < separators.len() && !separators[i].is_empty() {
                result.push_str(separators[i]);
            }
        }
        result
    }

    fn render_datetime_spans(&self) -> ratatui::text::Line<'static> {
        let reversed = Style::default().add_modifier(Modifier::REVERSED);
        let normal = Style::default();
        let parts = [
            format!("{:04}", self.year),
            format!("{:02}", self.month),
            format!("{:02}", self.day),
            format!("{:02}", self.hour),
            format!("{:02}", self.minute),
        ];
        let separators = ["-", "-", " ", ":", ""];

        let mut spans = vec![ratatui::text::Span::raw("When: ")];
        for (i, part) in parts.iter().enumerate() {
            let style = if i == self.dt_section {
                reversed
            } else {
                normal
            };
            let display = if i == self.dt_section {
                self.dt_typing.as_deref().unwrap_or(part)
            } else {
                part
            };
            spans.push(ratatui::text::Span::styled(display.to_string(), style));
            if i < separators.len() && !separators[i].is_empty() {
                spans.push(ratatui::text::Span::raw(separators[i]));
            }
        }
        ratatui::text::Line::from(spans)
    }
}

impl Default for MeetingModal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl MeetingModal {
    pub fn set_datetime_for_test(
        &mut self,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) {
        self.year = year;
        self.month = month;
        self.day = day;
        self.hour = hour;
        self.minute = minute;
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

    fn key_tab() -> KeyEvent {
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)
    }

    fn key_backtab() -> KeyEvent {
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
    }

    #[test]
    fn enter_returns_name_and_datetime() {
        let mut modal = MeetingModal::new();
        modal.open();

        for c in "Test Meeting".chars() {
            modal.handle_key(key_char(c));
        }

        let result = modal.handle_key(key_enter());
        assert!(result.is_some());
        let (name, _dt) = result.expect("should return Some");
        assert_eq!(name, "Test Meeting");
        assert!(!modal.is_active());
    }

    #[test]
    fn esc_closes_returns_none_from_name() {
        let mut modal = MeetingModal::new();
        modal.open();

        let result = modal.handle_key(key_esc());
        assert!(result.is_none());
        assert!(!modal.is_active());
    }

    #[test]
    fn esc_closes_returns_none_from_datetime() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());

        let result = modal.handle_key(key_esc());
        assert!(result.is_none());
        assert!(!modal.is_active());
    }

    #[test]
    fn q_closes_modal_from_name_field() {
        let mut modal = MeetingModal::new();
        modal.open();

        let result = modal.handle_key(key_char('q'));
        assert!(result.is_none());
        assert!(!modal.is_active());
    }

    #[test]
    fn q_closes_modal_from_datetime_field() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());

        let result = modal.handle_key(key_char('q'));
        assert!(result.is_none());
        assert!(!modal.is_active());
    }

    #[test]
    fn h_l_navigates_between_fields_via_tab() {
        let mut modal = MeetingModal::new();
        modal.open();

        assert_eq!(modal.field_focus, FieldFocus::Name);

        modal.handle_key(key_tab());
        assert_eq!(modal.field_focus, FieldFocus::Datetime);
        assert_eq!(modal.dt_section, 0);

        modal.handle_key(key_backtab());
        assert_eq!(modal.field_focus, FieldFocus::Name);
    }

    #[test]
    fn h_l_navigates_datetime_sections() {
        let mut modal = MeetingModal::new();
        modal.open();

        modal.handle_key(key_tab());
        assert_eq!(modal.field_focus, FieldFocus::Datetime);
        assert_eq!(modal.dt_section, 0);

        modal.handle_key(key_char('l'));
        assert_eq!(modal.dt_section, 1);

        modal.handle_key(key_char('l'));
        assert_eq!(modal.dt_section, 2);

        modal.handle_key(key_char('h'));
        assert_eq!(modal.dt_section, 1);

        modal.handle_key(key_char('h'));
        assert_eq!(modal.dt_section, 0);

        modal.handle_key(key_char('h'));
        assert_eq!(modal.field_focus, FieldFocus::Name);
    }

    #[test]
    fn j_k_increments_decrements_datetime_section() {
        let mut modal = MeetingModal::new();
        modal.open();

        let initial_year = modal.year;
        let initial_month = modal.month;
        let initial_hour = modal.hour;
        let initial_minute = modal.minute;

        modal.handle_key(key_tab());
        assert_eq!(modal.field_focus, FieldFocus::Datetime);
        assert_eq!(modal.dt_section, 0);

        modal.handle_key(key_char('j'));
        assert_eq!(modal.year, initial_year + 1);

        modal.handle_key(key_char('l'));
        assert_eq!(modal.dt_section, 1);
        modal.handle_key(key_char('j'));
        let expected_month = if initial_month >= 12 {
            1
        } else {
            initial_month + 1
        };
        assert_eq!(modal.month, expected_month);

        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('l'));
        assert_eq!(modal.dt_section, 3);
        modal.handle_key(key_char('j'));
        let expected_hour = (initial_hour + 1) % 24;
        assert_eq!(modal.hour, expected_hour);

        modal.handle_key(key_char('l'));
        assert_eq!(modal.dt_section, 4);
        modal.handle_key(key_char('j'));
        let expected_minute = (initial_minute + 1) % 60;
        assert_eq!(modal.minute, expected_minute);

        modal.handle_key(key_char('k'));
        assert_eq!(modal.minute, initial_minute);
    }

    #[test]
    fn name_field_accepts_text_input() {
        let mut modal = MeetingModal::new();
        modal.open();

        modal.handle_key(key_char('H'));
        modal.handle_key(key_char('e'));
        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('o'));

        assert_eq!(modal.name_input.value(), "Hello");
        assert_eq!(modal.name_input.cursor_pos(), 5);

        modal.handle_key(key_backspace());
        assert_eq!(modal.name_input.value(), "Hell");
    }

    #[test]
    fn open_prefills_current_datetime() {
        let mut modal = MeetingModal::new();
        let now = Local::now();

        modal.open();

        assert_eq!(modal.year, now.year());
        assert_eq!(modal.month, now.month());
        assert_eq!(modal.day, now.day());
        assert_eq!(modal.hour, now.hour());
        assert_eq!(modal.minute, now.minute());
    }

    #[test]
    fn empty_name_on_enter_returns_none() {
        let mut modal = MeetingModal::new();
        modal.open();

        let result = modal.handle_key(key_enter());
        assert!(result.is_none());
        assert!(!modal.is_active());
    }

    #[test]
    fn day_clamps_on_month_change() {
        let mut modal = MeetingModal::new();
        modal.open();

        modal.year = 2024;
        modal.month = 1;
        modal.day = 31;

        modal.handle_key(key_tab());
        modal.handle_key(key_char('l'));
        modal.handle_key(key_char('j'));

        assert_eq!(modal.month, 2);
        assert_eq!(modal.day, 29);
    }

    #[test]
    fn typing_digits_replaces_section_on_full_width() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());
        assert_eq!(modal.dt_section, 0);
        assert_eq!(modal.field_focus, FieldFocus::Datetime);

        modal.handle_key(key_char('2'));
        modal.handle_key(key_char('0'));
        modal.handle_key(key_char('2'));
        modal.handle_key(key_char('5'));

        assert_eq!(modal.year, 2025);
        assert_eq!(modal.dt_typing, None);
    }

    #[test]
    fn typing_partial_digit_commits_on_section_change() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());

        modal.handle_key(key_char('2'));
        assert_eq!(modal.dt_typing.as_deref(), Some("2"));

        modal.handle_key(key_char('l'));
        assert_eq!(modal.year, 2);
        assert_eq!(modal.dt_section, 1);
        assert_eq!(modal.dt_typing, None);
    }

    #[test]
    fn typing_digits_into_month_clamps_to_12() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());
        modal.handle_key(key_char('l'));

        assert_eq!(modal.dt_section, 1);
        modal.handle_key(key_char('9'));
        modal.handle_key(key_char('9'));

        assert_eq!(modal.month, 12);
        assert_eq!(modal.dt_typing, None);
    }

    #[test]
    fn typing_digits_into_hour_clamps_to_23() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());
        for _ in 0..3 {
            modal.handle_key(key_char('l'));
        }
        assert_eq!(modal.dt_section, 3);

        modal.handle_key(key_char('9'));
        assert_eq!(modal.dt_typing.as_deref(), Some("9"));
        modal.handle_key(key_char('l'));

        assert_eq!(modal.hour, 9);
    }

    #[test]
    fn jk_clears_typing_buffer() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());

        modal.handle_key(key_char('2'));
        assert!(modal.dt_typing.is_some());

        modal.handle_key(key_char('j'));
        assert_eq!(modal.dt_typing, None);
    }

    #[test]
    fn typing_then_jk_uses_original_value_for_increment() {
        let mut modal = MeetingModal::new();
        modal.open();
        let initial_year = modal.year;
        modal.handle_key(key_tab());

        modal.handle_key(key_char('2'));
        modal.handle_key(key_char('j'));

        assert_eq!(modal.year, initial_year + 1);
    }

    #[test]
    fn typing_full_width_then_typing_again_starts_fresh() {
        let mut modal = MeetingModal::new();
        modal.open();
        modal.handle_key(key_tab());

        modal.handle_key(key_char('2'));
        modal.handle_key(key_char('0'));
        modal.handle_key(key_char('2'));
        modal.handle_key(key_char('5'));
        assert_eq!(modal.year, 2025);
        assert_eq!(modal.dt_typing, None);

        modal.handle_key(key_char('1'));
        modal.handle_key(key_char('9'));
        modal.handle_key(key_char('9'));
        modal.handle_key(key_char('9'));

        assert_eq!(modal.year, 1999);
    }

    #[test]
    fn name_field_enforces_max_length_of_200() {
        let mut modal = MeetingModal::new();
        modal.open();

        for _ in 0..201 {
            modal.handle_key(key_char('a'));
        }

        assert_eq!(modal.name_input.value().len(), 200);
    }

    #[test]
    fn name_field_rejects_control_characters() {
        let mut modal = MeetingModal::new();
        modal.open();

        modal.handle_key(key_char('a'));
        modal.handle_key(key_char('\x01'));
        modal.handle_key(key_char('\n'));
        modal.handle_key(key_char('\t'));
        modal.handle_key(key_char('b'));

        assert_eq!(modal.name_input.value(), "ab");
    }
}
