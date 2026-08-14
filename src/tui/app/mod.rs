mod actions;
mod input;

use std::{collections::BTreeSet, path::PathBuf};

use ratatui::{crossterm::event::KeyEvent, text::Line};

use crate::{
    manager::SkillManager,
    model::{Catalog, Scope, Skill, SkillState},
    remote::RemoteSkill,
    source::PreparedSource,
    terminal::sanitize_inline,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputKind {
    Search,
    Install,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Browse,
    Input(InputKind),
    InstallPicker,
    RemoteResults,
    Confirm,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Skills,
    Preview,
}

impl Focus {
    pub(super) const fn toggled(self) -> Self {
        match self {
            Self::Skills => Self::Preview,
            Self::Preview => Self::Skills,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScopeView {
    All,
    Local,
    Global,
}

impl ScopeView {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Local => "local",
            Self::Global => "global",
        }
    }

    pub(super) const fn filter(self) -> Option<Scope> {
        match self {
            Self::All => None,
            Self::Local => Some(Scope::Local),
            Self::Global => Some(Scope::Global),
        }
    }

    pub(super) const fn next(self) -> Self {
        match self {
            Self::All => Self::Local,
            Self::Local => Self::Global,
            Self::Global => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StateView {
    All,
    Enabled,
    Disabled,
}

impl StateView {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    pub(super) const fn filter(self) -> Option<SkillState> {
        match self {
            Self::All => None,
            Self::Enabled => Some(SkillState::Enabled),
            Self::Disabled => Some(SkillState::Disabled),
        }
    }

    pub(super) const fn next(self) -> Self {
        match self {
            Self::All => Self::Enabled,
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatusKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub(super) struct Status {
    pub(super) message: String,
    pub(super) kind: StatusKind,
}

impl Status {
    pub(super) fn new(kind: StatusKind, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            message: sanitize_inline(&message),
            kind,
        }
    }

    fn info(message: impl Into<String>) -> Self {
        Self::new(StatusKind::Info, message)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ListCursor {
    index: usize,
    offset: usize,
}

impl ListCursor {
    pub(super) fn new(index: usize, length: usize) -> Self {
        let mut cursor = Self::default();
        cursor.select(index, length);
        cursor
    }

    pub(super) const fn index(self) -> usize {
        self.index
    }

    pub(super) const fn offset(self) -> usize {
        self.offset
    }

    pub(super) fn select(&mut self, index: usize, length: usize) -> bool {
        let next = if length == 0 {
            0
        } else {
            index.min(length - 1)
        };
        let changed = self.index != next;
        self.index = next;
        self.offset = if length == 0 {
            0
        } else {
            self.offset.min(self.index)
        };
        changed
    }

    pub(super) fn move_by(&mut self, delta: isize, length: usize) -> bool {
        if length == 0 {
            return self.select(0, 0);
        }

        let index = if delta.is_negative() {
            self.index.saturating_sub(delta.unsigned_abs())
        } else {
            self.index.saturating_add(delta as usize)
        };
        self.select(index, length)
    }

    pub(super) fn first(&mut self, length: usize) -> bool {
        self.select(0, length)
    }

    pub(super) fn last(&mut self, length: usize) -> bool {
        self.select(length.saturating_sub(1), length)
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn ensure_visible(&mut self, length: usize, rows: usize) {
        if rows == 0 || length == 0 {
            self.offset = 0;
            return;
        }

        self.index = self.index.min(length - 1);
        if self.index < self.offset {
            self.offset = self.index;
        } else if self.index >= self.offset.saturating_add(rows) {
            self.offset = self.index.saturating_add(1).saturating_sub(rows);
        }

        let maximum_offset = length.saturating_sub(rows.min(length));
        self.offset = self.offset.min(maximum_offset);
    }
}

#[derive(Debug)]
pub(super) struct CachedLines<K> {
    key: K,
    lines: Vec<Line<'static>>,
}

impl<K: PartialEq> CachedLines<K> {
    pub(super) fn new(key: K, lines: Vec<Line<'static>>) -> Self {
        Self { key, lines }
    }

    pub(super) fn is_current(&self, key: &K) -> bool {
        self.key.eq(key)
    }

    pub(super) fn lines(&self) -> &[Line<'static>] {
        &self.lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SkillPreviewKey {
    skill_id: String,
    skill_path: PathBuf,
    skill_state: SkillState,
    width: u16,
}

impl SkillPreviewKey {
    pub(super) fn new(skill: &Skill, width: u16) -> Self {
        Self {
            skill_id: skill.id.clone(),
            skill_path: skill.path.clone(),
            skill_state: skill.state,
            width,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InstallPreviewKey {
    index: usize,
    width: u16,
}

impl InstallPreviewKey {
    pub(super) const fn new(index: usize, width: u16) -> Self {
        Self { index, width }
    }
}

pub(super) type PreviewCache = CachedLines<SkillPreviewKey>;
pub(super) type InstallPreviewCache = CachedLines<InstallPreviewKey>;

#[derive(Debug)]
pub(super) struct InstallPickerState {
    pub(super) prepared: PreparedSource,
    selected: BTreeSet<usize>,
    cursor: ListCursor,
    pub(super) scope: Scope,
    pub(super) replace: bool,
    pub(super) preview_cache: Option<InstallPreviewCache>,
}

impl InstallPickerState {
    pub(super) fn new(prepared: PreparedSource, selected: impl IntoIterator<Item = usize>) -> Self {
        let skill_count = prepared.skills.len();
        let selected = selected
            .into_iter()
            .filter(|index| *index < skill_count)
            .collect::<BTreeSet<_>>();
        let initial_index = selected.iter().next().copied().unwrap_or(0);

        Self {
            prepared,
            selected,
            cursor: ListCursor::new(initial_index, skill_count),
            scope: Scope::Local,
            replace: false,
            preview_cache: None,
        }
    }

    pub(super) fn for_source(prepared: PreparedSource) -> Self {
        let selected = (prepared.skills.len() == 1).then_some(0usize);
        Self::new(prepared, selected)
    }

    pub(super) fn len(&self) -> usize {
        self.prepared.skills.len()
    }

    pub(super) fn cursor_index(&self) -> usize {
        self.cursor.index()
    }

    pub(super) fn offset(&self) -> usize {
        self.cursor.offset()
    }

    pub(super) fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub(super) fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    pub(super) fn selected_indices(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }

    pub(super) fn move_by(&mut self, delta: isize) {
        self.cursor.move_by(delta, self.len());
    }

    pub(super) fn select_first(&mut self) {
        self.cursor.first(self.len());
    }

    pub(super) fn select_last(&mut self) {
        self.cursor.last(self.len());
    }

    pub(super) fn ensure_visible(&mut self, rows: usize) {
        self.cursor.ensure_visible(self.len(), rows);
    }

    pub(super) fn toggle_current(&mut self) {
        let index = self.cursor.index();
        if index >= self.len() {
            return;
        }

        if !self.selected.remove(&index) {
            self.selected.insert(index);
        }
    }

    pub(super) fn toggle_all(&mut self) {
        if self.selected.len() == self.len() {
            self.selected.clear();
        } else {
            self.selected = (0..self.len()).collect();
        }
    }

    pub(super) fn cycle_scope(&mut self) {
        self.scope = match self.scope {
            Scope::Local => Scope::Global,
            Scope::Global => Scope::Local,
        };
    }
}

#[derive(Debug, Clone)]
pub(super) enum PendingAction {
    Change {
        skills: Vec<Skill>,
        target: SkillState,
    },
    Remove {
        skills: Vec<Skill>,
    },
}

impl PendingAction {
    pub(super) fn prompt(&self) -> String {
        match self {
            Self::Change { skills, target } => format!(
                "Make {} skill(s) {target}? This moves directories between the enabled and disabled stores.",
                skills.len()
            ),
            Self::Remove { skills } => format!(
                "Remove {} skill(s)? They will be moved to a reversible .*-trash store.",
                skills.len()
            ),
        }
    }
}

#[derive(Debug)]
pub(super) struct App {
    pub(super) manager: SkillManager,
    pub(super) catalog: Catalog,
    pub(super) visible: Vec<usize>,
    selection: ListCursor,
    pub(super) preview_scroll: u16,
    pub(super) preview_cache: Option<PreviewCache>,
    pub(super) focus: Focus,
    pub(super) mode: Mode,
    pub(super) query: String,
    pub(super) input: String,
    pub(super) search_before_edit: String,
    pub(super) scope_view: ScopeView,
    pub(super) state_view: StateView,
    pub(super) status: Status,
    pub(super) should_quit: bool,
    pub(super) pending: Option<PendingAction>,
    pub(super) install_picker: Option<InstallPickerState>,
    pub(super) remote_query: String,
    pub(super) remote_results: Vec<RemoteSkill>,
    remote_selection: ListCursor,
}

impl App {
    pub(super) fn new(manager: SkillManager) -> Self {
        let catalog = manager.refresh();
        let warning_count = catalog.warnings.len();
        let mut app = Self {
            manager,
            catalog,
            visible: Vec::new(),
            selection: ListCursor::default(),
            preview_scroll: 0,
            preview_cache: None,
            focus: Focus::Skills,
            mode: Mode::Browse,
            query: String::new(),
            input: String::new(),
            search_before_edit: String::new(),
            scope_view: ScopeView::All,
            state_view: StateView::All,
            status: Status::info(if warning_count == 0 {
                "Ready. Press ? for help.".to_owned()
            } else {
                format!("Ready with {warning_count} discovery warning(s). Press ? for help.")
            }),
            should_quit: false,
            pending: None,
            install_picker: None,
            remote_query: String::new(),
            remote_results: Vec::new(),
            remote_selection: ListCursor::default(),
        };
        app.rebuild_visible(None);
        app
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) {
        self.dispatch_key(key);
    }

    pub(super) fn selected_skill(&self) -> Option<&Skill> {
        self.visible
            .get(self.selection.index())
            .and_then(|index| self.catalog.skills.get(*index))
    }

    pub(super) fn selected_position(&self) -> usize {
        self.selection.index()
    }

    pub(super) fn list_offset(&self) -> usize {
        self.selection.offset()
    }

    pub(super) fn remote_position(&self) -> usize {
        self.remote_selection.index()
    }

    pub(super) fn remote_offset(&self) -> usize {
        self.remote_selection.offset()
    }

    pub(super) fn selected_remote(&self) -> Option<&RemoteSkill> {
        self.remote_results.get(self.remote_selection.index())
    }

    pub(super) fn ensure_skill_visible(&mut self, rows: usize) {
        self.selection.ensure_visible(self.visible.len(), rows);
    }

    pub(super) fn ensure_remote_visible(&mut self, rows: usize) {
        self.remote_selection
            .ensure_visible(self.remote_results.len(), rows);
    }

    pub(super) fn ensure_install_visible(&mut self, rows: usize) {
        if let Some(picker) = self.install_picker.as_mut() {
            picker.ensure_visible(rows);
        }
    }

    pub(super) fn set_status(&mut self, kind: StatusKind, message: impl Into<String>) {
        self.status = Status::new(kind, message);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;

    use super::{CachedLines, ListCursor};

    #[test]
    fn list_cursor_clamps_selection_and_keeps_it_visible() {
        let mut cursor = ListCursor::new(8, 10);
        cursor.ensure_visible(10, 4);
        assert_eq!((cursor.index(), cursor.offset()), (8, 5));

        cursor.move_by(-7, 10);
        cursor.ensure_visible(10, 4);
        assert_eq!((cursor.index(), cursor.offset()), (1, 1));

        cursor.last(3);
        cursor.ensure_visible(3, 20);
        assert_eq!((cursor.index(), cursor.offset()), (2, 0));

        cursor.select(99, 0);
        assert_eq!((cursor.index(), cursor.offset()), (0, 0));
    }

    #[test]
    fn cached_lines_are_invalidated_only_when_the_key_changes() {
        let cache = CachedLines::new(("skill-one", 80), vec![Line::from("cached")]);

        assert!(cache.is_current(&("skill-one", 80)));
        assert!(!cache.is_current(&("skill-two", 80)));
        assert!(!cache.is_current(&("skill-one", 79)));
        assert_eq!(cache.lines().len(), 1);
    }
}
