use crate::{
    manager::{InstallTarget, SkillQuery},
    model::{Skill, SkillState},
};

use super::{App, InstallPickerState, Mode, PendingAction, StatusKind};

impl App {
    pub(super) fn open_install_source(&mut self, source: &str) {
        match self.manager.prepare_source(source, false) {
            Ok(prepared) => {
                self.install_picker = Some(InstallPickerState::for_source(prepared));
                self.input.clear();
                self.mode = Mode::InstallPicker;
            }
            Err(error) => {
                self.set_status(
                    StatusKind::Error,
                    format!("Could not open source: {error:#}"),
                );
            }
        }
    }

    pub(super) fn search_remote_catalog(&mut self, query: String) {
        match self.manager.search_remote(&query, None, 20) {
            Ok(results) => {
                self.remote_query = query;
                self.remote_results = results;
                self.remote_selection.reset();
                self.input.clear();
                self.mode = Mode::RemoteResults;
            }
            Err(error) => {
                self.set_status(
                    StatusKind::Error,
                    format!("Remote search failed: {error:#}"),
                );
            }
        }
    }

    pub(super) fn install_selected(&mut self, picker: &InstallPickerState) -> bool {
        let selected = picker.selected_indices();
        if selected.is_empty() {
            self.set_status(StatusKind::Error, "Select at least one skill to install.");
            return false;
        }

        let target = InstallTarget {
            scope: picker.scope,
            agents: Vec::new(),
            custom_root: None,
            replace: picker.replace,
        };
        match self.manager.install(&picker.prepared, &selected, &target) {
            Ok(report) => {
                self.mode = Mode::Browse;
                self.refresh(format!(
                    "Installed {} skill copy/copies.",
                    report.installed.len()
                ));
                true
            }
            Err(error) => {
                self.set_status(StatusKind::Error, format!("Install failed: {error:#}"));
                false
            }
        }
    }

    pub(super) fn prepare_selected_remote(&mut self) {
        let Some(remote) = self.selected_remote().cloned() else {
            self.set_status(StatusKind::Info, "No remote skill selected.");
            return;
        };
        let source = remote.install_source();

        match self.manager.prepare_source(&source, false) {
            Ok(prepared) => {
                let selected = prepared
                    .skills
                    .iter()
                    .enumerate()
                    .filter(|(_, skill)| skill.exact_name_match(&remote.name))
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                self.install_picker = Some(InstallPickerState::new(prepared, selected));
                self.mode = Mode::InstallPicker;
            }
            Err(error) => self.set_status(
                StatusKind::Error,
                format!("Could not prepare {}: {error:#}", remote.name),
            ),
        }
    }

    pub(super) fn set_search(&mut self, query: String) {
        let selected_id = self.selected_skill().map(|skill| skill.id.clone());
        self.query = query;
        self.rebuild_visible(selected_id.as_deref());
    }

    pub(super) fn clear_search(&mut self) {
        if self.query.is_empty() {
            self.set_status(StatusKind::Info, "Search is already clear.");
            return;
        }

        self.set_search(String::new());
        self.set_status(StatusKind::Info, "Search cleared.");
    }

    pub(super) fn cycle_scope(&mut self) {
        let selected_id = self.selected_skill().map(|skill| skill.id.clone());
        self.scope_view = self.scope_view.next();
        self.rebuild_visible(selected_id.as_deref());
        self.set_status(
            StatusKind::Info,
            format!("Scope filter: {}", self.scope_view.label()),
        );
    }

    pub(super) fn cycle_state(&mut self) {
        let selected_id = self.selected_skill().map(|skill| skill.id.clone());
        self.state_view = self.state_view.next();
        self.rebuild_visible(selected_id.as_deref());
        self.set_status(
            StatusKind::Info,
            format!("State filter: {}", self.state_view.label()),
        );
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        if self.selection.move_by(delta, self.visible.len()) {
            self.preview_scroll = 0;
        }
    }

    pub(super) fn select_first(&mut self) {
        if self.selection.first(self.visible.len()) {
            self.preview_scroll = 0;
        }
    }

    pub(super) fn select_last(&mut self) {
        if self.selection.last(self.visible.len()) {
            self.preview_scroll = 0;
        }
    }

    pub(super) fn move_remote_selection(&mut self, delta: isize) {
        self.remote_selection
            .move_by(delta, self.remote_results.len());
    }

    pub(super) fn select_first_remote(&mut self) {
        self.remote_selection.first(self.remote_results.len());
    }

    pub(super) fn select_last_remote(&mut self) {
        self.remote_selection.last(self.remote_results.len());
    }

