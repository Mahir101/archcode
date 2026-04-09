use termimad::MadSkin;

// Use termimad's re-exported crossterm to avoid version mismatch
use termimad::crossterm::style::Color;

/// Render markdown text to the terminal with syntax highlighting and formatting.
pub fn render_markdown(text: &str) {
    let skin = build_skin();
    // termimad's print_text handles headers, bold, italic, code blocks, lists, etc.
    skin.print_text(text);
}

/// Return the rendered markdown as a string (for testing or further processing).
#[allow(dead_code)]
pub fn render_markdown_to_string(text: &str) -> String {
    let skin = build_skin();
    skin.term_text(text).to_string()
}

fn build_skin() -> MadSkin {
    let mut skin = MadSkin::default();

    // Headers — bold cyan
    skin.headers[0].set_fg(Color::Cyan);
    skin.headers[1].set_fg(Color::Cyan);
    skin.headers[2].set_fg(Color::Cyan);

    // Bold — white bold (default is fine)
    skin.bold.set_fg(Color::White);

    // Italic — yellow
    skin.italic.set_fg(Color::Yellow);

    // Inline code — dark yellow on dark background
    skin.inline_code.set_fg(Color::DarkYellow);

    // Code blocks — keep default (gray background)
    skin.code_block.set_fg(Color::Green);

    // Bullet points
    skin.bullet = termimad::StyledChar::from_fg_char(Color::Cyan, '•');

    skin
}
