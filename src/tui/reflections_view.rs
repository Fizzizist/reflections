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

#[cfg(test)]
impl ReflectionsView {
    pub fn set_items(&mut self, items: Vec<Reflection>) {
        self.items = items;
        self.labels = (0..self.items.len()).map(|_| String::new()).collect();
    }

    pub fn set_selected_index(&mut self, idx: usize) {
        self.selected_index = Some(idx);
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn clamp_selected_index_for_test(&mut self) {
        self.clamp_selected_index();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use uuid::Uuid;

    fn fixed_reflection() -> Reflection {
        let fixed_time = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        Reflection {
            id: Uuid::now_v7(),
            about_id: None,
            file_path: "2024/01/15/test.md".to_string(),
            created_at: fixed_time,
            updated_at: fixed_time,
        }
    }

    fn key_j() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)
    }

    fn key_k() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)
    }

    #[tokio::test]
    async fn j_selects_first_item() {
        let mut view = ReflectionsView::new();
        view.set_items(vec![fixed_reflection(), fixed_reflection()]);

        view.handle_key(key_j()).await.expect("handle_key failed");
        assert_eq!(view.selected_index(), Some(0));
    }

    #[tokio::test]
    async fn j_then_j_selects_second_item() {
        let mut view = ReflectionsView::new();
        view.set_items(vec![fixed_reflection(), fixed_reflection()]);

        view.handle_key(key_j()).await.expect("handle_key failed");
        view.handle_key(key_j()).await.expect("handle_key failed");
        assert_eq!(view.selected_index(), Some(1));
    }

    #[tokio::test]
    async fn k_at_first_stays_at_first() {
        let mut view = ReflectionsView::new();
        view.set_items(vec![fixed_reflection(), fixed_reflection()]);

        view.handle_key(key_j()).await.expect("handle_key failed");
        view.handle_key(key_k()).await.expect("handle_key failed");
        assert_eq!(view.selected_index(), Some(0));
    }

    #[tokio::test]
    async fn j_on_empty_list_does_nothing() {
        let mut view = ReflectionsView::new();
        view.handle_key(key_j()).await.expect("handle_key failed");
        assert_eq!(view.selected_index(), None);
    }

    #[tokio::test]
    async fn k_on_empty_list_does_nothing() {
        let mut view = ReflectionsView::new();
        view.handle_key(key_k()).await.expect("handle_key failed");
        assert_eq!(view.selected_index(), None);
    }

    #[test]
    fn clamp_selected_index_after_items_disappear() {
        let mut view = ReflectionsView::new();
        view.set_items(vec![fixed_reflection(), fixed_reflection()]);
        view.set_selected_index(1);
        assert_eq!(view.selected_index(), Some(1));

        view.set_items(vec![fixed_reflection()]);
        view.clamp_selected_index_for_test();
        assert_eq!(view.selected_index(), Some(0));
    }
}
