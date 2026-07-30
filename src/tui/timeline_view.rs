use crate::models::DiffEntry;
use crate::models::event::EventType;
use crate::services::timeline::{TimelineEntity, TimelineEntry, TimelineService};
use crate::tui::highlight::build_renderer;
use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};
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
    viewport_height: usize,
}

impl TimelineView {
    pub async fn open(service: &TimelineService, entity_id: Uuid, title: &str) -> Result<Self> {
        let entries = service.get_entity_timeline(entity_id).await?;
        Ok(Self {
            entries,
            title: title.to_string(),
            scroll_offset: 0,
            viewport_height: 0,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => true,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let height = if self.viewport_height > 0 {
                    self.viewport_height
                } else {
                    24
                };
                self.scroll_down(height / 2);
                false
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let height = if self.viewport_height > 0 {
                    self.viewport_height
                } else {
                    24
                };
                self.scroll_up(height / 2);
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

        self.viewport_height = inner.height as usize;

        let markdown_content = if self.entries.is_empty() {
            "No events found for this entity.".to_string()
        } else {
            self.build_markdown_content()
        };

        let text = into_text_with_renderer(&markdown_content, &RENDERER);

        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let content_height = paragraph.line_count(inner.width);
        let max_scroll = content_height.saturating_sub(inner.height as usize);

        self.scroll_offset = self.scroll_offset.min(max_scroll);

        let paragraph = paragraph.scroll((self.scroll_offset as u16, 0));

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
            EventType::ReflectionUpdated => {
                format_diff_entry("💭 Reflection Update", &time_str, &entry.diff)
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
                format_diff_entry("📊 Summary Update", &time_str, &entry.diff)
            }
        }
    }

    fn parse_status_change(&self, metadata: &str) -> String {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(metadata) {
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

fn format_diff_entry(emoji_title: &str, time_str: &str, diff: &Option<Vec<DiffEntry>>) -> String {
    let title = format!("## {} — {}", emoji_title, time_str);
    match diff {
        Some(entries) if !entries.is_empty() => {
            let lines: Vec<String> = entries
                .iter()
                .map(|d| match (&d.left, &d.right) {
                    (Some(l), Some(_r)) => format!(" {}", l),
                    (Some(l), None) => format!("-{}", l),
                    (None, Some(r)) => format!("+{}", r),
                    (None, None) => String::new(),
                })
                .collect();
            format!("{}\n\n```\n{}\n```\n", title, lines.join("\n"))
        }
        _ => format!("{}\n\n (no diff content) \n", title),
    }
}

#[cfg(test)]
impl TimelineView {
    pub fn new_for_test(entries: Vec<TimelineEntry>, title: &str) -> Self {
        Self {
            entries,
            title: title.to_string(),
            scroll_offset: 0,
            viewport_height: 0,
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
            diff: None,
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
            diff: None,
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
            diff: None,
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
            diff: None,
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
            diff: None,
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
            diff: None,
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

    fn make_reflection_updated_entry(diff: Vec<DiffEntry>) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::ReflectionUpdated,
            metadata: serde_json::to_string(&diff).expect("serialize diff failed"),
            diff: Some(diff),
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
                content: Some("updated reflection content".to_string()),
            })),
        }
    }

    fn make_summary_updated_entry(diff: Vec<DiffEntry>) -> TimelineEntry {
        let t = fixed_time();
        TimelineEntry {
            event_id: Uuid::now_v7(),
            entity_id: Uuid::now_v7(),
            event_type: EventType::SummaryUpdated,
            metadata: serde_json::to_string(&diff).expect("serialize diff failed"),
            diff: Some(diff),
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
                content: Some("updated summary content".to_string()),
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
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
            viewport_height: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("summary_timeline", output);
    }

    #[test]
    fn reflection_updated_render() {
        let diff = vec![
            DiffEntry {
                left: Some("old line".to_string()),
                right: Some("old line".to_string()),
            },
            DiffEntry {
                left: None,
                right: Some("new line".to_string()),
            },
            DiffEntry {
                left: Some("removed line".to_string()),
                right: None,
            },
        ];
        let mut view = TimelineView {
            entries: vec![make_reflection_updated_entry(diff)],
            title: "Reflection Timeline".to_string(),
            scroll_offset: 0,
            viewport_height: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("reflection_updated_timeline", output);
    }

    #[test]
    fn summary_updated_render() {
        let diff = vec![
            DiffEntry {
                left: Some("same".to_string()),
                right: Some("same".to_string()),
            },
            DiffEntry {
                left: Some("removed".to_string()),
                right: None,
            },
            DiffEntry {
                left: None,
                right: Some("added".to_string()),
            },
        ];
        let mut view = TimelineView {
            entries: vec![make_summary_updated_entry(diff)],
            title: "Summary Timeline".to_string(),
            scroll_offset: 0,
            viewport_height: 0,
        };
        let output = render_view(&mut view);
        insta::assert_snapshot!("summary_updated_timeline", output);
    }

    fn render_narrow(view: &mut TimelineView) -> String {
        ensure_utc_tz();
        let backend = TestBackend::new(30, 10);
        let mut terminal = Terminal::new(backend).expect("terminal creation failed");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");
        terminal.backend().to_string()
    }

    #[test]
    fn scroll_to_bottom_with_wrapped_content() {
        let long_content = "This is a very long line that will definitely wrap when rendered in a narrow terminal width of thirty columns and it keeps going on and on to ensure it exceeds the viewport height so we can verify scrolling works correctly with wrapped lines that produce more visual lines than logical lines";
        let mut view = TimelineView {
            entries: vec![make_reflection_created_entry(Some(long_content))],
            title: "Test".to_string(),
            scroll_offset: 0,
            viewport_height: 0,
        };

        render_narrow(&mut view);
        let initial_offset = view.scroll_offset();

        view.scroll_offset = 100;
        render_narrow(&mut view);
        let max_after_clamp = view.scroll_offset();

        assert!(
            max_after_clamp > 0,
            "max_scroll should be > 0 with wrapped content"
        );
        assert!(
            max_after_clamp < 100,
            "scroll_offset should be clamped below 100"
        );
        assert_eq!(initial_offset, 0, "initial scroll_offset should be 0");
    }

    #[test]
    fn scroll_offset_clamped_to_max_with_wrapping() {
        let long_content = "AaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaBbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbCccccccccccccccccccccccccccccc";
        let mut view = TimelineView {
            entries: vec![make_reflection_created_entry(Some(long_content))],
            title: "Test".to_string(),
            scroll_offset: 0,
            viewport_height: 0,
        };

        render_narrow(&mut view);
        let max_scroll = view.scroll_offset();

        for _ in 0..20 {
            let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
            view.handle_key(key);
            render_narrow(&mut view);
        }

        assert_eq!(
            view.scroll_offset(),
            max_scroll,
            "scroll_offset should be clamped to max_scroll after repeated scrolling"
        );
    }

    #[test]
    fn scroll_step_uses_viewport_height_after_render() {
        let long_content = "This is a very long line that will definitely wrap when rendered in a narrow terminal width of thirty columns and it keeps going on and on to ensure it exceeds the viewport height so we can verify scrolling works correctly with wrapped lines that produce more visual lines than logical lines";
        let mut view = TimelineView {
            entries: vec![make_reflection_created_entry(Some(long_content))],
            title: "Test".to_string(),
            scroll_offset: 0,
            viewport_height: 0,
        };

        render_narrow(&mut view);

        let expected_step = view.viewport_height / 2;

        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        view.handle_key(key);

        assert_eq!(
            view.scroll_offset(),
            expected_step,
            "scroll step should match viewport_height / 2, not fallback 12"
        );
    }

    #[test]
    fn no_scroll_when_content_fits_in_viewport() {
        let mut view = TimelineView {
            entries: vec![make_todo_created_entry("Short")],
            title: "Test".to_string(),
            scroll_offset: 0,
            viewport_height: 0,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation failed");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");

        assert_eq!(
            view.scroll_offset(),
            0,
            "max_scroll should be 0 for short content"
        );

        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        view.handle_key(key);
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");
        assert_eq!(
            view.scroll_offset(),
            0,
            "Ctrl+d should have no effect when content fits"
        );

        let key = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        view.handle_key(key);
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");
        assert_eq!(
            view.scroll_offset(),
            0,
            "Ctrl+u should have no effect when content fits"
        );
    }
}
