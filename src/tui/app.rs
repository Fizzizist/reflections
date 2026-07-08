use crate::services::meeting::MeetingService;
use crate::services::note::NoteService;
use crate::services::reflection::ReflectionService;
use crate::services::summary::SummaryService;
use crate::services::timeline::TimelineService;
use crate::services::todo::TodoService;
use crate::tui::editor::EditorFn;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::Tabs;

use super::meetings_view::MeetingsView;
use super::reflections_view::ReflectionsView;
use super::summaries_view::SummariesView;
use super::todo_list::TodoListView;
use anyhow::Result;
use futures::stream::StreamExt;
use std::io;
use std::path::PathBuf;

use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
    KeyModifiers,
};
use ratatui::Terminal;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::CrosstermBackend;

#[derive(Clone, Copy)]
enum Tab {
    TodoList,
    Meetings,
    Reflections,
    Summaries,
}

const TABS: [Tab; 4] = [
    Tab::TodoList,
    Tab::Meetings,
    Tab::Reflections,
    Tab::Summaries,
];

pub struct App {
    todo_list_view: TodoListView,
    meetings_view: MeetingsView,
    reflections_view: ReflectionsView,
    summaries_view: SummariesView,
    reflection_service: ReflectionService,
    note_service: NoteService,
    editor_fn: EditorFn,
    active_tab: Tab,
    pending_g_prefix: bool,
}

impl App {
    pub fn new(db_path: PathBuf, root_dir: PathBuf) -> Self {
        let editor_fn = crate::tui::editor::default_editor_fn();
        let todo_service = TodoService::new(db_path.clone());
        let meeting_service = MeetingService::new(db_path.clone());
        let note_service = NoteService::new(db_path.clone(), root_dir.clone());
        let reflection_service = ReflectionService::new(db_path.clone(), root_dir.clone());
        let timeline_service = TimelineService::new(db_path.clone(), root_dir.clone());
        let summary_service = SummaryService::new(db_path, root_dir);
        Self {
            todo_list_view: TodoListView::new(
                todo_service,
                reflection_service.clone(),
                editor_fn.clone(),
                note_service.clone(),
                timeline_service.clone(),
            ),
            meetings_view: MeetingsView::new(
                meeting_service,
                reflection_service.clone(),
                editor_fn.clone(),
                note_service.clone(),
                timeline_service.clone(),
            ),
            reflections_view: ReflectionsView::new(reflection_service.clone()),
            summaries_view: SummariesView::new(summary_service.clone(), editor_fn.clone()),
            reflection_service,
            note_service,
            editor_fn,
            active_tab: Tab::TodoList,
            pending_g_prefix: false,
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.todo_list_view.init().await?;
        self.meetings_view.init().await?;
        self.reflections_view.init().await?;
        self.summaries_view.init().await?;
        Ok(())
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.pending_g_prefix {
            return self.handle_g_prefix(key).await;
        }

        if !self.is_modal_active()
            && let Some(needs_clear) = self.handle_global_key(key).await?
        {
            if needs_clear {
                self.reflections_view.refresh().await?;
            }
            return Ok(needs_clear);
        }

        self.delegate_to_tab(key).await
    }

    async fn set_tab(&mut self, tab: Tab) {
        self.active_tab = tab;
        let _ = match tab {
            Tab::Summaries => self.summaries_view.refresh().await,
            _ => Ok(()),
        };
    }

    async fn handle_g_prefix(&mut self, key: KeyEvent) -> Result<bool> {
        self.pending_g_prefix = false;
        match key.code {
            KeyCode::Char('t') => {
                self.set_tab(self.next_tab()).await;
            }
            KeyCode::Char('T') => {
                self.active_tab = self.prev_tab();
            }
            _ => {
                return self.delegate_to_tab(key).await;
            }
        }
        Ok(false)
    }

    async fn handle_global_key(&mut self, key: KeyEvent) -> Result<Option<bool>> {
        match key.code {
            KeyCode::Char('g') => {
                self.pending_g_prefix = true;
                Ok(Some(false))
            }
            KeyCode::Char('R') => {
                let needs_clear = crate::tui::editor::create_and_edit_reflection(
                    &mut self.reflection_service,
                    &self.editor_fn,
                    None,
                )
                .await?;
                Ok(Some(needs_clear))
            }
            KeyCode::Char('N') => {
                let needs_clear = crate::tui::editor::create_and_edit(
                    &mut self.note_service,
                    &self.editor_fn,
                    None,
                )
                .await?;
                Ok(Some(needs_clear))
            }
            _ => Ok(None),
        }
    }

    async fn delegate_to_tab(&mut self, key: KeyEvent) -> Result<bool> {
        let needs_clear = match self.active_tab {
            Tab::TodoList => self.todo_list_view.handle_key(key).await?,
            Tab::Meetings => self.meetings_view.handle_key(key).await?,
            Tab::Reflections => self.reflections_view.handle_key(key).await?,
            Tab::Summaries => self.summaries_view.handle_key(key).await?,
        };
        if needs_clear {
            self.reflections_view.refresh().await?;
        }
        Ok(needs_clear)
    }

    fn next_tab(&self) -> Tab {
        let current = self.active_tab as usize;
        TABS[(current + 1) % TABS.len()]
    }

    fn prev_tab(&self) -> Tab {
        let current = self.active_tab as usize;
        TABS[(current + TABS.len() - 1) % TABS.len()]
    }

    fn is_modal_active(&self) -> bool {
        match self.active_tab {
            Tab::TodoList => self.todo_list_view.is_modal_active(),
            Tab::Meetings => self.meetings_view.is_modal_active(),
            Tab::Reflections => self.reflections_view.is_modal_active(),
            Tab::Summaries => self.summaries_view.is_modal_active(),
        }
    }
}

#[cfg(test)]
impl App {
    pub fn set_app_editor_fn(&mut self, f: EditorFn) {
        self.editor_fn = f;
    }

