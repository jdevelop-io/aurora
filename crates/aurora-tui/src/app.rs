use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
use crossterm::event::{KeyCode, KeyEvent};
use std::time::Instant;

// ── BeamView ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BeamView {
    pub name: String,
    pub depends_on: Vec<String>,
    pub status: BeamStatus,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub started_at: Option<Instant>,
}

impl BeamView {
    pub fn new(name: String, depends_on: Vec<String>) -> Self {
        BeamView {
            name,
            depends_on,
            status: BeamStatus::Pending,
            stdout: vec![],
            stderr: vec![],
            started_at: None,
        }
    }

    pub fn status_symbol(&self) -> &str {
        match &self.status {
            BeamStatus::Pending => "─",
            BeamStatus::Running => "⣴",
            BeamStatus::Success { cached: true, .. } => "✦",
            BeamStatus::Success { cached: false, .. } => "✔",
            BeamStatus::Skipped { .. } => "◌",
            BeamStatus::Failed { .. } => "✕",
            BeamStatus::Cancelled => "✕",
        }
    }

    pub fn all_logs(&self) -> Vec<String> {
        let mut all = self.stdout.clone();
        if !self.stderr.is_empty() {
            all.push("── stderr ──".to_string());
            all.extend(self.stderr.clone());
        }
        all
    }
}

// ── FocusPanel ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FocusPanel {
    Beams,
    Logs,
}

// ── Actions retournées par handle_key ────────────────────────────

#[derive(Debug, PartialEq)]
pub enum PickerAction {
    Launch(Vec<String>),
    Quit,
}

#[derive(Debug, PartialEq)]
pub enum ExecutionAction {
    Quit,
    OpenLogView { beam_index: usize },
    Rerun { root: String, pre_success: Vec<String> },
}

#[derive(Debug, PartialEq)]
pub enum LogViewAction {
    Close,
    Quit,
}

// ── PickerState ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PickerBeam {
    pub name: String,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
}

pub struct PickerState {
    pub beams: Vec<PickerBeam>,
    pub selected: usize,
    pub search: String,
    pub show_deps: bool,
    pub checked: Vec<bool>,
}

impl PickerState {
    pub fn new(beam_info: Vec<(String, Option<String>, Vec<String>)>) -> Self {
        let len = beam_info.len();
        PickerState {
            beams: beam_info
                .into_iter()
                .map(|(name, description, depends_on)| PickerBeam {
                    name,
                    description,
                    depends_on,
                })
                .collect(),
            selected: 0,
            search: String::new(),
            show_deps: false,
            checked: vec![false; len],
        }
    }

