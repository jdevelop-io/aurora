use aurora_core::events::{BeamStatus, SchedulerEvent, SkipReason, WatchTrigger};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cmp::Reverse;
use std::collections::HashSet;
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
            BeamStatus::FailedAllowed { .. } => "⚠",
            BeamStatus::Cancelled => "⊘",
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

    /// Is the beam displaying logs replayed from the cache rather than a
    /// fresh run? Used to flag that the logs are from the last run.
    pub fn is_cached(&self) -> bool {
        matches!(
            self.status,
            BeamStatus::Skipped {
                reason: SkipReason::Cached
            }
        )
    }

    /// Placeholder text shown when the beam has no output,
    /// adapted to its status.
    pub fn empty_placeholder(&self) -> &'static str {
        match self.status {
            BeamStatus::Pending => "(waiting to start)",
            BeamStatus::Running => "(no output yet)",
            _ => "(no output)",
        }
    }

    /// Iterates over log lines as they are displayed: stdout, then
    /// separator and stderr if present. If there is no output, a single
    /// placeholder line. Single source shared by rendering and search.
    pub fn iter_log_lines(&self) -> impl Iterator<Item = (&str, LogKind)> {
        let has_stderr = !self.stderr.is_empty();
        let is_empty = self.stdout.is_empty() && !has_stderr;
        let stdout = self.stdout.iter().map(|l| (l.as_str(), LogKind::Stdout));
        let sep = has_stderr
            .then_some(("── stderr ──", LogKind::Separator))
            .into_iter();
        let stderr = self.stderr.iter().map(|l| (l.as_str(), LogKind::Stderr));
        let placeholder = is_empty
            .then_some((self.empty_placeholder(), LogKind::Placeholder))
            .into_iter();
        stdout.chain(sep).chain(stderr).chain(placeholder)
    }

    pub fn log_line_count(&self) -> usize {
        self.iter_log_lines().count()
    }

    /// Index of the first visual line (after wrap) corresponding to the
    /// logical line `logical_line`, at width `width`. Used to convert
    /// a logical line index into a visual scroll offset.
    pub fn visual_offset(&self, logical_line: usize, width: u16) -> u16 {
        self.iter_log_lines()
            .take(logical_line)
            .map(|(t, _)| visual_rows(t, width))
            .fold(0u16, |acc, rows| acc.saturating_add(rows))
    }

    /// Total number of visual lines (after wrap) at width `width`.
    pub fn total_visual_rows(&self, width: u16) -> u16 {
        self.iter_log_lines()
            .map(|(t, _)| visual_rows(t, width))
            .fold(0u16, |acc, rows| acc.saturating_add(rows))
    }

    /// Index of the logical line displayed at visual offset `offset`
    /// (the logical line at the top of the screen). Inverse of `visual_offset`.
    pub fn logical_line_at_visual(&self, offset: u16, width: u16) -> usize {
        let mut acc = 0u16;
        for (i, (t, _)) in self.iter_log_lines().enumerate() {
            acc = acc.saturating_add(visual_rows(t, width));
            if acc > offset {
                return i;
            }
        }
        self.log_line_count().saturating_sub(1)
    }
}

