use crate::services::editable::{EditableEntityRecord, ReadableEntity};
use crate::tui::editor;
use crate::tui::highlight::build_renderer;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::sync::LazyLock;
use the_other_tui_markdown::{Renderer, Theme, into_text_with_renderer};

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

pub struct ContentView<T: EditableEntityRecord, S: ReadableEntity<Entity = T>> {
    entity: T,
    content: String,
    scroll_offset: usize,
    viewport_height: usize,
    title: &'static str,
    editor_fn: editor::EditorFn,
    _service: std::marker::PhantomData<S>,
}

impl<T: EditableEntityRecord, S: ReadableEntity<Entity = T>> ContentView<T, S> {
    pub async fn open(
        service: &S,
        entity: T,
        title: &'static str,
        editor_fn: editor::EditorFn,
    ) -> Result<Self> {
        let content = service.get_content(entity.id()).await?;
        Ok(Self {
            entity,
            content,
            scroll_offset: 0,
            viewport_height: 0,
            title,
            editor_fn,
            _service: std::marker::PhantomData,
        })
    }

    pub async fn refresh(&mut self, service: &S) -> Result<()> {
        self.content = service.get_content(self.entity.id()).await?;
        Ok(())
    }

    fn effective_viewport_height(&self) -> usize {
        if self.viewport_height > 0 {
            self.viewport_height
        } else {
            24
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent, service: &mut S) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Ok(true),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let height = self.effective_viewport_height();
                self.scroll_down(height / 2);
                Ok(false)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let height = self.effective_viewport_height();
                self.scroll_up(height / 2);
                Ok(false)
            }
            KeyCode::Char('e') => {
                editor::edit(service, &self.entity, &self.editor_fn).await?;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(self.title);
        let inner = block.inner(area);

        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        self.viewport_height = inner.height as usize;

        let text = into_text_with_renderer(&self.content, &RENDERER);

        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let content_height = paragraph.line_count(inner.width);
        let max_scroll = content_height.saturating_sub(inner.height as usize);

        self.scroll_offset = self.scroll_offset.min(max_scroll);

        let paragraph = paragraph.scroll((self.scroll_offset as u16, 0));

        frame.render_widget(paragraph, inner);
    }

    fn scroll_down(&mut self, half_page: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(half_page);
    }

    fn scroll_up(&mut self, half_page: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(half_page);
    }
}

#[cfg(test)]
impl<T: EditableEntityRecord + Clone, S: ReadableEntity<Entity = T>> ContentView<T, S> {
    pub fn new_for_test(entity: T, content: &str, title: &'static str) -> Self {
        Self {
            entity,
            content: content.to_string(),
            scroll_offset: 0,
            viewport_height: 0,
            title,
            editor_fn: editor::default_editor_fn(),
            _service: std::marker::PhantomData,
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::reflection::Reflection;
    use crate::models::summary::Summary;
    use crate::services::reflection::ReflectionService;
    use crate::services::summary::SummaryService;
    use chrono::{DateTime, Duration, Utc};
    use ratatui::{Terminal, backend::TestBackend};
    use tempfile::tempdir;
    use uuid::Uuid;

    fn fixed_time() -> DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc)
    }

    fn make_summary() -> Summary {
        let t = fixed_time();
        Summary {
            id: Uuid::now_v7(),
            file_path: "test.md".to_string(),
            start: t,
            end: t + Duration::hours(1),
            created_at: t,
            updated_at: t,
        }
    }

    fn make_reflection() -> Reflection {
        let t = fixed_time();
        Reflection {
            id: Uuid::now_v7(),
            about_id: None,
            file_path: "test.md".to_string(),
            created_at: t,
            updated_at: t,
        }
    }

    fn render_narrow_summary(view: &mut ContentView<Summary, SummaryService>) -> String {
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
        let mut view = ContentView::<Summary, SummaryService>::new_for_test(
            make_summary(),
            long_content,
            "Summary",
        );

        render_narrow_summary(&mut view);
        let initial_offset = view.scroll_offset();

        view.scroll_offset = 100;
        render_narrow_summary(&mut view);
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
        let mut view = ContentView::<Summary, SummaryService>::new_for_test(
            make_summary(),
            long_content,
            "Summary",
        );

        render_narrow_summary(&mut view);
        let max_scroll = view.scroll_offset();

        view.scroll_offset = 100;
        render_narrow_summary(&mut view);

        assert_eq!(
            view.scroll_offset(),
            max_scroll,
            "scroll_offset should be clamped to max_scroll after setting high value"
        );
    }

    #[tokio::test]
    async fn scroll_step_uses_viewport_height_after_render() {
        let db_dir = tempdir().expect("create tempdir failed");
        let db_path = db_dir.path().join("test.db");
        Database::open_path(&db_path).await.expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let mut svc = SummaryService::new(db_path, root_dir.path().to_path_buf());

        let long_content = "This is a very long line that will definitely wrap when rendered in a narrow terminal width of thirty columns and it keeps going on and on to ensure it exceeds the viewport height so we can verify scrolling works correctly with wrapped lines that produce more visual lines than logical lines";
        let mut view = ContentView::<Summary, SummaryService>::new_for_test(
            make_summary(),
            long_content,
            "Summary",
        );

        render_narrow_summary(&mut view);
        let expected_step = view.viewport_height / 2;

        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        view.handle_key(key, &mut svc)
            .await
            .expect("handle_key failed");

        assert_eq!(
            view.scroll_offset(),
            expected_step,
            "scroll step should match viewport_height / 2, not fallback 12"
        );
    }

    #[tokio::test]
    async fn no_scroll_when_content_fits_in_viewport() {
        let db_dir = tempdir().expect("create tempdir failed");
        let db_path = db_dir.path().join("test.db");
        Database::open_path(&db_path).await.expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let mut svc = SummaryService::new(db_path, root_dir.path().to_path_buf());

        let mut view = ContentView::<Summary, SummaryService>::new_for_test(
            make_summary(),
            "Short content",
            "Summary",
        );

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
        view.handle_key(key, &mut svc)
            .await
            .expect("handle_key failed");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");
        assert_eq!(
            view.scroll_offset(),
            0,
            "Ctrl+d should have no effect when content fits"
        );

        let key = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        view.handle_key(key, &mut svc)
            .await
            .expect("handle_key failed");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");
        assert_eq!(
            view.scroll_offset(),
            0,
            "Ctrl+u should have no effect when content fits"
        );
    }

    #[tokio::test]
    async fn content_view_works_with_reflection_service() {
        let db_dir = tempdir().expect("create tempdir failed");
        let db_path = db_dir.path().join("test.db");
        Database::open_path(&db_path).await.expect("db open failed");
        let root_dir = tempdir().expect("create tempdir failed");
        let _svc = ReflectionService::new(db_path, root_dir.path().to_path_buf());

        let mut view = ContentView::<Reflection, ReflectionService>::new_for_test(
            make_reflection(),
            "Reflection content",
            "Reflection",
        );

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation failed");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("draw failed");

        assert_eq!(view.scroll_offset(), 0);
        assert_eq!(view.title, "Reflection");
    }
}
