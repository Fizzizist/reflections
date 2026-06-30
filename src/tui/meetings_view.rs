use super::meeting_modal::MeetingModal;
use crate::models::meeting::Meeting;
use crate::services::meeting::MeetingService;
use crate::services::reflection::ReflectionService;
use crate::tui::editor::EditorFn;
use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, Row, Table},
};

pub struct MeetingsView {
    items: Vec<Meeting>,
    service: MeetingService,
    meeting_modal: MeetingModal,
    selected_index: Option<usize>,
    reflection_service: ReflectionService,
    editor_fn: EditorFn,
}

impl MeetingsView {
    pub fn new(
        service: MeetingService,
        reflection_service: ReflectionService,
        editor_fn: EditorFn,
    ) -> Self {
        Self {
            items: Vec::new(),
            service,
            meeting_modal: MeetingModal::new(),
            selected_index: None,
            reflection_service,
            editor_fn,
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.load_items().await
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
        if self.meeting_modal.is_active() {
            if let Some((name, scheduled_at)) = self.meeting_modal.handle_key(key) {
                if let Err(e) = self.service.create_meeting(&name, scheduled_at).await {
                    drop(e);
                    self.meeting_modal.close();
                    return Ok(false);
                }
                self.load_items().await?;
                self.clamp_selected_index();
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
            KeyCode::Char('a') => {
                self.meeting_modal.open();
            }
            KeyCode::Char('r')
                if let Some(idx) = self.selected_index
                    && let Some(item) = self.items.get(idx) =>
            {
                let reflection = self
                    .reflection_service
                    .create_reflection(Some(item.id))
                    .await?;
                let path = self.reflection_service.full_path(&reflection.file_path);
                if let Err(e) = (self.editor_fn)(&path) {
                    self.reflection_service
                        .cleanup_reflection(reflection.id)
                        .await?;
                    return Err(e);
                }
                self.reflection_service
                    .cleanup_reflection(reflection.id)
                    .await?;
                return Ok(true);
            }
            _ => {}
        }

        Ok(false)
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Meetings");
        let inner = block.inner(area);

        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.items.is_empty() {
            let screen = hjkl_splash::start_screen::StartScreen::build(env!("CARGO_PKG_VERSION"));
            super::splash::render(frame, inner, &screen);
        } else {
            let rows: Vec<Row> = self
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let scheduled_local = item.scheduled_at.with_timezone(&Local);
                    let created_local = item.created_at.with_timezone(&Local);
                    let updated_local = item.updated_at.with_timezone(&Local);
                    let row = Row::new(vec![
                        Cell::new(item.name.clone()),
                        Cell::new(scheduled_local.format("%Y-%m-%d %H:%M").to_string()),
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
                    Constraint::Min(20),
                    Constraint::Length(18),
                    Constraint::Length(18),
                    Constraint::Length(18),
                ],
            )
            .header(
                Row::new(vec![
                    Cell::new("Name"),
                    Cell::new("Scheduled"),
                    Cell::new("Created"),
                    Cell::new("Updated"),
                ])
                .style(Style::default().bold()),
            );

            frame.render_widget(table, inner);
        }

        if self.meeting_modal.is_active() {
            self.meeting_modal.render(frame, area);
        }
    }

    async fn load_items(&mut self) -> Result<()> {
        self.items = self.service.list_meetings_for_today().await?;
        Ok(())
    }

    pub fn is_modal_active(&self) -> bool {
        self.meeting_modal.is_active()
    }
}

#[cfg(test)]
impl MeetingsView {
    pub fn set_items(&mut self, items: Vec<Meeting>) {
        self.items = items;
    }

    pub fn open_modal(&mut self) {
        self.meeting_modal.open();
    }

    pub fn set_datetime_for_test(
        &mut self,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) {
        self.meeting_modal
            .set_datetime_for_test(year, month, day, hour, minute);
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn set_selected_index(&mut self, idx: usize) {
        self.selected_index = Some(idx);
    }

    pub fn clamp_selected_index_for_test(&mut self) {
        self.clamp_selected_index();
    }

    pub fn set_editor_fn(&mut self, f: EditorFn) {
        self.editor_fn = f;
    }
}
