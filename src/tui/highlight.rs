use std::sync::LazyLock;

use ratatui::text::{Line, Span};
use syntect::highlighting::{FontStyle, Theme as SyntectTheme};
use syntect::parsing::SyntaxSet;

use the_other_tui_markdown::RendererBuilder;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static MONOKAI_EXTENDED: LazyLock<SyntectTheme> = LazyLock::new(|| {
    let assets = syntect_assets::assets::HighlightingAssets::from_binary();
    assets.get_theme("Monokai Extended Origin").clone()
});

fn syntect_style_to_ratatui(style: &syntect::highlighting::Style) -> ratatui::style::Style {
    let fg = match style.foreground {
        syntect::highlighting::Color { r, g, b, a: 255 } => {
            Some(ratatui::style::Color::Rgb(r, g, b))
        }
        _ => None,
    };

    let mut modifiers = ratatui::style::Modifier::empty();
    if style.font_style.contains(FontStyle::BOLD) {
        modifiers |= ratatui::style::Modifier::BOLD;
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        modifiers |= ratatui::style::Modifier::ITALIC;
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        modifiers |= ratatui::style::Modifier::UNDERLINED;
    }

    ratatui::style::Style::new()
        .fg(fg.unwrap_or(ratatui::style::Color::Gray))
        .add_modifier(modifiers)
}

fn highlight_code_block(lang: &str, content: &str) -> Vec<Line<'static>> {
    let syntax = if lang.is_empty() {
        SYNTAX_SET.find_syntax_plain_text()
    } else {
        SYNTAX_SET
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text())
    };

    let mut highlighter = syntect::easy::HighlightLines::new(syntax, &MONOKAI_EXTENDED);
    let mut lines: Vec<Line<'static>> = Vec::new();

    for line in syntect::util::LinesWithEndings::from(content) {
        let ranges = highlighter
            .highlight_line(line, &SYNTAX_SET)
            .unwrap_or_else(|_| vec![(syntect::highlighting::Style::default(), line)]);

        let spans: Vec<Span<'static>> = ranges
            .into_iter()
            .map(|(style, text)| {
                let ratatui_style = syntect_style_to_ratatui(&style);
                Span::styled(text.to_string(), ratatui_style)
            })
            .collect();

        lines.push(Line::from(spans));
    }

    lines
}

pub fn build_renderer(theme: the_other_tui_markdown::Theme) -> the_other_tui_markdown::Renderer {
    RendererBuilder::new()
        .with_theme(theme)
        .with_code_block(highlight_code_block)
        .build()
}