    pub fn set_todo_editor_fn(&mut self, f: EditorFn) {
        self.todo_list_view.set_editor_fn(f);
    }

    pub fn set_meeting_editor_fn(&mut self, f: EditorFn) {
        self.meetings_view.set_editor_fn(f);
    }
}

pub fn render_app(app: &mut App, frame: &mut ratatui::Frame) {
    let overlay_active = match app.active_tab {
        Tab::TodoList => app.todo_list_view.is_timeline_active(),
        Tab::Meetings => app.meetings_view.is_timeline_active(),
        Tab::Reflections => false,
        Tab::Summaries => app.summaries_view.is_summary_view_active(),
    };

    let view_area = if overlay_active {
        frame.area()
    } else {
        let chunks =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(frame.area());

        let tab_titles = vec!["Todo List", "Meetings", "Reflections", "Summaries"];
        let tabs = Tabs::new(tab_titles)
            .select(app.active_tab as usize)
            .highlight_style(ratatui::style::Style::default().reversed());
        frame.render_widget(tabs, chunks[0]);

        chunks[1]
    };

    match app.active_tab {
        Tab::TodoList => app.todo_list_view.render(frame, view_area),
        Tab::Meetings => app.meetings_view.render(frame, view_area),
        Tab::Reflections => app.reflections_view.render(frame, view_area),
        Tab::Summaries => app.summaries_view.render(frame, view_area),
    }
}

fn is_global_quit(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    db_path: PathBuf,
    root_dir: PathBuf,
) -> Result<()> {
    let mut app = App::new(db_path, root_dir);
    app.init().await?;
    terminal.draw(|frame| render_app(&mut app, frame))?;

    let mut terminal_events = EventStream::new();
    loop {
        tokio::select! {
            event = terminal_events.next() => {
                match event {
                    Some(Ok(terminal_event)) => {
                        if let Event::Key(key) = terminal_event {
                            if is_global_quit(&key) {
                                break;
                            }
                            let needs_clear = app.handle_key(key).await?;
                            if needs_clear {
                                terminal.clear()?;
                            }
                            terminal.draw(|frame| render_app(&mut app, frame))?;
                        }
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!("terminal event stream error: {e}"));
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn run(db_path: PathBuf, root_dir: PathBuf) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal, db_path, root_dir).await;
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;
    result
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use std::sync::Arc;

    use super::*;
    use crate::models::meeting::Meeting;
    use crate::models::reflection::Reflection;
    use crate::models::todo_item::TodoItem;
    use chrono::Utc;
    use std::sync::OnceLock;
    use uuid::Uuid;

    static TZ_INIT: OnceLock<()> = OnceLock::new();

    fn ensure_utc_tz() {
        TZ_INIT.get_or_init(|| {
            // SAFETY: tests are single-threaded for insta snapshots;
            // setting TZ once before any rendering occurs is safe here.
            unsafe { std::env::set_var("TZ", "UTC") };
        });
    }

    async fn test_app() -> App {
        ensure_utc_tz();
        let db_dir = tempfile::tempdir().expect("create tempdir failed").keep();
        let db_path = db_dir.join("test.db");
        let _db = crate::db::Database::open_path(&db_path)
            .await
            .expect("db open failed");
        let root_dir = tempfile::tempdir().expect("create tempdir failed").keep();
        App::new(db_path, root_dir)
    }

    fn fixed_item(label: &str) -> TodoItem {
        let fixed_time = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        TodoItem {
            id: Uuid::now_v7(),
            label: label.to_string(),
            status: crate::models::todo_item::TodoStatus::New,
            created_at: fixed_time,
            updated_at: fixed_time,
        }
    }

    fn fixed_meeting(name: &str) -> Meeting {
        let fixed_time = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        Meeting {
            id: Uuid::now_v7(),
            name: name.to_string(),
            scheduled_at: fixed_time,
            created_at: fixed_time,
            updated_at: fixed_time,
        }
    }

    fn test_editor_fn() -> EditorFn {
        Arc::new(|path: &std::path::Path| {
            std::fs::write(path, "test reflection content")?;
            Ok(Vec::new())
        })
    }

    fn noop_editor_fn() -> EditorFn {
        Arc::new(|_path: &std::path::Path| Ok(Vec::new()))
    }

    #[tokio::test]
    async fn empty_app_render() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("new default app open", terminal.backend());
    }

    #[tokio::test]
    async fn populated_list_render() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("buy groceries"), fixed_item("write tests")]);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("populated todo list", terminal.backend());
    }

