use super::splash;
use crate::services::summary::SummaryService;
use crate::{models::summary::Summary, tui::summary_view::SummaryView};
use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, Row, Table},
};

pub struct SummariesView {
    items: Vec<Summary>,
    labels: Vec<String>,
    selected_index: Option<usize>,
    service: SummaryService,
    summary_view: Option<SummaryView>,
}

impl SummariesView {
    pub fn new(service: SummaryService) -> Self {
        Self {
            items: Vec::new(),
            labels: Vec::new(),
            selected_index: None,
            service,
            summary_view: None,
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.load_items().await
    }

    pub async fn refresh(&mut self) -> Result<()> {
        self.load_items().await
    }

    async fn load_items(&mut self) -> Result<()> {
        let summaries = self.service.list_summaries().await?;
        let mut labels = Vec::with_capacity(summaries.len());
        for summary in &summaries {
            labels.push(self.service.get_title(summary).await?);
        }
        self.items = summaries;
        self.labels = labels;
        self.clamp_selected_index();
        Ok(())
    }

    fn clamp_selected_index(&mut self) {
        if self.items.is_empty() {
            self.selected_index = None;
        } else {
            self.selected_index = Some(
                self.selected_index
                    .unwrap_or(0)
                    .min(self.items.len().saturating_sub(1)),
            );
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if let Some(summary_view) = &mut self.summary_view {
            if summary_view.handle_key(key) {
                self.summary_view = None;
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if !self.items.is_empty() => {
                let max = self.items.len().saturating_sub(1);
                let new_idx = match self.selected_index {
                    None => 0,
                    Some(idx) => idx.saturating_add(1).min(max),
                };
                self.selected_index = Some(new_idx);
            }
            KeyCode::Char('k') | KeyCode::Up if !self.items.is_empty() => {
                let new_idx = match self.selected_index {
                    None => 0,
                    Some(idx) => idx.saturating_sub(1),
                };
                self.selected_index = Some(new_idx);
            }
            KeyCode::Enter
                if let Some(idx) = self.selected_index
                    && let Some(item) = self.items.get(idx) =>
            {
                self.summary_view = Some(SummaryView::open(&self.service, item.id).await?);
            }
            _ => {}
        }

        Ok(false)
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(summary_view) = &mut self.summary_view {
            summary_view.render(frame, area);
            return;
        }
        let block = Block::default().borders(Borders::ALL).title("Summaries");
        let inner = block.inner(area);

        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.items.is_empty() {
            let screen = hjkl_splash::start_screen::StartScreen::build(env!("CARGO_PKG_VERSION"));
            splash::render(frame, inner, &screen);
        } else {
            let rows: Vec<Row> = self
                .items
                .iter()
                .zip(self.labels.iter())
                .enumerate()
                .map(|(i, (item, label))| {
                    let created_local = item.created_at.with_timezone(&Local);
                    let updated_local = item.updated_at.with_timezone(&Local);
                    let start = item.start.with_timezone(&Local);
                    let end = item.end.with_timezone(&Local);
                    let row = Row::new(vec![
                        Cell::new(label.clone()),
                        Cell::new(start.format("%Y-%m-%d %H:%M").to_string()),
                        Cell::new(end.format("%Y-%m-%d %H:%M").to_string()),
                        Cell::new(created_local.format("%Y-%m-%d %H:%M").to_string()),
                        Cell::new(updated_local.format("%Y-%m-%d %H:%M").to_string()),
                    ]);
                    if Some(i) == self.selected_index {
                        row.style(Style::default().reversed())
                    } else {
                        row
                    }
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Min(40),
                    Constraint::Length(18),
                    Constraint::Length(18),
                    Constraint::Length(18),
                    Constraint::Length(18),
                ],
            )
            .header(
                Row::new(vec![
                    Cell::new("Title"),
                    Cell::new("Start"),
                    Cell::new("End"),
                    Cell::new("Created"),
                    Cell::new("Updated"),
                ])
                .style(Style::default().bold()),
            );

            frame.render_widget(table, inner);
        }
    }

    pub fn is_modal_active(&self) -> bool {
        self.summary_view.is_some()
    }

    pub fn is_summary_view_active(&self) -> bool {
        self.summary_view.is_some()
    }
}
