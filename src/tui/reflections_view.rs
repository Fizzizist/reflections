use super::splash;
use hjkl_splash::start_screen::StartScreen;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders},
};

pub struct ReflectionsView;

impl ReflectionsView {
    pub fn new() -> Self {
        ReflectionsView
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("Reflections").borders(Borders::ALL);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let screen = StartScreen::build(env!("CARGO_PKG_VERSION"));
        splash::render(frame, inner, &screen);
    }
}
