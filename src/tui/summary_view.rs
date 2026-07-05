use crate::models::summary::Summary;
use crate::services::summary::SummaryService;
use crate::tui::editor;
use crate::tui::highlight::build_renderer;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
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

pub struct SummaryView {
    summary: Summary,
    content: String,
    scroll_offset: usize,
    editor_fn: editor::EditorFn,
}

impl SummaryView {
    pub async fn open(
        service: &SummaryService,
        summary: Summary,
        editor_fn: editor::EditorFn,
    ) -> Result<Self> {
        let content = service.get_content(summary.id).await?;
        Ok(Self {
            summary,
            content,
            scroll_offset: 0,
            editor_fn,
        })
    }

    pub async fn refresh(&mut self, service: &SummaryService) -> Result<()> {
        self.content = service.get_content(self.summary.id).await?;
        Ok(())
    }

    pub async fn handle_key(
        &mut self,
        key: KeyEvent,
        service: &mut SummaryService,
    ) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Ok(true),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // if it can't actually get a height, just use a reasonable default
                let (_, height) = terminal::size().unwrap_or((0, 24));
                self.scroll_down((height / 2).into());
                Ok(false)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let (_, height) = terminal::size().unwrap_or((0, 24));
                self.scroll_up((height / 2).into());
                Ok(false)
            }
            KeyCode::Char('e') => {
                editor::edit(service, &self.summary, &self.editor_fn).await?;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Summary");
        let inner = block.inner(area);

        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let text = into_text_with_renderer(&self.content, &RENDERER);
        let content_height = text.lines.len();
        let max_scroll = content_height.saturating_sub(inner.height as usize);

        self.scroll_offset = self.scroll_offset.min(max_scroll);

        let paragraph = Paragraph::new(text)
            .scroll((self.scroll_offset as u16, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, inner);
    }

    fn scroll_down(&mut self, half_page: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(half_page);
    }

    fn scroll_up(&mut self, half_page: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(half_page);
    }
}
