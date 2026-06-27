use crate::services::meeting::MeetingService;
use crate::services::todo::TodoService;

use super::meetings_view::MeetingsView;
use super::reflections_view::ReflectionsView;
use super::todo_list::TodoListView;
use anyhow::Result;
use futures::stream::StreamExt;
use std::io;

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
}

const TABS: [Tab; 3] = [Tab::TodoList, Tab::Meetings, Tab::Reflections];

pub struct App {
    todo_list_view: TodoListView,
    meetings_view: MeetingsView,
    reflections_view: ReflectionsView,
    active_tab: Tab,
    pending_g_prefix: bool,
}

impl App {
    pub fn new(todo_service: TodoService, meeting_service: MeetingService) -> Self {
        Self {
            todo_list_view: TodoListView::new(todo_service),
            meetings_view: MeetingsView::new(meeting_service),
            reflections_view: ReflectionsView::new(),
            active_tab: Tab::TodoList,
            pending_g_prefix: false,
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.todo_list_view.init().await?;
        self.meetings_view.init().await?;
        Ok(())
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.pending_g_prefix {
            match key.code {
                KeyCode::Char('t') => {
                    self.active_tab = self.next_tab();
                    self.pending_g_prefix = false;
                    return Ok(());
                }
                KeyCode::Char('T') => {
                    self.active_tab = self.prev_tab();
                    self.pending_g_prefix = false;
                    return Ok(());
                }
                _ => {
                    self.pending_g_prefix = false;
                }
            }
        }

        if key.code == KeyCode::Char('g') && !self.is_modal_active() {
            self.pending_g_prefix = true;
            return Ok(());
        }

        match self.active_tab {
            Tab::TodoList => self.todo_list_view.handle_key(key).await?,
            Tab::Meetings => self.meetings_view.handle_key(key).await?,
            Tab::Reflections => {}
        }
        Ok(())
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
            Tab::Reflections => false,
        }
    }
}

pub fn render_app(app: &mut App, frame: &mut ratatui::Frame) {
    use ratatui::layout::{Constraint, Layout};
    use ratatui::widgets::Tabs;

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(frame.area());

    let tab_titles = vec!["Todo List", "Meetings", "Reflections"];
    let tabs = Tabs::new(tab_titles)
        .select(app.active_tab as usize)
        .highlight_style(ratatui::style::Style::default().reversed());
    frame.render_widget(tabs, chunks[0]);

    let view_area = chunks[1];
    match app.active_tab {
        Tab::TodoList => app.todo_list_view.render(frame, view_area),
        Tab::Meetings => app.meetings_view.render(frame, view_area),
        Tab::Reflections => app.reflections_view.render(frame, view_area),
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
    todo_service: TodoService,
    meeting_service: MeetingService,
) -> Result<()> {
    let mut app = App::new(todo_service, meeting_service);
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
                            app.handle_key(key).await?;
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

pub async fn run(todo_service: TodoService, meeting_service: MeetingService) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal, todo_service, meeting_service).await;
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
    use turso::Builder;

    use super::*;
    use crate::models::meeting::Meeting;
    use crate::models::todo_item::TodoItem;
    use crate::schema;
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
        let db = Builder::new_local(":memory:")
            .experimental_custom_types(true)
            .build()
            .await
            .expect("trouble building the DB");
        let conn = db.connect().expect("trouble connecting to the db");
        schema::init_schema(&conn)
            .await
            .expect("schema init failed");
        let conn2 = db.connect().expect("trouble connecting to the db");
        App::new(TodoService::new(conn), MeetingService::new(conn2))
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
    async fn gt_wraps_from_reflections_to_todo() {
        let mut app = test_app().await;
        app.active_tab = Tab::Reflections;
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
        assert!(matches!(app.active_tab, Tab::TodoList));
        assert_eq!(app.todo_list_view.selected_index(), Some(1));
    }
}
