use std::path::Path;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Cell, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::{
    model::SkillState,
    terminal::sanitize_inline,
    tui::app::{App, Focus, PreviewCache, SkillPreviewKey, StatusKind},
};

use super::{
    colored_panel, panel, preview::build_skill_preview, rounded_block, ACTIVE_BORDER,
    INACTIVE_BORDER, MINIMUM_HEIGHT, MINIMUM_WIDTH, SELECTED_BACKGROUND,
};

pub(super) fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let counts = app.catalog.counts();
    let query = if app.query.is_empty() {
        "none".to_owned()
    } else {
        format!("{:?}", app.query)
    };
    let line = Line::from(vec![
        Span::styled(
            " skills ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "{} visible / {} total  ·  {} on  ·  {} off  ·  scope {}  ·  state {}  ·  search {}",
            app.visible.len(),
            app.catalog.skills.len(),
            counts.enabled,
            counts.disabled,
            app.scope_view.label(),
            app.state_view.label(),
            query
        )),
    ]);
    frame.render_widget(Paragraph::new(line).block(panel(" skillctl ")), area);
}

pub(super) fn render_main(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.width >= 96 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(43), Constraint::Percentage(57)])
            .split(area);
        render_skill_list(frame, app, columns[0]);
        render_preview(frame, app, columns[1]);
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
            .split(area);
        render_skill_list(frame, app, rows[0]);
        render_preview(frame, app, rows[1]);
    }
}

fn render_skill_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let border_color = focus_border(app.focus, Focus::Skills);
    let block = colored_panel(" Skills ", border_color);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.visible.is_empty() {
        frame.render_widget(
            Paragraph::new(
                "No skills match the current filters.\n\nPress c to clear search, g to change \
                 scope, or s to change state.",
            )
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    let visible_rows = inner.height.saturating_sub(1) as usize;
    app.ensure_skill_visible(visible_rows);
    let selected_position = app.selected_position();
    let rows = app
        .visible
        .iter()
        .enumerate()
        .skip(app.list_offset())
        .take(visible_rows)
        .filter_map(|(position, index)| {
            let skill = app.catalog.skills.get(*index)?;
            let state_style = match skill.state {
                SkillState::Enabled => Style::default().fg(Color::Green),
                SkillState::Disabled => Style::default().fg(Color::Yellow),
            };
            let row_style = if position == selected_position {
                selected_row_style()
            } else {
                Style::default()
            };
            let root = sanitize_inline(&root_label(&skill.agents, &skill.root_path));

            Some(
                Row::new(vec![
                    Cell::from(match skill.state {
                        SkillState::Enabled => "on",
                        SkillState::Disabled => "off",
                    })
                    .style(state_style),
                    Cell::from(skill.scope.short_label()),
                    Cell::from(sanitize_inline(&skill.title)),
                    Cell::from(root),
                ])
                .style(row_style),
            )
        })
        .collect::<Vec<_>>();

    let header = Row::new(vec!["ST", "S", "TITLE", "ROOT"]).style(
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    );
    let widths = [
        Constraint::Length(4),
        Constraint::Length(3),
        Constraint::Percentage(58),
        Constraint::Percentage(42),
    ];
    frame.render_widget(
        Table::new(rows, widths).header(header).column_spacing(1),
        inner,
    );
}

fn root_label(agents: &[String], root_path: &Path) -> String {
    match agents {
        [] => root_path.display().to_string(),
        [agent] => agent.clone(),
        agents => format!("Shared ({})", agents.len()),
    }
}

fn render_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    let border_color = focus_border(app.focus, Focus::Preview);
    let Some(selected_index) = app.visible.get(app.selected_position()).copied() else {
        frame.render_widget(
            Paragraph::new("Select a skill to preview its SKILL.md.")
                .block(colored_panel(" Preview ", border_color)),
            area,
        );
        return;
    };
    let Some(skill) = app.catalog.skills.get(selected_index) else {
        return;
    };

    let viewport_width = area.width.saturating_sub(2).max(1);
    let viewport_height = usize::from(area.height.saturating_sub(2));
    let cache_key = SkillPreviewKey::new(skill, viewport_width);
    let title = sanitize_inline(&skill.title);

    if app
        .preview_cache
        .as_ref()
        .is_none_or(|cache| !cache.is_current(&cache_key))
    {
        let lines = build_skill_preview(
            skill,
            app.manager.registry().environment(),
            usize::from(viewport_width),
        );
        app.preview_cache = Some(PreviewCache::new(cache_key, lines));
    }

    let line_count = app
        .preview_cache
        .as_ref()
        .map(|cache| cache.lines().len())
        .unwrap_or_default();
    let (scroll, max_scroll) =
        clamped_preview_scroll(app.preview_scroll, line_count, viewport_height);
    app.preview_scroll = scroll;

    let visible_lines = app
        .preview_cache
        .as_ref()
        .map(|cache| {
            cache
                .lines()
                .iter()
                .skip(usize::from(app.preview_scroll))
                .take(viewport_height)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let title = format!(
        " Preview · {title} · {}/{} ",
        app.preview_scroll, max_scroll
    );
    frame.render_widget(
        Paragraph::new(Text::from(visible_lines)).block(colored_panel(title, border_color)),
        area,
    );
}

fn clamped_preview_scroll(scroll: u16, line_count: usize, viewport_height: usize) -> (u16, u16) {
    let maximum = line_count
        .saturating_sub(viewport_height)
        .min(usize::from(u16::MAX)) as u16;
    (scroll.min(maximum), maximum)
}

pub(super) fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let status_style = match app.status.kind {
        StatusKind::Info => Style::default().fg(Color::Gray),
        StatusKind::Success => Style::default().fg(Color::Green),
        StatusKind::Error => Style::default().fg(Color::Red),
    };
    let line = Line::from(vec![
        Span::styled(app.status.message.as_str(), status_style),
        Span::styled(
            "  │  / search  space toggle  E/D all  i install  f find  g/s filters  ? help  q quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).block(rounded_block()), area);
}

pub(super) fn render_too_small(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(format!(
            "Terminal is too small ({}×{}).\nResize to at least {MINIMUM_WIDTH}×{MINIMUM_HEIGHT}.",
            area.width, area.height
        ))
        .block(panel(" skillctl "))
        .alignment(Alignment::Center),
        area,
    );
}

fn focus_border(current: Focus, panel_focus: Focus) -> Color {
    if current == panel_focus {
        ACTIVE_BORDER
    } else {
        INACTIVE_BORDER
    }
}

fn selected_row_style() -> Style {
    Style::default()
        .bg(SELECTED_BACKGROUND)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{clamped_preview_scroll, root_label};

    #[test]
    fn labels_multi_agent_roots_as_shared() {
        let agents = vec!["Cline".to_owned(), "Dexto".to_owned()];

        assert_eq!(
            root_label(&agents, Path::new("/shared/skills")),
            "Shared (2)"
        );
    }

    #[test]
    fn preserves_single_agent_and_custom_root_labels() {
        assert_eq!(
            root_label(&["Codex".to_owned()], Path::new("/codex/skills")),
            "Codex"
        );
        assert_eq!(
            root_label(&[], Path::new("/custom/skills")),
            "/custom/skills"
        );
    }

    #[test]
    fn clamps_preview_scroll_to_the_current_viewport() {
        assert_eq!(clamped_preview_scroll(u16::MAX, 100, 10), (90, 90));
        assert_eq!(clamped_preview_scroll(5, 4, 10), (0, 0));
        assert_eq!(
            clamped_preview_scroll(u16::MAX, usize::MAX, 0),
            (u16::MAX, u16::MAX)
        );
    }
}
