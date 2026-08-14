mod browse;
mod modals;
mod picker;
mod preview;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear},
    Frame,
};

use super::app::{App, Mode};

const ACTIVE_BORDER: Color = Color::Cyan;
const INACTIVE_BORDER: Color = Color::DarkGray;
const SELECTED_BACKGROUND: Color = Color::DarkGray;
const INSTALL_PREVIEW_BYTES: usize = 32 * 1024;
const MINIMUM_WIDTH: u16 = 52;
const MINIMUM_HEIGHT: u16 = 14;

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT {
        browse::render_too_small(frame, area);
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    browse::render_header(frame, app, sections[0]);
    browse::render_main(frame, app, sections[1]);
    browse::render_footer(frame, app, sections[2]);

    match app.mode {
        Mode::Input(kind) => modals::render_input(frame, app, kind, area),
        Mode::InstallPicker => picker::render_install_picker(frame, app, area),
        Mode::RemoteResults => picker::render_remote_results(frame, app, area),
        Mode::Confirm => modals::render_confirmation(frame, app, area),
        Mode::Help => modals::render_help(frame, area),
        Mode::Browse => {}
    }
}

fn rounded_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}

fn panel(title: impl Into<String>) -> Block<'static> {
    rounded_block().title(title.into())
}

fn colored_panel(title: impl Into<String>, color: Color) -> Block<'static> {
    panel(title).border_style(Style::default().fg(color))
}

fn popup_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical_margin = (100u16.saturating_sub(percent_y)) / 2;
    let horizontal_margin = (100u16.saturating_sub(percent_x)) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(vertical_margin),
            Constraint::Percentage(percent_y),
            Constraint::Percentage(100u16.saturating_sub(vertical_margin + percent_y)),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(horizontal_margin),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(100u16.saturating_sub(horizontal_margin + percent_x)),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn render_popup_panel(
    frame: &mut Frame,
    area: Rect,
    percent_x: u16,
    percent_y: u16,
    block: Block<'static>,
) -> Rect {
    let popup = popup_rect(percent_x, percent_y, area);
    frame.render_widget(Clear, popup);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    inner
}
