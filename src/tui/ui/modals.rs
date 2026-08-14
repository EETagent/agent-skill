use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};

use crate::terminal::sanitize_inline;
use crate::tui::app::{App, InputKind};

use super::{colored_panel, popup_rect, render_popup_panel};

pub(super) fn render_input(frame: &mut Frame, app: &App, kind: InputKind, area: Rect) {
    let (title, hint) = match kind {
        InputKind::Search => (
            " Search installed skills ",
            "Results update as you type · Enter accept · Esc restore previous query",
        ),
        InputKind::Install => (
            " Install from source ",
            "Local path, Git URL, GitHub/GitLab URL, or owner/repository · Enter inspect",
        ),
        InputKind::Remote => (
            " Find remote skills ",
            "Search the skills.sh catalog · Enter search · Esc cancel",
        ),
    };
    let inner = render_popup_panel(frame, area, 78, 28, colored_panel(title, Color::Cyan));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::raw(sanitize_inline(&app.input)),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ]))
        .wrap(Wrap { trim: false }),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center),
        rows[1],
    );
}

pub(super) fn render_confirmation(frame: &mut Frame, app: &App, area: Rect) {
    let popup = popup_rect(68, 30, area);
    let prompt = app
        .pending
        .as_ref()
        .map(|pending| pending.prompt())
        .unwrap_or_else(|| "No pending operation.".to_owned());
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(format!(
            "{prompt}\n\nPress y or Enter to continue; n or Esc to cancel."
        ))
        .block(colored_panel(" Confirm ", Color::Yellow))
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Center),
        popup,
    );
}

pub(super) fn render_help(frame: &mut Frame, area: Rect) {
    let popup = popup_rect(84, 82, area);
    frame.render_widget(Clear, popup);
    let lines = vec![
        help_line(
            "j / k, arrows",
            "move through skills; when Preview is focused, scroll",
        ),
        help_line("Tab", "switch focus between the skill list and preview"),
        help_line(
            "Space / Enter",
            "toggle the selected skill enabled/disabled",
        ),
        help_line("e / d", "enable or disable the selected skill"),
        help_line("E / D", "enable or disable every visible matching skill"),
        help_line(
            "x / X",
            "remove selected / all visible skills to reversible trash",
        ),
        help_line(
            "/",
            "full-text search titles, names, descriptions, paths, and agents",
        ),
        help_line("c", "clear the active search"),
        help_line("g / s", "cycle scope and state filters"),
        help_line("PageUp / PageDown", "scroll the SKILL.md preview"),
        help_line("i", "inspect and install skills from a local or Git source"),
        help_line(
            "f",
            "find skills in the remote catalog and install a result",
        ),
        help_line("r", "rescan all registered local/global paths"),
        help_line("? / Esc", "close this help"),
        help_line("q / Ctrl+C", "quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Disable never deletes: the complete directory moves to a sibling \
             .*-disabled store. Normal remove moves to .*-trash.",
            Style::default().fg(Color::Green),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(colored_panel(" Help ", Color::Cyan))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn help_line<'a>(key: &'a str, description: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{key:<22}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(description),
    ])
}