    pub fn filtered(&self) -> Vec<(usize, &PickerBeam, u32)> {
        use crate::picker::fuzzy::fuzzy_score;
        if self.search.is_empty() {
            return self
                .beams
                .iter()
                .enumerate()
                .map(|(i, b)| (i, b, 500))
                .collect();
        }
        let mut results: Vec<(usize, &PickerBeam, u32)> = self
            .beams
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                let score = fuzzy_score(&self.search, &b.name, b.description.as_deref());
                if score > 0 {
                    Some((i, b, score))
                } else {
                    None
                }
            })
            .collect();
        results.sort_by(|a, b| b.2.cmp(&a.2));
        results
    }

    pub fn selected_beam_indices(&self) -> Vec<usize> {
        self.checked
            .iter()
            .enumerate()
            .filter(|(_, &checked)| checked)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PickerAction> {
        let filtered = self.filtered();
        let count = filtered.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Some(PickerAction::Quit),
            KeyCode::Enter => {
                let checked = self.selected_beam_indices();
                if checked.is_empty() {
                    if let Some((_orig_idx, beam, _)) = filtered.get(self.selected) {
                        return Some(PickerAction::Launch(vec![beam.name.clone()]));
                    }
                } else {
                    let names = checked.iter().map(|&i| self.beams[i].name.clone()).collect();
                    return Some(PickerAction::Launch(names));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Char(' ') => {
                let orig_idx = filtered.get(self.selected).map(|(i, _, _)| *i);
                drop(filtered);
                if let Some(idx) = orig_idx {
                    self.checked[idx] = !self.checked[idx];
                }
                return None;
            }
            KeyCode::Tab => {
                self.show_deps = !self.show_deps;
            }
            KeyCode::Backspace => {
                self.search.pop();
                self.selected = 0;
            }
            KeyCode::Char(c) => {
                self.search.push(c);
                self.selected = 0;
            }
            _ => {}
        }
        None
    }
}

// ── ExecutionState ───────────────────────────────────────────────

pub struct ExecutionState {
    pub beams: Vec<BeamView>,
    pub selected: usize,
    pub done: Option<bool>,
    pub focus: FocusPanel,
}

impl ExecutionState {
    pub fn new(beam_info: Vec<(String, Vec<String>)>) -> Self {
        ExecutionState {
            beams: beam_info.into_iter().map(|(name, deps)| BeamView::new(name, deps)).collect(),
            selected: 0,
            done: None,
            focus: FocusPanel::Beams,
        }
    }

    pub fn apply_event(&mut self, event: SchedulerEvent) {
        match event {
            SchedulerEvent::BeamStarted { name } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = BeamStatus::Running;
                    b.started_at = Some(Instant::now());
                }
            }
            SchedulerEvent::BeamCompleted { name, status } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = status;
                }
            }
            SchedulerEvent::BeamOutput { name, line, is_stderr } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    if is_stderr {
                        b.stderr.push(line);
                    } else {
                        b.stdout.push(line);
                    }
                }
            }
            SchedulerEvent::AllDone { success } => {
                self.done = Some(success);
            }
        }
    }

    /// Calcule les beams à relancer depuis le beam sélectionné.
    /// Retourne (root_name, to_rerun, pre_success).
    pub fn compute_rerun(&self, selected: usize) -> (String, Vec<String>, Vec<String>) {
        let root_name = self.beams[selected].name.clone();
        let mut to_rerun = vec![];
        let mut pre_success = vec![];

        let mut stack = vec![selected];
        let mut visited: Vec<usize> = vec![];

        while let Some(idx) = stack.pop() {
            if visited.contains(&idx) {
                continue;
            }
            visited.push(idx);
            let beam = &self.beams[idx];
            if idx == selected {
                // Le beam racine est toujours relancé, quel que soit son statut
                to_rerun.push(beam.name.clone());
            } else {
                match &beam.status {
                    BeamStatus::Failed { .. } | BeamStatus::Cancelled => {
                        to_rerun.push(beam.name.clone());
                    }
                    BeamStatus::Success { .. } | BeamStatus::Skipped { .. } => {
                        pre_success.push(beam.name.clone());
                    }
                    _ => {}
                }
            }
            for dep_name in &beam.depends_on {
                if let Some(dep_idx) = self.beams.iter().position(|b| &b.name == dep_name) {
                    if !visited.contains(&dep_idx) {
                        stack.push(dep_idx);
                    }
                }
            }
        }

        (root_name, to_rerun, pre_success)
    }

    /// Remet les beams listés à Pending et vide leurs logs. Reset aussi exec.done.
    pub fn reset_for_rerun(&mut self, names: &[String]) {
        for beam in self.beams.iter_mut() {
            if names.contains(&beam.name) {
                beam.status = BeamStatus::Pending;
                beam.stdout.clear();
                beam.stderr.clear();
                beam.started_at = None;
            }
        }
        self.done = None;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExecutionAction> {
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPanel::Beams => FocusPanel::Logs,
                    FocusPanel::Logs => FocusPanel::Beams,
                };
                None
            }
            KeyCode::Char('q') => Some(ExecutionAction::Quit),
            KeyCode::Enter => Some(ExecutionAction::OpenLogView {
                beam_index: self.selected,
            }),
            KeyCode::Char('r') => {
                if self.done.is_some() {
                    let beam = &self.beams[self.selected];
                    if matches!(beam.status, BeamStatus::Failed { .. } | BeamStatus::Cancelled | BeamStatus::Success { .. } | BeamStatus::Skipped { .. }) {
                        let (root, to_rerun, pre_success) = self.compute_rerun(self.selected);
                        self.reset_for_rerun(&to_rerun);
                        return Some(ExecutionAction::Rerun { root, pre_success });
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.beams.len().saturating_sub(1));
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}

// ── LogViewState ─────────────────────────────────────────────────

pub struct LogViewState {
    pub beam_index: usize,
    pub scroll: u16,
    pub scroll_locked: bool, // true = l'utilisateur a scrollé manuellement
}

impl LogViewState {
    pub fn new(beam_index: usize, total_lines: usize) -> Self {
        let scroll = total_lines.saturating_sub(1) as u16;
        LogViewState {
            beam_index,
            scroll,
            scroll_locked: false,
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        total_lines: usize,
        panel_height: u16,
    ) -> Option<LogViewAction> {
        let max_scroll = total_lines.saturating_sub(1) as u16;
        match key.code {
            KeyCode::Esc => return Some(LogViewAction::Close),
            KeyCode::Char('q') => return Some(LogViewAction::Quit),
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                self.scroll_locked = true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = (self.scroll + 1).min(max_scroll);
                if self.scroll >= max_scroll {
                    self.scroll_locked = false;
                }
            }
            KeyCode::Char('G') => {
                self.scroll = max_scroll;
                self.scroll_locked = false;
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(panel_height.saturating_sub(2));
                self.scroll_locked = true;
            }
            KeyCode::PageDown => {
                self.scroll = (self.scroll + panel_height.saturating_sub(2)).min(max_scroll);
                if self.scroll >= max_scroll {
                    self.scroll_locked = false;
                }
            }
            _ => {}
        }
        None
    }

    /// Appelé à chaque tick : si auto-scroll actif, coller au bas.
    pub fn auto_scroll(&mut self, total_lines: usize) {
        if !self.scroll_locked {
            self.scroll = total_lines.saturating_sub(1) as u16;
        }
    }
}

// ── Ancienne App (rétro-compatibilité — supprimée en Task 11) ────

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Running,
    LogView,
    Done { success: bool },
}

pub struct App {
    pub beams: Vec<BeamView>,
    pub mode: AppMode,
    pub selected: usize,
    pub log_scroll: u16,
}

impl App {
    pub fn new(beam_names: Vec<String>) -> Self {
        App {
            beams: beam_names.into_iter().map(|n| BeamView::new(n, vec![])).collect(),
            mode: AppMode::Running,
            selected: 0,
            log_scroll: 0,
        }
    }

    pub fn apply_event(&mut self, event: SchedulerEvent) {
        match event {
            SchedulerEvent::BeamStarted { name } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = BeamStatus::Running;
                    b.started_at = Some(Instant::now());
                }
            }
            SchedulerEvent::BeamCompleted { name, status } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = status;
                }
            }
            SchedulerEvent::BeamOutput { name, line, is_stderr } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    if is_stderr {
                        b.stderr.push(line);
                    } else {
                        b.stdout.push(line);
                    }
                }
            }
            SchedulerEvent::AllDone { success } => {
                self.mode = AppMode::Done { success };
            }
        }
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.beams.len().saturating_sub(1));
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}
