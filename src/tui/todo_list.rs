use super::splash;
use ratatui::{
    Frame,
    widgets::{Block, Borders},
};

pub struct TodoListView;

impl TodoListView {
    pub fn default() -> Self {
        Self {}
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let block = Block::default().borders(Borders::ALL).title("TODO List");
        let inner = block.inner(frame.area());

        frame.render_widget(block, frame.area());

        // safeguard
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let screen = hjkl_splash::start_screen::StartScreen::build(env!("CARGO_PKG_VERSION"));
        splash::render(frame, inner, &screen);
    }
}
