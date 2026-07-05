use crate::models::event::EventType;
use crate::services::timeline::{TimelineEntity, TimelineEntry, TimelineService};
use crate::tui::highlight::build_renderer;
use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use serde_json::Value;
use std::sync::LazyLock;
use the_other_tui_markdown::{Renderer, Theme, into_text_with_renderer};
use uuid::Uuid;

static RENDERER: LazyLock<Renderer> = LazyLock::new(|| {
    let theme = Theme {
        code_block: Style::new().fg(Color::Gray),
        code_block_lang: Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
        inline_code: Style::default().fg(Color::Gray),
        ..Theme::default()
    };
    build_renderer(theme)
});

pub struct TimelineView {
    entries: Vec<TimelineEntry>,
    title: String,
    scroll_offset: usize,
}

impl TimelineView {
    pub async fn open(service: &TimelineService, entity_id: Uuid, title: &str) -> Result<Self> {
        let entries = service.get_entity_timeline(entity_id).await?;
        Ok(Self {
            entries,
            title: title.to_string(),
            scroll_offset: 0,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => true,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // if it can't actually get a height, just use a reasonable default
                let (_, height) = terminal::size().unwrap_or((0, 24));
                self.scroll_down((height / 2).into());
                false
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let (_, height) = terminal::size().unwrap_or((0, 24));
                self.scroll_up((height / 2).into());
                false
            }
            _ => false,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title.as_str());
        let inner = block.inner(area);

        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let markdown_content = if self.entries.is_empty() {
            "No events found for this entity.".to_string()
        } else {
            self.build_markdown_content()
        };

        let text = into_text_with_renderer(&markdown_content, &RENDERER);
        let content_height = text.lines.len();
        let max_scroll = content_height.saturating_sub(inner.height as usize);

        self.scroll_offset = self.scroll_offset.min(max_scroll);

        let paragraph = Paragraph::new(text)
            .scroll((self.scroll_offset as u16, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, inner);
    }

    fn build_markdown_content(&self) -> String {
        let mut parts = Vec::new();

        for entry in &self.entries {
            let content = self.format_entry(entry);
            parts.push(content);
        }

        parts.join("\n---\n\n")
    }

    fn format_entry(&self, entry: &TimelineEntry) -> String {
        let timestamp = entry.created_at.with_timezone(&Local);
        let time_str = timestamp.format("%Y-%m-%d %H:%M").to_string();

        match entry.event_type {
            EventType::TodoItemCreated => {
                let label = entry
                    .entity
                    .as_ref()
                    .and_then(|e| match e {
                        TimelineEntity::TodoItem(item) => Some(item.label.as_str()),
                        _ => None,
                    })
                    .unwrap_or("(no label)");
                format!("## 📋 Todo Created — {}\n\n{}\n", time_str, label)
            }
            EventType::TodoItemStatusChanged => {
                let status_change = self.parse_status_change(&entry.metadata);
                format!("## 📋 Status Changed — {}\n\n{}\n", time_str, status_change)
            }
            EventType::MeetingCreated => {
                let name = entry
                    .entity
                    .as_ref()
                    .and_then(|e| match e {
                        TimelineEntity::Meeting(m) => Some(m.name.as_str()),
                        _ => None,
                    })
                    .unwrap_or("(no name)");
                format!("## 📅 Meeting Created — {}\n\n{}\n", time_str, name)
            }
            EventType::ReflectionCreated => {
                let content = entry
                    .entity
                    .as_ref()
                    .and_then(|e| match e {
                        TimelineEntity::Reflection(r) => r.content.as_deref(),
                        _ => None,
                    })
                    .unwrap_or("(no content)");
                format!("## 💭 Reflection — {}\n\n{}\n", time_str, content)
            }
            EventType::NoteCreated => {
                let content = entry
                    .entity
                    .as_ref()
                    .and_then(|e| match e {
                        TimelineEntity::Note(n) => n.content.as_deref(),
                        _ => None,
                    })
                    .unwrap_or("(no content)");
                format!("## 📝 Note — {}\n\n{}\n", time_str, content)
            }
            EventType::SummaryCreated => {
                let content = entry
                    .entity
                    .as_ref()
                    .and_then(|e| match e {
                        TimelineEntity::Summary(s) => s.content.as_deref(),
                        _ => None,
                    })
                    .unwrap_or("(no content)");
                format!("## 📊 Summary — {}\n\n{}\n", time_str, content)
            }
            EventType::SummaryUpdated => {
                let mut content = Vec::new();
                if let Ok(json) = serde_json::from_str::<Vec<Value>>(&entry.metadata) {
                    for diff_entry in json {
                        match diff_entry.get("left") {
                            Some(left) => match diff_entry.get("right") {
                                Some(_right) => content.push(format!(" {}", left)),
                                None => content.push(format!("-{}", left)),
                            },
                            None => {
                                if let Some(right) = diff_entry.get("right") {
                                    content.push(format!("+{}", right));
                                }
                            }
                        }
                    }
                }
                if !content.is_empty() {
                    return format!("## 📊 Summary — {}\n\n{}\n", time_str, content.join("\n"));
                }
                format!("## 📊 Summary — {}\n\n (no diff content) \n", time_str)
            }
        }
    }

    fn parse_status_change(&self, metadata: &str) -> String {
        if let Ok(json) = serde_json::from_str::<Value>(metadata) {
            let old_status = json
                .get("old_status")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN");
            let new_status = json
                .get("new_status")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN");
            format!("{} → {}", old_status, new_status)
        } else {
            "(invalid metadata)".to_string()
        }
    }

    fn scroll_down(&mut self, half_page: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(half_page);
    }

    fn scroll_up(&mut self, half_page: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(half_page);
    }
}

#[cfg(test)]
impl TimelineView {
    pub fn new_for_test(entries: Vec<TimelineEntry>, title: &str) -> Self {
        Self {
            entries,
            title: title.to_string(),
            scroll_offset: 0,
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::meeting::Meeting;
    use crate::models::note::Note;
    use crate::models::reflection::Reflection;
    use crate::models::summary::Summary;
    use crate::models::todo_item::TodoItem;
    use crate::services::timeline::{NoteWithContent, ReflectionWithContent, SummaryWithContent};
    use chrono::{DateTime, Duration, Utc};
    use ratatui::{Terminal, backend::TestBackend};
    use std::sync::OnceLock;

    static TZ_INIT: OnceLock<()> = OnceLock::new();

    fn ensure_utc_tz() {
        TZ_INIT.get_or_init(|| unsafe {
            std::env::set_var("TZ", "UTC");
        });
    }

    fn fixed_time() -> DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc)
    }