/// Strips ANSI escape sequences and control characters from a captured
/// log line. Tools (deptrac, phpcs, ...) emit raw color and cursor-positioning
/// codes: left as-is, ratatui would write them to the terminal which would
/// reinterpret them, corrupting the display (shifted text, leftovers from the
/// previous screen). The carriage return is stripped because it would
/// rewrite the line; the tab is kept.
pub fn sanitize_log_line(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek() {
                // CSI: ESC [ ... final byte in 0x40..=0x7E (e.g. SGR colors).
                Some('[') => {
                    chars.next();
                    for n in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... terminated by BEL (0x07) or ST (ESC \).
                Some(']') => {
                    chars.next();
                    while let Some(n) = chars.next() {
                        if n == '\x07' {
                            break;
                        }
                        if n == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // Other short escape sequence: skip the next byte.
                _ => {
                    chars.next();
                }
            },
            '\r' => {}
            c if c.is_control() && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// Splits a logical line into visual segments of at most `width` characters.
/// Splits by character (deterministic), so the logical index converts
/// exactly into a visual offset. An empty line produces a single empty segment (1 line).
pub fn wrap_log_line(text: &str, width: u16) -> Vec<&str> {
    if width == 0 {
        return vec![text];
    }
    let width = width as usize;
    let bounds: Vec<usize> = text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(text.len()))
        .collect();
    let nchars = bounds.len() - 1;
    if nchars == 0 {
        return vec![""];
    }
    let mut segments = Vec::new();
    let mut i = 0;
    while i < nchars {
        let end = (i + width).min(nchars);
        segments.push(&text[bounds[i]..bounds[end]]);
        i = end;
    }
    segments
}

/// Number of visual lines a logical line occupies at width `width`.
/// Saturates at `u16::MAX` so a pathologically long single line cannot wrap
/// the count.
pub fn visual_rows(text: &str, width: u16) -> u16 {
    wrap_log_line(text, width).len().min(u16::MAX as usize) as u16
}

// ── LogKind ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Stdout,
    Stderr,
    Separator,
    Placeholder,
}

// ── LogSearch ────────────────────────────────────────────────────

/// Incremental search in the logs of the selected beam.
#[derive(Debug, Default)]
pub struct LogSearch {
    pub input_active: bool,
    pub query: String,
    pub matches: Vec<usize>,
    pub current: usize,
}

impl LogSearch {
    pub fn new() -> Self {
        LogSearch::default()
    }

    /// Is there an active query (input in progress or a non-empty confirmed query)?
    pub fn is_active(&self) -> bool {
        self.input_active || !self.query.is_empty()
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Index of the logical line of the current match, or None if there is no match.
    pub fn current_line(&self) -> Option<usize> {
        self.matches.get(self.current).copied()
    }

    /// Recomputes the lines matching the query (case-insensitive).
    /// Only keeps Stdout/Stderr lines; empty query => no match.
    pub fn recompute(&mut self, beam: &BeamView) {
        self.matches.clear();
        self.current = 0;
        if self.query.is_empty() {
            return;
        }
        let needle = self.query.to_lowercase();
        for (idx, (text, kind)) in beam.iter_log_lines().enumerate() {
            if matches!(kind, LogKind::Stdout | LogKind::Stderr)
                && text.to_lowercase().contains(&needle)
            {
                self.matches.push(idx);
            }
        }
    }

    /// Recomputes matches while keeping the current logical line.
    /// Used during execution: new output can produce matches
    /// without resetting the `n`/`N` navigation.
    pub fn recompute_preserving(&mut self, beam: &BeamView) {
        let prev_line = self.current_line();
        self.recompute(beam);
        if let Some(pl) = prev_line {
            if let Some(pos) = self.matches.iter().position(|&l| l == pl) {
                self.current = pos;
            }
        }
    }

    pub fn next(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current = (self.current + 1) % self.matches.len();
    }

    pub fn prev(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current = (self.current + self.matches.len() - 1) % self.matches.len();
    }

    /// Fully resets the search.
    pub fn clear(&mut self) {
        self.input_active = false;
        self.query.clear();
        self.matches.clear();
        self.current = 0;
    }
}

// ── FocusPanel ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FocusPanel {
    Beams,
    Logs,
}

// ── Actions returned by handle_key ────────────────────────────

#[derive(Debug, PartialEq)]
pub enum PickerAction {
    Launch(Vec<String>),
    Quit,
}

#[derive(Debug, PartialEq)]
pub enum ExecutionAction {
    Quit,
    OpenLogView {
        beam_index: usize,
    },
    Rerun {
        root: String,
        pre_success: Vec<String>,
    },
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
    /// Display form with the param signature (`deploy <version> [env=staging]`);
    /// equals `name` for a param-less beam.
    pub signature: String,
    /// True when the beam declares a param without a default: it cannot be
    /// launched from the picker (no value input here), only from the CLI.
    pub requires_args: bool,
}

pub struct PickerState {
    pub beams: Vec<PickerBeam>,
    pub selected: usize,
    pub search: String,
    /// Filter input mode active (`/`). Outside this mode letters are
    /// commands; aligned with the runner's log search.
    pub search_input: bool,
    pub show_deps: bool,
    pub checked: Vec<bool>,
    /// Transient advisory shown in the footer (e.g. a parameterized beam
    /// cannot be launched/checked from here). Cleared on the next keypress.
    pub notice: Option<String>,
}

impl PickerState {
    pub fn new(beams: Vec<PickerBeam>) -> Self {
        let len = beams.len();
        PickerState {
            beams,
            selected: 0,
            search: String::new(),
            search_input: false,
            // Dependency panel visible from the start; `d` collapses it.
            show_deps: true,
            checked: vec![false; len],
            notice: None,
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
        // Best score first. `sort_by_key` is a stable sort, so beams that tie
        // keep their declaration order, exactly as the previous comparator did.
        results.sort_by_key(|(_, _, score)| Reverse(*score));
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
        // Ctrl+C always quits.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(PickerAction::Quit);
        }

        // Any other key clears a previously shown notice: it is a one-frame
        // advisory, not a persistent banner.
        self.notice = None;

        let count = self.filtered().len();

        // Filter input mode (`/`): keystrokes feed the filter. Enter
        // locks it in and exits, Esc clears it and exits. Same model as the
        // runner's log search.
        if self.search_input {
            match key.code {
                KeyCode::Esc => {
                    self.search.clear();
                    self.search_input = false;
                    self.selected = 0;
                }
                KeyCode::Enter => self.search_input = false,
                KeyCode::Backspace => {
                    self.search.pop();
                    self.selected = 0;
                }
                KeyCode::Down => self.select_next(count),
                KeyCode::Up => self.select_prev(count),
                KeyCode::Home => self.selected = 0,
                KeyCode::End => self.selected = count.saturating_sub(1),
                KeyCode::Char(c) => {
                    self.search.push(c);
                    self.selected = 0;
                }
                _ => {}
            }
            return None;
        }

        // Command mode: letters are shortcuts.
        match key.code {
            KeyCode::Char('/') => {
                self.search.clear();
                self.search_input = true;
                self.selected = 0;
            }
            KeyCode::Esc => {
                if self.search.is_empty() {
                    return Some(PickerAction::Quit);
                }
                self.search.clear();
                self.selected = 0;
            }
            KeyCode::Enter => return self.launch(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(count),
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(count),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = count.saturating_sub(1),
            KeyCode::Char(' ') => {
                if let Some(idx) = self.filtered().get(self.selected).map(|(i, _, _)| *i) {
                    if self.beams[idx].requires_args {
                        self.notice = Some(format!(
                            "'{}' requires arguments: run `aurora {}`",
                            self.beams[idx].name, self.beams[idx].signature
                        ));
                    } else {
                        self.checked[idx] = !self.checked[idx];
                    }
                }
            }
            KeyCode::Char('d') => self.show_deps = !self.show_deps,
            _ => {}
        }
        None
    }

    fn select_next(&mut self, count: usize) {
        if count > 0 {
            self.selected = if self.selected + 1 >= count {
                0
            } else {
                self.selected + 1
            };
        }
    }

    fn select_prev(&mut self, count: usize) {
        if count > 0 {
            self.selected = if self.selected == 0 {
                count - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Launch action: the checked beams if any exist, otherwise the beam
    /// selected in the filtered list. A beam that requires arguments cannot be
    /// launched from here (no value input in the picker): a notice is set
    /// instead, pointing at the CLI invocation.
    fn launch(&mut self) -> Option<PickerAction> {
        let checked = self.selected_beam_indices();
        if !checked.is_empty() {
            let names = checked
                .iter()
                .map(|&i| self.beams[i].name.clone())
                .collect();
            return Some(PickerAction::Launch(names));
        }
        let beam = {
            let filtered = self.filtered();
            let (_, b, _) = filtered.get(self.selected)?;
            (*b).clone()
        };
        if beam.requires_args {
            self.notice = Some(format!(
                "'{}' requires arguments: run `aurora {}`",
                beam.name, beam.signature
            ));
            return None;
        }
        Some(PickerAction::Launch(vec![beam.name]))
    }
}

// ── WatchUiState ─────────────────────────────────────────────────

/// The execution TUI's watch state (the part that is pure and testable). The
/// live `notify` watcher and its trigger receiver are held by the render loop,
/// not here. `pending` holds a trigger that arrived while a run was still in
/// progress: it is applied at `AllDone`, implementing "finish, then re-run".
#[derive(Debug, Default)]
pub struct WatchUiState {
    pub armed: bool,
    pub pending: Option<WatchTrigger>,
}

impl WatchUiState {
    pub fn arm(&mut self) {
        self.armed = true;
    }

    /// Disarms and discards any pending trigger: turning the watch off must not
    /// leave a queued re-run that would fire later.
    pub fn disarm(&mut self) {
        self.armed = false;
        self.pending = None;
    }

    /// Records a trigger. Returns `true` when it can be applied immediately (no
    /// run in progress); otherwise it is held as `pending` for `AllDone` and
    /// `false` is returned.
    pub fn on_trigger(&mut self, trigger: WatchTrigger, run_in_progress: bool) -> bool {
        if run_in_progress {
            self.pending = Some(trigger);
            false
        } else {
            true
        }
    }

    /// Consumes and returns the pending trigger, if any.
    pub fn take_pending(&mut self) -> Option<WatchTrigger> {
        self.pending.take()
    }
}

// ── ExecutionState ───────────────────────────────────────────────

pub struct ExecutionState {
    pub beams: Vec<BeamView>,
    pub selected: usize,
    pub done: Option<bool>,
    pub focus: FocusPanel,
    pub show_deps: bool,
    /// Filter for the beam list (typed via `/` when focus is on the
    /// beams). Empty = all beams visible.
    pub beam_filter: String,
    /// Beam filter input mode active.
    pub filter_input: bool,
    /// Names of the beams the current run actually executes: the transitive
    /// closure of the launched target. The progress count and the status
    /// breakdown are scoped to this set, and the sidebar dims the rest as
    /// available-but-idle. Defaults to every beam so an unscoped state counts
    /// the whole list.
    run_set: HashSet<String>,
}

impl ExecutionState {
    pub fn new(beam_info: Vec<(String, Vec<String>)>) -> Self {
        let run_set = beam_info.iter().map(|(name, _)| name.clone()).collect();
        ExecutionState {
            beams: beam_info
                .into_iter()
                .map(|(name, deps)| BeamView::new(name, deps))
                .collect(),
            selected: 0,
            done: None,
            focus: FocusPanel::Beams,
            show_deps: false,
            beam_filter: String::new(),
            filter_input: false,
            run_set,
        }
    }

    /// Whether `name` belongs to the current run (see `run_set`). Beams outside
    /// it are shown in the sidebar but dimmed and left out of the progress
    /// count.
    pub fn is_in_run(&self, name: &str) -> bool {
        self.run_set.contains(name)
    }

    /// Number of beams in the current run: the status bar's denominator.
    pub fn run_total(&self) -> usize {
        self.beams
            .iter()
            .filter(|b| self.run_set.contains(&b.name))
            .count()
    }

    /// Replaces the run scope with an explicit set of beam names. Used at launch
    /// with the closure computed by the composition root, which also covers the
    /// virtual multi-beam selection the TUI cannot resolve on its own.
    pub fn set_run_set(&mut self, names: impl IntoIterator<Item = String>) {
        self.run_set = names.into_iter().collect();
    }

    /// Scopes the run to `root`'s transitive closure over `depends_on`, computed
    /// from the beams already known to the view. Used when a rerun (or a watch
    /// reload) makes `root` the target actually being executed.
    pub fn focus_run_on(&mut self, root: &str) {
        self.run_set = self.closure_of(root);
    }

    /// Transitive closure of `root` (root plus its transitive dependencies) over
    /// the view's own `depends_on` edges. An unknown `root` yields just itself.
    fn closure_of(&self, root: &str) -> HashSet<String> {
        let index: std::collections::HashMap<&str, usize> = self
            .beams
            .iter()
            .enumerate()
            .map(|(i, b)| (b.name.as_str(), i))
            .collect();
        let mut seen: HashSet<String> = HashSet::new();
        let mut stack = vec![root.to_string()];
        while let Some(name) = stack.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(&i) = index.get(name.as_str()) {
                for dep in &self.beams[i].depends_on {
                    if !seen.contains(dep) {
                        stack.push(dep.clone());
                    }
                }
            }
        }
        seen
    }

    /// Every beam's name in declaration order. Used by a watch re-run, which
    /// resets all beams so the cache re-checks each one (no past success is
    /// assumed on an inputs change).
    pub fn all_beam_names(&self) -> Vec<String> {
        self.beams.iter().map(|b| b.name.clone()).collect()
    }

    /// Indices of the beams matching the current filter, in execution
    /// order (no reordering: the list stays stable). All beams if the
    /// filter is empty.
    pub fn visible_indices(&self) -> Vec<usize> {
        if self.beam_filter.is_empty() {
            return (0..self.beams.len()).collect();
        }
        use crate::picker::fuzzy::fuzzy_score;
        self.beams
            .iter()
            .enumerate()
            .filter(|(_, b)| fuzzy_score(&self.beam_filter, &b.name, None) > 0)
            .map(|(i, _)| i)
            .collect()
    }

    /// Recenters the selection on the first visible beam if the current
    /// selection is hidden by the filter. Call after every filter edit.
    pub fn clamp_selection_to_visible(&mut self) {
        let visible = self.visible_indices();
        if !visible.contains(&self.selected) {
            self.selected = visible.first().copied().unwrap_or(0);
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
            SchedulerEvent::BeamOutput {
                name,
                line,
                is_stderr,
            } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    let line = sanitize_log_line(&line);
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

    /// Computes the beams to rerun starting from the selected beam.
    /// Returns (root_name, to_rerun, pre_success).
    pub fn compute_rerun(&self, selected: usize) -> (String, Vec<String>, Vec<String>) {
        let root_name = self.beams[selected].name.clone();
        let mut to_rerun = vec![];
        let mut pre_success = vec![];

        let mut stack = vec![selected];
        let mut visited: HashSet<usize> = HashSet::new();

        while let Some(idx) = stack.pop() {
            if !visited.insert(idx) {
                continue;
            }
            let beam = &self.beams[idx];
            if idx == selected {
                // The root beam is always rerun, regardless of its status
                to_rerun.push(beam.name.clone());
            } else {
                match &beam.status {
                    BeamStatus::Failed { .. } | BeamStatus::Cancelled => {
                        to_rerun.push(beam.name.clone());
                    }
                    BeamStatus::Success { .. }
                    | BeamStatus::Skipped { .. }
                    | BeamStatus::FailedAllowed { .. } => {
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

    /// Resets the listed beams to Pending and clears their logs. Also resets exec.done.
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
            KeyCode::Left => {
                self.focus = FocusPanel::Beams;
                None
            }
            KeyCode::Right => {
                self.focus = FocusPanel::Logs;
                None
            }
            KeyCode::Char('d') => {
                self.show_deps = !self.show_deps;
                None
            }
            KeyCode::Char('q') => Some(ExecutionAction::Quit),
            KeyCode::Enter => Some(ExecutionAction::OpenLogView {
                beam_index: self.selected,
            }),
            KeyCode::Char('r') => {
                if self.done.is_some() {
                    let beam = &self.beams[self.selected];
                    if matches!(
                        beam.status,
                        BeamStatus::Failed { .. }
                            | BeamStatus::FailedAllowed { .. }
                            | BeamStatus::Cancelled
                            | BeamStatus::Success { .. }
                            | BeamStatus::Skipped { .. }
                    ) {
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

    /// Selects the first beam with `Failed` status. Returns `true` if found,
    /// does nothing otherwise (e.g. failure caused only by `Cancelled` beams).
    pub fn select_first_failed(&mut self) -> bool {
        if let Some(idx) = self
            .beams
            .iter()
            .position(|b| matches!(b.status, BeamStatus::Failed { .. }))
        {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    pub fn select_next(&mut self) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return;
        }
        let pos = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let next = if pos + 1 >= visible.len() { 0 } else { pos + 1 };
        self.selected = visible[next];
    }

    pub fn select_prev(&mut self) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return;
        }
        let pos = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let prev = if pos == 0 { visible.len() - 1 } else { pos - 1 };
        self.selected = visible[prev];
    }

    pub fn select_first(&mut self) {
        if let Some(&first) = self.visible_indices().first() {
            self.selected = first;
        }
    }

    pub fn select_last(&mut self) {
        if let Some(&last) = self.visible_indices().last() {
            self.selected = last;
        }
    }
}

// ── LogViewState ─────────────────────────────────────────────────

pub struct LogViewState {
    pub beam_index: usize,
    pub scroll: u16,
    pub scroll_locked: bool, // true = the user has scrolled manually
}

impl LogViewState {
    pub fn new(beam_index: usize) -> Self {
        LogViewState {
            beam_index,
            scroll: 0,
            scroll_locked: false,
        }
    }

    /// Maximum scroll offset (in visual lines): we do not scroll
    /// past the last full screen.
    fn max_scroll(total_visual: u16, height: u16) -> u16 {
        total_visual.saturating_sub(height)
    }

    /// Moves the scroll by `delta` visual lines, clamped to [0, max_scroll].
    /// Scrolling up locks auto-scroll; reaching the bottom re-enables it.
    pub fn scroll_lines(&mut self, delta: i32, total_visual: u16, height: u16) {
        let max = Self::max_scroll(total_visual, height) as i32;
        let next = (self.scroll as i32 + delta).clamp(0, max);
        self.scroll = next as u16;
        if delta < 0 {
            self.scroll_locked = true;
        } else if next >= max {
            self.scroll_locked = false;
        }
    }

    /// Goes to the top of the logs and locks auto-scroll.
    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
        self.scroll_locked = true;
    }

    /// Goes to the bottom of the logs and re-enables auto-scroll.
    pub fn scroll_to_bottom(&mut self, total_visual: u16, height: u16) {
        self.scroll = Self::max_scroll(total_visual, height);
        self.scroll_locked = false;
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        total_visual: u16,
        height: u16,
    ) -> Option<LogViewAction> {
        let page = height.saturating_sub(2).max(1) as i32;
        match key.code {
            KeyCode::Esc => return Some(LogViewAction::Close),
            KeyCode::Char('q') => return Some(LogViewAction::Quit),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_lines(-1, total_visual, height),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_lines(1, total_visual, height),
            KeyCode::Char('g') => self.scroll_to_top(),
            KeyCode::Char('G') => self.scroll_to_bottom(total_visual, height),
            KeyCode::PageUp => self.scroll_lines(-page, total_visual, height),
            KeyCode::PageDown => self.scroll_lines(page, total_visual, height),
            _ => {}
        }
        None
    }

    /// Called on every tick: if auto-scroll is active, stick to the bottom.
    pub fn auto_scroll(&mut self, total_visual: u16, height: u16) {
        if !self.scroll_locked {
            self.scroll = Self::max_scroll(total_visual, height);
        }
    }
}

#[cfg(test)]
mod watch_state_tests {
    use super::*;
    use aurora_core::events::WatchTrigger;

    #[test]
    fn arm_then_disarm_clears_pending() {
        let mut w = WatchUiState::default();
        assert!(!w.armed);
        w.arm();
        assert!(w.armed);
        // A trigger seen during a run is held pending.
        assert!(!w.on_trigger(
            WatchTrigger {
                beamfile_changed: false
            },
            true
        ));
        assert!(w.pending.is_some());
        // Disarming discards the pending trigger.
        w.disarm();
        assert!(!w.armed);
        assert!(w.pending.is_none());
    }

    #[test]
    fn trigger_when_idle_applies_immediately() {
        let mut w = WatchUiState::default();
        w.arm();
        // run_in_progress = false -> apply now, nothing held.
        assert!(w.on_trigger(
            WatchTrigger {
                beamfile_changed: true
            },
            false
        ));
        assert!(w.pending.is_none());
    }

    #[test]
    fn take_pending_consumes_the_held_trigger() {
        let mut w = WatchUiState::default();
        w.arm();
        w.on_trigger(
            WatchTrigger {
                beamfile_changed: true,
            },
            true,
        );
        assert_eq!(
            w.take_pending(),
            Some(WatchTrigger {
                beamfile_changed: true
            })
        );
        assert_eq!(w.take_pending(), None, "consumed once");
    }

    #[test]
    fn all_beam_names_lists_every_beam() {
        let exec = ExecutionState::new(vec![
            ("build".to_string(), vec!["lint".to_string()]),
            ("lint".to_string(), vec![]),
        ]);
        assert_eq!(
            exec.all_beam_names(),
            vec!["build".to_string(), "lint".to_string()]
        );
    }
}
