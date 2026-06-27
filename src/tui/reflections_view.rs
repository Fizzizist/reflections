use super::splash;
use crate::models::reflection::Reflection;
use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, Row, Table},
};

pub struct ReflectionsView {
    items: Vec<Reflection>,
    labels: Vec<String>,
    selected_index: Option<usize>,
}

impl ReflectionsView {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            labels: Vec::new(),
            selected_index: None,
        }
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

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
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
            _ => {}
        }

        Ok(())
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Reflections");
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
                    let updated_local = item.updated_at.with_timezone(&Local);
                    let row = Row::new(vec![
                        Cell::new(label.clone()),
                        Cell::new(updated_local.format("%Y-%m-%d %H:%M").to_string()),
                    ]);
                    if Some(i) == self.selected_index {
                        row.style(Style::default().reversed())
                    } else {
                        row
                    }
                })
                .collect();

            let table = Table::new(rows, [Constraint::Min(40), Constraint::Length(18)]).header(
                Row::new(vec![Cell::new("Label"), Cell::new("Updated")])
                    .style(Style::default().bold()),
            );

            frame.render_widget(table, inner);
        }
    }

    pub fn is_modal_active(&self) -> bool {
        false
    }

    pub fn set_items_with_labels(&mut self, items: Vec<Reflection>, labels: Vec<String>) {
        self.items = items;
        self.labels = labels;
        self.clamp_selected_index();
    }
}