    fn make_todo_created_entry(label: &str) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::TodoItemCreated,
            metadata: "{}".to_string(),
            created_at: t,
            updated_at: t,
            entity: Some(TimelineEntity::TodoItem(TodoItem {
                id: Uuid::now_v7(),
                label: label.to_string(),
                status: crate::models::todo_item::TodoStatus::New,
                created_at: t,
                updated_at: t,
            })),
        }
    }

    fn make_status_change_entry(old_status: &str, new_status: &str) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::TodoItemStatusChanged,
            metadata: format!(
                r#"{{"old_status":"{}","new_status":"{}"}}"#,
                old_status, new_status
            ),
            created_at: t,
            updated_at: t,
            entity: Some(TimelineEntity::TodoItem(TodoItem {
                id: Uuid::now_v7(),
                label: "test todo".to_string(),
                status: crate::models::todo_item::TodoStatus::New,
                created_at: t,
                updated_at: t,
            })),
        }
    }

    fn make_reflection_created_entry(content: Option<&str>) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::ReflectionCreated,
            metadata: "{}".to_string(),
            created_at: t,
            updated_at: t,
            entity: Some(TimelineEntity::Reflection(ReflectionWithContent {
                reflection: Reflection {
                    id: Uuid::now_v7(),
                    about_id: None,
                    file_path: "test.md".to_string(),
                    created_at: t,
                    updated_at: t,
                },
                content: content.map(String::from),
            })),
        }
    }

    fn make_note_created_entry(content: Option<&str>) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::NoteCreated,
            metadata: "{}".to_string(),
            created_at: t,
            updated_at: t,
            entity: Some(TimelineEntity::Note(NoteWithContent {
                note: Note {
                    id: Uuid::now_v7(),
                    related_to_id: None,
                    file_path: "test.md".to_string(),
                    created_at: t,
                    updated_at: t,
                },
                content: content.map(String::from),
            })),
        }
    }

    fn make_meeting_created_entry(name: &str) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::MeetingCreated,
            metadata: "{}".to_string(),
            created_at: t,
            updated_at: t,
            entity: Some(TimelineEntity::Meeting(Meeting {
                id: Uuid::now_v7(),
                name: name.to_string(),
                scheduled_at: t,
                created_at: t,
                updated_at: t,
            })),
        }
    }

    fn make_summary_created_entry(content: Option<&str>) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::SummaryCreated,
            metadata: "{}".to_string(),
            created_at: t,
            updated_at: t,
            entity: Some(TimelineEntity::Summary(SummaryWithContent {
                summary: Summary {
                    id: Uuid::now_v7(),
                    file_path: "test.md".to_string(),
                    start: t,
                    end: t + Duration::hours(1),
                    created_at: t,
                    updated_at: t,
                },
                content: content.map(String::from),
            })),
        }
    }

    fn render_view(view: &mut TimelineView) -> String {
        ensure_utc_tz();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation failed");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");
        terminal.backend().to_string()
    }

    #[test]
    fn empty_timeline_render() {
        let mut view = TimelineView {
            entries: vec![],
            title: "Test Timeline".to_string(),
            scroll_offset: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("empty_timeline", output);
    }

    #[test]
    fn todo_timeline_render() {
        let mut view = TimelineView {
            entries: vec![make_todo_created_entry("Test todo item")],
            title: "Todo Timeline".to_string(),
            scroll_offset: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("todo_timeline", output);
    }

    #[test]
    fn todo_with_status_change_render() {
        let mut view = TimelineView {
            entries: vec![
                make_todo_created_entry("Test todo"),
                make_status_change_entry("NEW", "IN_PROGRESS"),
            ],
            title: "Todo Timeline".to_string(),
            scroll_offset: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("todo_status_change", output);
    }

    #[test]
    fn reflection_timeline_render() {
        let mut view = TimelineView {
            entries: vec![make_reflection_created_entry(Some(
                "This is reflection content",
            ))],
            title: "Reflection Timeline".to_string(),
            scroll_offset: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("reflection_timeline", output);
    }

    #[test]
    fn ctrl_d_scrolls_down() {
        let mut view = TimelineView {
            entries: vec![
                make_todo_created_entry("Item 1"),
                make_todo_created_entry("Item 2"),
                make_reflection_created_entry(Some("Content")),
            ],
            title: "Test".to_string(),
            scroll_offset: 0,
        };

        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        view.handle_key(key);

        assert!(view.scroll_offset() > 0, "scroll_offset should increase");
    }

    #[test]
    fn ctrl_u_scrolls_up() {
        let mut view = TimelineView {
            entries: vec![make_todo_created_entry("Test")],
            title: "Test".to_string(),
            scroll_offset: 10,
        };

        let key = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        view.handle_key(key);

        assert!(
            view.scroll_offset() < 10,
            "scroll_offset should decrease from 10"
        );
    }

    #[test]
    fn ctrl_u_clamped_at_zero() {
        let mut view = TimelineView {
            entries: vec![make_todo_created_entry("Test")],
            title: "Test".to_string(),
            scroll_offset: 0,
        };

        let key = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        view.handle_key(key);

        assert_eq!(view.scroll_offset(), 0, "scroll_offset should stay at 0");
    }

    #[test]
    fn esc_closes_view() {
        let mut view = TimelineView {
            entries: vec![],
            title: "Test".to_string(),
            scroll_offset: 0,
        };

        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let should_close = view.handle_key(key);

        assert!(should_close, "Esc should close the view");
    }

    #[test]
    fn q_closes_view() {
        let mut view = TimelineView {
            entries: vec![],
            title: "Test".to_string(),
            scroll_offset: 0,
        };

        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let should_close = view.handle_key(key);

        assert!(should_close, "q should close the view");
    }

    #[test]
    fn meeting_timeline_render() {
        let mut view = TimelineView {
            entries: vec![make_meeting_created_entry("Team Standup")],
            title: "Meeting Timeline".to_string(),
            scroll_offset: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("meeting_timeline", output);
    }

    #[test]
    fn note_timeline_render() {
        let mut view = TimelineView {
            entries: vec![make_note_created_entry(Some("This is a note content"))],
            title: "Note Timeline".to_string(),
            scroll_offset: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("note_timeline", output);
    }

    #[test]
    fn summary_timeline_render() {
        let mut view = TimelineView {
            entries: vec![make_summary_created_entry(Some("Weekly summary content"))],
            title: "Summary Timeline".to_string(),
            scroll_offset: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("summary_timeline", output);
    }
}
