use super::splash;
use crate::models::todo_item::{TodoItem, TodoStatus};
use crate::services::note::NoteService;
use crate::services::reflection::ReflectionService;
use crate::services::todo::TodoService;
use crate::tui::editor::EditorFn;
use crate::tui::input_modal::InputModal;
use crate::tui::status_modal::StatusModal;
use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, Row, Table},
};

pub struct TodoListView {
    items: Vec<TodoItem>,
    service: TodoService,
    input_modal: InputModal,
    selected_index: Option<usize>,
    show_all: bool,
    status_modal: StatusModal,
    reflection_service: ReflectionService,
    editor_fn: EditorFn,
    note_service: NoteService,
    note_editor_fn: EditorFn,
}

impl TodoListView {
    pub fn new(
        service: TodoService,
        reflection_service: ReflectionService,
        editor_fn: EditorFn,
        note_service: NoteService,
        note_editor_fn: EditorFn,
    ) -> Self {
        Self {
            items: Vec::new(),
            service,
            input_modal: InputModal::new(),
            selected_index: None,
            show_all: false,
            status_modal: StatusModal::new(),
            reflection_service,
            editor_fn,
            note_service,
            note_editor_fn,
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
        if self.status_modal.is_active() {
            if let Some(status) = self.status_modal.handle_key(key) {
                if let Some(id) = self.status_modal.target_item_id() {
                    self.service.update_todo_status(id, status).await?;
                    self.load_items().await?;
                    self.clamp_selected_index();
                }
                self.status_modal.close();
            }
            return Ok(false);
        }

        if self.input_modal.is_active() {
            if let Some(input) = self.input_modal.handle_key(key) {
                let trimmed = input.trim();
                if !trimmed.is_empty() {
                    self.submit_todo(trimmed).await?;
                }
                self.input_modal.close();
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
            KeyCode::Char('u')
                if let Some(idx) = self.selected_index
                    && let Some(item) = self.items.get(idx) =>
            {
                self.status_modal.open(item.id, &item.status);
            }
            KeyCode::Char('a') => {
                self.input_modal.open();
            }
            KeyCode::Char('A') => {
                self.show_all = !self.show_all;
                self.load_items().await?;
                self.clamp_selected_index();
            }
            KeyCode::Char('r')
                if let Some(idx) = self.selected_index
                    && let Some(item) = self.items.get(idx) =>
            {
                return super::editor::create_and_edit_reflection(
                    &mut self.reflection_service,
                    &self.editor_fn,
                    Some(item.id),
                )
                .await;
            }
            KeyCode::Char('n')
                if let Some(idx) = self.selected_index
                    && let Some(item) = self.items.get(idx) =>
            {
                return super::editor::create_and_edit(
                    &mut self.note_service,
                    &self.note_editor_fn,
                    Some(item.id),
                )
                .await;
            }
            _ => {}
        }

        Ok(false)
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("TODO List");
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
                .enumerate()
                .map(|(i, item)| {
                    let local_time = item.created_at.with_timezone(&Local);
                    let updated_time = item.updated_at.with_timezone(&Local);
                    let status_cell = match item.status {
                        TodoStatus::New => Cell::new("NEW"),
                        TodoStatus::InProgress => Cell::new("IN_PROGRESS"),
                        TodoStatus::Done => Cell::new("DONE"),
                    };
                    let row = Row::new(vec![
                        status_cell,
                        Cell::new(item.label.clone()),
                        Cell::new(local_time.format("%Y-%m-%d %H:%M").to_string()),
                        Cell::new(updated_time.format("%Y-%m-%d %H:%M").to_string()),
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
                    Constraint::Length(12),
                    Constraint::Min(20),
                    Constraint::Length(18),
                    Constraint::Length(18),
                ],
            )
            .header(
                Row::new(vec![
                    Cell::new("Status"),
                    Cell::new("Label"),
                    Cell::new("Created"),
                    Cell::new("Updated"),
                ])
                .style(Style::default().bold()),
            );

            frame.render_widget(table, inner);
        }

        if self.input_modal.is_active() {
            self.input_modal.render(frame, area);
        }

        if self.status_modal.is_active() {
            self.status_modal.render(frame, area);
        }
    }

    async fn load_items(&mut self) -> Result<()> {
        if self.show_all {
            self.items = self.service.list_all_todo_items().await?;
        } else {
            self.items = self.service.list_todo_items().await?;
        }
        Ok(())
    }

    async fn submit_todo(&mut self, label: &str) -> Result<()> {
        self.service.create_todo_item(label).await?;
        self.load_items().await?;
        Ok(())
    }

    pub fn is_modal_active(&self) -> bool {
        self.input_modal.is_active() || self.status_modal.is_active()
    }
}

#[cfg(test)]
impl TodoListView {
    pub fn set_items(&mut self, items: Vec<TodoItem>) {
        self.items = items;
    }

    pub fn open_modal(&mut self) {
        self.input_modal.open();
    }

    pub fn set_selected_index(&mut self, idx: usize) {
        self.selected_index = Some(idx);
    }

    pub fn open_status_modal(&mut self) {
        if let Some(item) = self.items.first() {
            self.status_modal.open(item.id, &item.status);
        } else {
            let dummy_id = uuid::Uuid::now_v7();
            self.status_modal.open(dummy_id, &TodoStatus::New);
        }
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn is_status_modal_active(&self) -> bool {
        self.status_modal.is_active()
    }

    pub fn is_show_all(&self) -> bool {
        self.show_all
    }

    pub fn clamp_selected_index_for_test(&mut self) {
        self.clamp_selected_index();
    }

    pub fn set_editor_fn(&mut self, f: EditorFn) {
        self.editor_fn = f;
    }

    pub fn set_note_editor_fn(&mut self, f: EditorFn) {
        self.note_editor_fn = f;
    }
}