    pub(super) fn scroll_preview(&mut self, delta: i32) {
        self.preview_scroll = if delta.is_negative() {
            self.preview_scroll
                .saturating_sub(delta.unsigned_abs() as u16)
        } else {
            self.preview_scroll.saturating_add(delta as u16)
        };
    }

    pub(super) fn toggle_selected(&mut self) {
        let Some(skill) = self.selected_skill().cloned() else {
            self.set_status(StatusKind::Info, "No skill selected.");
            return;
        };
        let target = skill.state.opposite();
        self.apply_change(vec![skill], target);
    }

    pub(super) fn change_selected(&mut self, target: SkillState) {
        let Some(skill) = self.selected_skill().cloned() else {
            self.set_status(StatusKind::Info, "No skill selected.");
            return;
        };
        if skill.state == target {
            self.set_status(
                StatusKind::Info,
                format!("{} is already {target}.", skill.title),
            );
            return;
        }
        self.apply_change(vec![skill], target);
    }

    pub(super) fn confirm_filtered_change(&mut self, target: SkillState) {
        let skills = self
            .visible_skill_copies()
            .into_iter()
            .filter(|skill| skill.state != target)
            .collect::<Vec<_>>();
        if skills.is_empty() {
            self.set_status(
                StatusKind::Info,
                format!("No visible skills need to be made {target}."),
            );
            return;
        }

        self.pending = Some(PendingAction::Change { skills, target });
        self.mode = Mode::Confirm;
    }

    pub(super) fn confirm_selected_remove(&mut self) {
        let Some(skill) = self.selected_skill().cloned() else {
            self.set_status(StatusKind::Info, "No skill selected.");
            return;
        };

        self.pending = Some(PendingAction::Remove {
            skills: vec![skill],
        });
        self.mode = Mode::Confirm;
    }

    pub(super) fn confirm_filtered_remove(&mut self) {
        let skills = self.visible_skill_copies();
        if skills.is_empty() {
            self.set_status(StatusKind::Info, "No visible skills to remove.");
            return;
        }

        self.pending = Some(PendingAction::Remove { skills });
        self.mode = Mode::Confirm;
    }

    pub(super) fn execute_pending(&mut self) {
        let Some(action) = self.pending.take() else {
            self.mode = Mode::Browse;
            return;
        };
        self.mode = Mode::Browse;

        match action {
            PendingAction::Change { skills, target } => self.apply_change(skills, target),
            PendingAction::Remove { skills } => match self.manager.remove(&skills, false) {
                Ok(changes) => self.refresh(format!(
                    "Removed {} skill(s) to reversible trash.",
                    changes.len()
                )),
                Err(error) => {
                    self.set_status(StatusKind::Error, format!("Remove failed: {error:#}"));
                }
            },
        }
    }

    pub(super) fn refresh(&mut self, message: impl Into<String>) {
        let selected_id = self.selected_skill().map(|skill| skill.id.clone());
        self.catalog = self.manager.refresh();
        self.preview_cache = None;
        self.rebuild_visible(selected_id.as_deref());

        let mut message = message.into();
        let warning_count = self.catalog.warnings.len();
        if warning_count > 0 {
            message.push_str(&format!(" {warning_count} discovery warning(s)."));
        }
        self.set_status(StatusKind::Success, message);
    }

    pub(super) fn rebuild_visible(&mut self, preferred_id: Option<&str>) {
        let query = SkillQuery {
            text: self.query.trim().to_owned(),
            scope: self.scope_view.filter(),
            state: self.state_view.filter(),
            root: None,
        };
        self.visible = query.ranked_indices(&self.catalog);

        let selected = preferred_id
            .and_then(|id| {
                self.visible.iter().position(|index| {
                    self.catalog
                        .skills
                        .get(*index)
                        .is_some_and(|skill| skill.id == id)
                })
            })
            .unwrap_or_else(|| self.selection.index());
        self.selection.select(selected, self.visible.len());
        self.preview_scroll = 0;
    }

    fn visible_skill_copies(&self) -> Vec<Skill> {
        self.visible
            .iter()
            .filter_map(|index| self.catalog.skills.get(*index))
            .cloned()
            .collect()
    }

    fn apply_change(&mut self, skills: Vec<Skill>, target: SkillState) {
        match self.manager.change_state(&skills, target) {
            Ok(changes) => self.refresh(format!("Made {} skill(s) {target}.", changes.len())),
            Err(error) => self.set_status(
                StatusKind::Error,
                format!("Could not make skill(s) {target}: {error:#}"),
            ),
        }
    }
}