    #[tokio::test]
    async fn modal_open_render() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("buy groceries")]);
        app.todo_list_view.open_modal();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("modal open", terminal.backend());
    }

    #[tokio::test]
    async fn modal_overflow_text_wraps_and_grows() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("buy groceries")]);
        app.todo_list_view.open_modal();

        for c in "this is a very long todo item description that keeps going well past the \
                  inner width of the modal so the text must wrap onto multiple lines"
            .chars()
        {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .await
                .expect("handle_key failed");
        }

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("modal open overflow", terminal.backend());
    }

    #[tokio::test]
    async fn selected_item_render() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("buy groceries"), fixed_item("write tests")]);
        app.todo_list_view.set_selected_index(0);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("selected item render", terminal.backend());
    }

    #[tokio::test]
    async fn status_modal_open_render() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("buy groceries"), fixed_item("write tests")]);
        app.todo_list_view.open_status_modal();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("status modal open", terminal.backend());
    }

    #[tokio::test]
    async fn j_selects_first_item() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("task 1"), fixed_item("task 2")]);

        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(key_j).await.expect("handle_key failed");

        assert_eq!(app.todo_list_view.selected_index(), Some(0));
    }

    #[tokio::test]
    async fn j_then_j_selects_second_item() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("task 1"), fixed_item("task 2")]);

        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(key_j).await.expect("handle_key failed");
        app.handle_key(key_j).await.expect("handle_key failed");

        assert_eq!(app.todo_list_view.selected_index(), Some(1));
    }

    #[tokio::test]
    async fn k_at_first_stays_at_first() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("task 1"), fixed_item("task 2")]);

        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let key_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        app.handle_key(key_j).await.expect("handle_key failed");
        app.handle_key(key_k).await.expect("handle_key failed");

        assert_eq!(app.todo_list_view.selected_index(), Some(0));
    }

    #[tokio::test]
    async fn u_opens_status_modal() {
        let mut app = test_app().await;
        app.todo_list_view.set_items(vec![fixed_item("task 1")]);

        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let key_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE);
        app.handle_key(key_j).await.expect("handle_key failed");
        app.handle_key(key_u).await.expect("handle_key failed");

        assert!(app.todo_list_view.is_status_modal_active());
    }

    #[tokio::test]
    async fn a_toggles_show_all() {
        let mut app = test_app().await;
        app.todo_list_view.set_items(vec![fixed_item("task 1")]);

        let key_a = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE);
        app.handle_key(key_a).await.expect("handle_key failed");

        assert!(app.todo_list_view.is_show_all());
    }

    #[tokio::test]
    async fn j_on_empty_list_does_nothing() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(key_j).await.expect("handle_key failed");

        assert_eq!(app.todo_list_view.selected_index(), None);
    }

    #[tokio::test]
    async fn k_on_empty_list_does_nothing() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        let key_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        app.handle_key(key_k).await.expect("handle_key failed");

        assert_eq!(app.todo_list_view.selected_index(), None);
    }

    #[tokio::test]
    async fn u_on_empty_list_does_nothing() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        let key_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE);
        app.handle_key(key_u).await.expect("handle_key failed");

        assert!(!app.todo_list_view.is_status_modal_active());
    }

    #[tokio::test]
    async fn selection_clamped_after_item_disappears() {
        let mut app = test_app().await;
        let item1 = fixed_item("task 1");
        let item2 = fixed_item("task 2");
        app.todo_list_view.set_items(vec![item1, item2]);

        app.todo_list_view.set_selected_index(1);
        assert_eq!(app.todo_list_view.selected_index(), Some(1));

        app.todo_list_view.set_items(vec![fixed_item("task 1")]);
        app.todo_list_view.clamp_selected_index_for_test();

        assert_eq!(app.todo_list_view.selected_index(), Some(0));
    }

    #[tokio::test]
    async fn reflections_view_render() {
        let mut app = test_app().await;
        app.active_tab = Tab::Reflections;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("reflections view", terminal.backend());
    }

    #[tokio::test]
    async fn populated_reflections_render() {
        let mut app = test_app().await;
        let fixed_time = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        let reflection = Reflection {
            id: Uuid::now_v7(),
            about_id: None,
            file_path: "2024/01/15/test.md".to_string(),
            created_at: fixed_time,
            updated_at: fixed_time,
        };
        app.reflections_view.set_items_with_labels(
            vec![reflection],
            vec!["General Reflection 2024-01-15 10:30".to_string()],
        );
        app.active_tab = Tab::Reflections;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("populated reflections", terminal.backend());
    }

    #[tokio::test]
    async fn meetings_view_render() {
        let mut app = test_app().await;
        app.active_tab = Tab::Meetings;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("meetings view", terminal.backend());
    }

    #[tokio::test]
    async fn populated_meetings_render() {
        let mut app = test_app().await;
        app.active_tab = Tab::Meetings;
        app.meetings_view
            .set_items(vec![fixed_meeting("Standup"), fixed_meeting("Retro")]);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("populated meetings", terminal.backend());
    }

    #[tokio::test]
    async fn meeting_selection_clamped_after_item_disappears() {
        let mut app = test_app().await;
        let meeting1 = fixed_meeting("Standup");
        let meeting2 = fixed_meeting("Retro");
        app.meetings_view.set_items(vec![meeting1, meeting2]);

        app.meetings_view.set_selected_index(1);
        assert_eq!(app.meetings_view.selected_index(), Some(1));

        app.meetings_view.set_items(vec![fixed_meeting("Standup")]);
        app.meetings_view.clamp_selected_index_for_test();

        assert_eq!(app.meetings_view.selected_index(), Some(0));
    }

    #[tokio::test]
    async fn meeting_modal_open_render() {
        let mut app = test_app().await;
        app.active_tab = Tab::Meetings;
        app.meetings_view.set_items(vec![fixed_meeting("Standup")]);
        app.meetings_view.open_modal();
        app.meetings_view.set_datetime_for_test(2024, 1, 15, 10, 30);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("meeting modal open", terminal.backend());
    }

    #[tokio::test]
    async fn meeting_modal_long_name_wraps_and_grows() {
        let mut app = test_app().await;
        app.active_tab = Tab::Meetings;
        app.meetings_view.set_items(vec![fixed_meeting("Standup")]);
        app.meetings_view.open_modal();
        app.meetings_view.set_datetime_for_test(2024, 1, 15, 10, 30);

        for c in "an extraordinarily verbose meeting title that overflows the modal \
                  inner width and therefore wraps"
            .chars()
        {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .await
                .expect("handle_key failed");
        }

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("meeting modal long name", terminal.backend());
    }

    #[tokio::test]
    async fn a_opens_meeting_modal() {
        let mut app = test_app().await;
        app.active_tab = Tab::Meetings;
        app.meetings_view.set_items(vec![fixed_meeting("Standup")]);

        let key_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(key_a).await.expect("handle_key failed");

        assert!(app.meetings_view.is_modal_active());
    }

    #[tokio::test]
    async fn meeting_modal_blocks_tab_switch() {
        let mut app = test_app().await;
        app.active_tab = Tab::Meetings;
        app.meetings_view.set_items(vec![fixed_meeting("Standup")]);
        app.meetings_view.open_modal();

        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");

        assert!(matches!(app.active_tab, Tab::Meetings));
        assert!(app.meetings_view.is_modal_active());
    }

    #[tokio::test]
    async fn meeting_list_state_persists_across_tab_switch() {
        let mut app = test_app().await;
        app.active_tab = Tab::Meetings;
        app.meetings_view
            .set_items(vec![fixed_meeting("Standup"), fixed_meeting("Retro")]);
        app.meetings_view.set_selected_index(1);

        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Reflections));

        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Summaries));

        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::TodoList));

        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Meetings));
        assert_eq!(app.meetings_view.selected_index(), Some(1));
    }

    #[tokio::test]
    async fn tab_bar_render() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("tab bar render", terminal.backend());
    }

    #[tokio::test]
    async fn gt_switches_right() {
        let mut app = test_app().await;
        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Meetings));
    }

    #[tokio::test]
    async fn gt_capital_switches_left() {
        let mut app = test_app().await;
        app.active_tab = Tab::Reflections;
        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_big_t = KeyEvent::new(KeyCode::Char('T'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_big_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Meetings));
    }

    #[tokio::test]
    async fn gt_wraps_from_summaries_to_todo() {
        let mut app = test_app().await;
        app.active_tab = Tab::Summaries;
        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::TodoList));
    }

    #[tokio::test]
    async fn g_prefix_resets_on_unrelated_key() {
        let mut app = test_app().await;
        app.todo_list_view.set_items(vec![fixed_item("task 1")]);
        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_j).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::TodoList));
        assert!(!app.pending_g_prefix);
        assert_eq!(app.todo_list_view.selected_index(), Some(0));
    }

    #[tokio::test]
    async fn modal_blocks_tab_switch() {
        let mut app = test_app().await;
        app.todo_list_view.set_items(vec![fixed_item("task 1")]);
        app.todo_list_view.open_modal();
        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::TodoList));
        assert!(app.todo_list_view.is_modal_active());
    }

    #[tokio::test]
    async fn todo_list_state_persists_across_tab_switch() {
        let mut app = test_app().await;
        app.todo_list_view
            .set_items(vec![fixed_item("task 1"), fixed_item("task 2")]);
        app.todo_list_view.set_selected_index(1);

        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Meetings));

        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Reflections));

        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::Summaries));

        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");
        assert!(matches!(app.active_tab, Tab::TodoList));
        assert_eq!(app.todo_list_view.selected_index(), Some(1));
    }

    #[tokio::test]
    async fn r_key_on_todo_list_creates_reflection() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.todo_list_view.set_items(vec![fixed_item("test item")]);
        app.todo_list_view.set_selected_index(0);
        app.set_todo_editor_fn(test_editor_fn());

        let key_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        app.handle_key(key_r).await.expect("handle_key failed");

        let reflections = app
            .reflection_service
            .list_reflections()
            .await
            .expect("list failed");
        assert_eq!(reflections.len(), 1);
        assert!(reflections[0].about_id.is_some());
    }

    #[tokio::test]
    async fn capital_r_creates_general_reflection() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.set_app_editor_fn(test_editor_fn());

        let key_r = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE);
        app.handle_key(key_r).await.expect("handle_key failed");

        let reflections = app
            .reflection_service
            .list_reflections()
            .await
            .expect("list failed");
        assert_eq!(reflections.len(), 1);
        assert!(reflections[0].about_id.is_none());
    }

    #[tokio::test]
    async fn r_key_on_meetings_creates_reflection() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        app.active_tab = Tab::Meetings;
        app.meetings_view.set_items(vec![fixed_meeting("Standup")]);
        app.meetings_view.set_selected_index(0);
        app.set_meeting_editor_fn(test_editor_fn());

        let key_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        app.handle_key(key_r).await.expect("handle_key failed");

        let reflections = app
            .reflection_service
            .list_reflections()
            .await
            .expect("list failed");
        assert_eq!(reflections.len(), 1);
        assert!(reflections[0].about_id.is_some());
    }

    #[tokio::test]
    async fn r_key_with_no_selection_does_nothing() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        app.todo_list_view.set_items(vec![fixed_item("task 1")]);

        let key_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        app.handle_key(key_r).await.expect("handle_key failed");

        let reflections = app
            .reflection_service
            .list_reflections()
            .await
            .expect("list failed");
        assert_eq!(reflections.len(), 0);
    }

    #[tokio::test]
    async fn empty_editor_exit_cleans_up_reflection() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.set_app_editor_fn(noop_editor_fn());

        let key_r = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE);
        app.handle_key(key_r).await.expect("handle_key failed");

        let reflections = app
            .reflection_service
            .list_reflections()
            .await
            .expect("list failed");
        assert_eq!(reflections.len(), 0);
    }

    #[tokio::test]
    async fn empty_editor_exit_on_todo_cleans_up_reflection() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.todo_list_view.set_items(vec![fixed_item("test item")]);
        app.todo_list_view.set_selected_index(0);
        app.set_todo_editor_fn(noop_editor_fn());

        let key_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        app.handle_key(key_r).await.expect("handle_key failed");

        let reflections = app
            .reflection_service
            .list_reflections()
            .await
            .expect("list failed");
        assert_eq!(reflections.len(), 0);
    }

    #[tokio::test]
    async fn n_key_on_todo_list_creates_note() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        let item = fixed_item("test item");
        app.todo_list_view.set_items(vec![item.clone()]);
        app.todo_list_view.set_selected_index(0);
        app.set_todo_editor_fn(test_editor_fn());

        let key_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        app.handle_key(key_n).await.expect("handle_key failed");

        let notes = app.note_service.list_notes_for_test().await;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].related_to_id, Some(item.id));
    }

    #[tokio::test]
    async fn n_key_on_meetings_creates_note() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        app.active_tab = Tab::Meetings;
        let meeting = fixed_meeting("Standup");
        app.meetings_view.set_items(vec![meeting.clone()]);
        app.meetings_view.set_selected_index(0);
        app.set_meeting_editor_fn(test_editor_fn());

        let key_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        app.handle_key(key_n).await.expect("handle_key failed");

        let notes = app.note_service.list_notes_for_test().await;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].related_to_id, Some(meeting.id));
    }

    #[tokio::test]
    async fn capital_n_creates_general_note() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.set_app_editor_fn(test_editor_fn());

        let key_n = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE);
        app.handle_key(key_n).await.expect("handle_key failed");

        let note_count = app.note_service.note_count().await;
        assert_eq!(note_count, 1);
    }

    #[tokio::test]
    async fn n_key_with_no_selection_does_nothing() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        app.todo_list_view.set_items(vec![fixed_item("task 1")]);

        let key_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        app.handle_key(key_n).await.expect("handle_key failed");

        let note_count = app.note_service.note_count().await;
        assert_eq!(note_count, 0);
    }

    #[tokio::test]
    async fn empty_editor_exit_cleans_up_note() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.set_app_editor_fn(noop_editor_fn());

        let key_n = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE);
        app.handle_key(key_n).await.expect("handle_key failed");

        let note_count = app.note_service.note_count().await;
        assert_eq!(note_count, 0);
    }

    #[tokio::test]
    async fn empty_editor_exit_on_todo_cleans_up_note() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.todo_list_view.set_items(vec![fixed_item("test item")]);
        app.todo_list_view.set_selected_index(0);
        app.set_todo_editor_fn(noop_editor_fn());

        let key_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        app.handle_key(key_n).await.expect("handle_key failed");

        let note_count = app.note_service.note_count().await;
        assert_eq!(note_count, 0);
    }

    #[tokio::test]
    async fn enter_opens_timeline_from_todo_list() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.todo_list_view
            .create_todo_for_test("timeline test todo")
            .await
            .expect("create todo failed");
        app.todo_list_view.set_selected_index(0);

        let key_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(key_enter).await.expect("handle_key failed");

        assert!(
            app.todo_list_view.is_timeline_active(),
            "timeline should be active after Enter"
        );
    }

    #[tokio::test]
    async fn enter_opens_timeline_from_meetings_view() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        app.active_tab = Tab::Meetings;

        app.meetings_view
            .create_meeting_for_test("timeline test meeting", Utc::now())
            .await
            .expect("create meeting failed");
        app.meetings_view.set_selected_index(0);

        let key_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(key_enter).await.expect("handle_key failed");

        assert!(
            app.meetings_view.is_timeline_active(),
            "timeline should be active after Enter"
        );
    }

    #[tokio::test]
    async fn esc_closes_timeline_returns_to_todo_list() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.todo_list_view.set_items(vec![fixed_item("test item")]);
        app.todo_list_view.set_selected_index(0);
        app.todo_list_view.set_timeline_view(
            super::super::timeline_view::TimelineView::new_for_test(vec![], "Test"),
        );

        assert!(app.todo_list_view.is_timeline_active());

        let key_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key(key_esc).await.expect("handle_key failed");

        assert!(
            !app.todo_list_view.is_timeline_active(),
            "timeline should be closed after Esc"
        );
    }

    #[tokio::test]
    async fn q_closes_timeline_returns_to_meetings_view() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");
        app.active_tab = Tab::Meetings;

        app.meetings_view.set_items(vec![fixed_meeting("Standup")]);
        app.meetings_view.set_selected_index(0);
        app.meetings_view.set_timeline_view(
            super::super::timeline_view::TimelineView::new_for_test(vec![], "Test"),
        );

        assert!(app.meetings_view.is_timeline_active());

        let key_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        app.handle_key(key_q).await.expect("handle_key failed");

        assert!(
            !app.meetings_view.is_timeline_active(),
            "timeline should be closed after q"
        );
    }

    #[tokio::test]
    async fn timeline_blocks_tab_switch() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.todo_list_view.set_items(vec![fixed_item("test item")]);
        app.todo_list_view.set_selected_index(0);
        app.todo_list_view.set_timeline_view(
            super::super::timeline_view::TimelineView::new_for_test(vec![], "Test"),
        );

        let key_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let key_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(key_g).await.expect("handle_key failed");
        app.handle_key(key_t).await.expect("handle_key failed");

        assert!(
            matches!(app.active_tab, Tab::TodoList),
            "tab should not switch while timeline is active"
        );
        assert!(app.todo_list_view.is_timeline_active());
    }

    #[tokio::test]
    async fn timeline_render_in_app() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        let fixed_time = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .expect("parse failed")
            .with_timezone(&Utc);
        let todo_id = Uuid::now_v7();
        let ts = fixed_time.to_rfc3339();
        app.todo_list_view
            .create_todo_with_fixed_time_for_test("snapshot todo", todo_id, &ts)
            .await
            .expect("create todo failed");
        app.todo_list_view.set_selected_index(0);

        let key_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(key_enter).await.expect("handle_key failed");

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("timeline in app", terminal.backend());
    }

    #[tokio::test]
    async fn timeline_empty_render_in_app() {
        let mut app = test_app().await;
        app.init().await.expect("init failed");

        app.todo_list_view.set_items(vec![fixed_item("test item")]);
        app.todo_list_view.set_selected_index(0);
        app.todo_list_view.set_timeline_view(
            super::super::timeline_view::TimelineView::new_for_test(vec![], "Timeline: test item"),
        );

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation");
        terminal
            .draw(|frame| render_app(&mut app, frame))
            .expect("failed to draw");
        insta::assert_snapshot!("timeline empty in app", terminal.backend());
    }
}
