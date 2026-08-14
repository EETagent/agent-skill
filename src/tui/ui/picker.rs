use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{
    model::Scope,
    source::redact_source,
    terminal::sanitize_inline,
    tui::app::{App, InstallPickerState, InstallPreviewCache, InstallPreviewKey},
};

use super::{
    colored_panel, panel, preview::build_install_preview, render_popup_panel, SELECTED_BACKGROUND,
};

pub(super) fn render_install_picker(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner = render_popup_panel(
        frame,
        area,
        94,
        90,
        colored_panel(" Install skills ", Color::Cyan),
    );
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(inner);

    let rows_available = sections[1].height.saturating_sub(2) as usize;
    app.ensure_install_visible(rows_available);
    let Some(picker) = app.install_picker.as_mut() else {
        return;
    };

    frame.render_widget(
        Paragraph::new(format!(
            "Source: {}  ·  {} skill(s) discovered",
            sanitize_inline(&redact_source(&picker.prepared.resolved)),
            picker.len()
        ))
        .style(Style::default().fg(Color::Gray)),
        sections[0],
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(sections[1]);
    let cursor = picker.cursor_index();
    let items = picker
        .prepared
        .skills
        .iter()
        .enumerate()
        .skip(picker.offset())
        .take(rows_available)
        .map(|(index, skill)| {
            let selected = picker.is_selected(index);
            let checked = if selected { "[x]" } else { "[ ]" };
            let style = if index == cursor {
                selected_item_style()
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    checked,
                    Style::default().fg(if selected {
                        Color::Green
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::raw(format!(" {}", sanitize_inline(&skill.title))),
            ]))
            .style(style)
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(items).block(panel(" Select ")), body[0]);

    render_install_preview(frame, picker, body[1]);

    let scope = match picker.scope {
        Scope::Local => "local .agents/skills",
        Scope::Global => "global ~/.agents/skills",
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{} selected  ·  target: {}  ·  replace: {}\n\
             Space select  a all/none  Tab target  r replace  Enter install  Esc cancel",
            picker.selected_count(),
            scope,
            if picker.replace {
                "yes (archive old)"
            } else {
                "no"
            }
        ))
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center),
        sections[2],
    );
}

fn render_install_preview(frame: &mut Frame, picker: &mut InstallPickerState, area: Rect) {
    let preview_width = area.width.saturating_sub(2).max(1);
    let cursor = picker.cursor_index();
    let cache_key = InstallPreviewKey::new(cursor, preview_width);
    if picker
        .preview_cache
        .as_ref()
        .is_none_or(|cache| !cache.is_current(&cache_key))
    {
        let lines = picker
            .prepared
            .skills
            .get(cursor)
            .map(|skill| build_install_preview(skill, usize::from(preview_width)))
            .unwrap_or_else(|| vec![Line::from("No skill selected.")]);
        picker.preview_cache = Some(InstallPreviewCache::new(cache_key, lines));
    }

    let preview = picker
        .preview_cache
        .as_ref()
        .map(|cache| cache.lines().to_vec())
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(Text::from(preview)).block(panel(" Source preview ")),
        area,
    );
}

pub(super) fn render_remote_results(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(" Remote results · {:?} ", app.remote_query);
    let inner = render_popup_panel(frame, area, 90, 86, colored_panel(title, Color::Cyan));
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(inner);
    let rows_available = sections[0].height.saturating_sub(2) as usize;
    app.ensure_remote_visible(rows_available);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(sections[0]);
    let cursor = app.remote_position();
    let items = app
        .remote_results
        .iter()
        .enumerate()
        .skip(app.remote_offset())
        .take(rows_available)
        .map(|(index, skill)| {
            ListItem::new(format!(
                "{}  ·  {} installs",
                sanitize_inline(&skill.name),
                skill.installs
            ))
            .style(if index == cursor {
                selected_item_style()
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(panel(format!(" {} result(s) ", app.remote_results.len()))),
        body[0],
    );

    let detail = app
        .selected_remote()
        .map(|skill| {
            format!(
                "Name: {}\nSource: {}\nSlug: {}\nInstalls: {}\n\n\
                 Install source:\n{}\n\n\
                 Press Enter or i to inspect and install this result.",
                sanitize_inline(&skill.name),
                sanitize_inline(&redact_source(&skill.source)),
                sanitize_inline(&skill.slug),
                skill.installs,
                sanitize_inline(&redact_source(&skill.install_source()))
            )
        })
        .unwrap_or_else(|| "No remote results.".to_owned());
    frame.render_widget(
        Paragraph::new(detail)
            .block(panel(" Details "))
            .wrap(Wrap { trim: false }),
        body[1],
    );
    frame.render_widget(
        Paragraph::new("j/k move  Enter/i inspect & install  / new query  Esc close")
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center),
        sections[1],
    );
}

fn selected_item_style() -> Style {
    Style::default()
        .bg(SELECTED_BACKGROUND)
        .add_modifier(Modifier::BOLD)
}
