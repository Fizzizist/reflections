use anyhow::Result;
use futures::stream::StreamExt;
use ratatui::layout::Layout;
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

use super::splash;
pub struct App {}

impl App {
    pub fn new() -> Self {
        Self {}
    }
}

pub fn render_app(app: &mut App, frame: &mut ratatui::Frame) {
    let screen = hjkl_splash::start_screen::StartScreen::build(env!("CARGO_PKG_VERSION"));
    splash::render(frame, frame.area(), &screen);
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    terminal.draw(|frame| render_app(&mut app, frame))?;
    let mut tick_interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut terminal_events = EventStream::new();
    loop {
        tokio::select! {
            Some(Ok(terminal_event)) = terminal_events.next() => {
                if let Event::Key(key) = terminal_event {
                    if let KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. } = key { break; }
                }
            }
            _ = tick_interval.tick() => {}
        }
    }

    Ok(())
}

pub async fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal).await;
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;
    result
}
