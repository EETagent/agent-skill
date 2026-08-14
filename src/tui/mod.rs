mod app;
mod highlight;
mod ui;

use std::io::{self, IsTerminal};

use anyhow::{anyhow, bail, Context, Result};
use ratatui::{
    crossterm::event::{self, Event, KeyEventKind},
    DefaultTerminal,
};

use crate::manager::SkillManager;

use app::App;

pub fn run(manager: SkillManager) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "the interactive UI requires a terminal; use `skillctl list`, `find`, \
             `enable`, or another subcommand for non-interactive use"
        );
    }

    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(error) => {
            let init_error = anyhow::Error::new(error).context("failed to initialize terminal UI");
            return match ratatui::try_restore().context("failed to restore terminal state") {
                Ok(()) => Err(init_error),
                Err(restore_error) => Err(anyhow!(
                    "{init_error:#}; terminal restoration also failed: {restore_error:#}"
                )),
            };
        }
    };
    let app_result = run_loop(&mut terminal, manager);
    let restore_result = ratatui::try_restore().context("failed to restore terminal state");

    match (app_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(app_error), Err(restore_error)) => Err(anyhow!(
            "{app_error:#}; terminal restoration also failed: {restore_error:#}"
        )),
    }
}

fn run_loop(terminal: &mut DefaultTerminal, manager: SkillManager) -> Result<()> {
    let mut app = App::new(manager);

    while !app.should_quit {
        terminal
            .draw(|frame| ui::render(frame, &mut app))
            .context("failed to draw terminal UI")?;

        match event::read().context("failed to read terminal event")? {
            Event::Key(key) if key.kind != KeyEventKind::Release => app.handle_key(key),
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    Ok(())
}
