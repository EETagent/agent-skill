use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::SkillState;

use super::{App, Focus, InputKind, Mode, StatusKind};

impl App {
    pub(super) fn dispatch_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return;
        }

        match self.mode {
            Mode::Browse => self.handle_browse_key(key),
            Mode::Input(kind) => self.handle_input_key(kind, key),
            Mode::InstallPicker => self.handle_install_picker_key(key),
            Mode::RemoteResults => self.handle_remote_results_key(key),
            Mode::Confirm => self.handle_confirm_key(key),
            Mode::Help => self.handle_help_key(key),
        }
    }

    fn handle_browse_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('u') => self.scroll_preview(-10),
                KeyCode::Char('d') => self.scroll_preview(10),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Tab => self.focus = self.focus.toggled(),
            KeyCode::Down | KeyCode::Char('j') => match self.focus {
                Focus::Skills => self.move_selection(1),
                Focus::Preview => self.scroll_preview(1),
            },
            KeyCode::Up | KeyCode::Char('k') => match self.focus {
                Focus::Skills => self.move_selection(-1),
                Focus::Preview => self.scroll_preview(-1),
            },
            KeyCode::Home => match self.focus {
                Focus::Skills => self.select_first(),
                Focus::Preview => self.preview_scroll = 0,
            },
            KeyCode::End => match self.focus {
                Focus::Skills => self.select_last(),
                Focus::Preview => self.preview_scroll = u16::MAX,
            },
            KeyCode::PageUp => self.scroll_preview(-12),
            KeyCode::PageDown => self.scroll_preview(12),
            KeyCode::Char('/') => self.begin_input(InputKind::Search),
            KeyCode::Char('c') => self.clear_search(),
            KeyCode::Char('g') => self.cycle_scope(),
            KeyCode::Char('s') => self.cycle_state(),
            KeyCode::Char('r') => self.refresh("Catalog refreshed."),
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected(),
            KeyCode::Char('e') => self.change_selected(SkillState::Enabled),
            KeyCode::Char('d') => self.change_selected(SkillState::Disabled),
            KeyCode::Char('E') => self.confirm_filtered_change(SkillState::Enabled),
            KeyCode::Char('D') => self.confirm_filtered_change(SkillState::Disabled),
            KeyCode::Char('x') => self.confirm_selected_remove(),
            KeyCode::Char('X') => self.confirm_filtered_remove(),
            KeyCode::Char('i') => self.begin_input(InputKind::Install),
            KeyCode::Char('f') => self.begin_input(InputKind::Remote),
            _ => {}
        }
    }

    fn handle_input_key(&mut self, kind: InputKind, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_input(kind),
            KeyCode::Enter => self.submit_input(kind),
            KeyCode::Backspace => {
                self.input.pop();
                self.apply_live_search(kind);
            }
            KeyCode::Delete if kind == InputKind::Search => {
                self.input.clear();
                self.apply_live_search(kind);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                self.apply_live_search(kind);
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                trim_last_word(&mut self.input);
                self.apply_live_search(kind);
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.input.push(character);
                self.apply_live_search(kind);
            }
            _ => {}
        }
    }

    fn handle_install_picker_key(&mut self, key: KeyEvent) {
        let Some(mut picker) = self.install_picker.take() else {
            self.mode = Mode::Browse;
            return;
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Browse;
                self.set_status(StatusKind::Info, "Install cancelled.");
                return;
            }
            KeyCode::Down | KeyCode::Char('j') => picker.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => picker.move_by(-1),
            KeyCode::Home => picker.select_first(),
            KeyCode::End => picker.select_last(),
            KeyCode::Char(' ') => picker.toggle_current(),
            KeyCode::Char('a') => picker.toggle_all(),
            KeyCode::Tab => picker.cycle_scope(),
            KeyCode::Char('r') => picker.replace = !picker.replace,
            KeyCode::Enter => {
                if self.install_selected(&picker) {
                    return;
                }
            }
            _ => {}
        }

        self.install_picker = Some(picker);
    }

    fn handle_remote_results_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Browse,
            KeyCode::Down | KeyCode::Char('j') => self.move_remote_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_remote_selection(-1),
            KeyCode::Home => self.select_first_remote(),
            KeyCode::End => self.select_last_remote(),
            KeyCode::Char('/') => self.begin_input(InputKind::Remote),
            KeyCode::Enter | KeyCode::Char('i') => self.prepare_selected_remote(),
            _ => {}
        }
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.execute_pending(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.pending = None;
                self.mode = Mode::Browse;
                self.set_status(StatusKind::Info, "Operation cancelled.");
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
        ) {
            self.mode = Mode::Browse;
        }
    }

    fn begin_input(&mut self, kind: InputKind) {
        match kind {
            InputKind::Search => {
                self.search_before_edit.clone_from(&self.query);
                self.input.clone_from(&self.query);
            }
            InputKind::Remote if self.mode == Mode::RemoteResults => {
                self.input.clone_from(&self.remote_query);
            }
            InputKind::Install | InputKind::Remote => self.input.clear(),
        }
        self.mode = Mode::Input(kind);
    }

    fn cancel_input(&mut self, kind: InputKind) {
        if kind == InputKind::Search {
            self.set_search(self.search_before_edit.clone());
        }
        self.input.clear();
        self.mode = Mode::Browse;
    }

    fn submit_input(&mut self, kind: InputKind) {
        match kind {
            InputKind::Search => {
                self.set_search(self.input.clone());
                self.input.clear();
                self.mode = Mode::Browse;
            }
            InputKind::Install => {
                let source = self.input.trim().to_owned();
                if source.is_empty() {
                    self.set_status(StatusKind::Error, "Enter a source path or repository.");
                    return;
                }
                self.open_install_source(&source);
            }
            InputKind::Remote => {
                let query = self.input.trim().to_owned();
                if query.is_empty() {
                    self.set_status(StatusKind::Error, "Enter a remote search query.");
                    return;
                }
                self.search_remote_catalog(query);
            }
        }
    }

    fn apply_live_search(&mut self, kind: InputKind) {
        if kind == InputKind::Search {
            self.set_search(self.input.clone());
        }
    }
}

fn trim_last_word(input: &mut String) {
    while input.chars().next_back().is_some_and(char::is_whitespace) {
        input.pop();
    }
    while input
        .chars()
        .next_back()
        .is_some_and(|character| !character.is_whitespace())
    {
        input.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::trim_last_word;

    #[test]
    fn trims_trailing_whitespace_and_the_previous_word() {
        let mut input = "alpha beta   ".to_owned();
        trim_last_word(&mut input);
        assert_eq!(input, "alpha ");

        trim_last_word(&mut input);
        assert!(input.is_empty());
    }
}
