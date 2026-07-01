use aurora_core::scheduler::{BeamStatus, SchedulerEvent, SkipReason};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

    /// Le beam affiche-t-il des logs rejoués depuis le cache plutôt que d'une
    /// exécution fraîche ? Sert à signaler que les logs datent du dernier run.
    pub fn is_cached(&self) -> bool {
        matches!(
            self.status,
            BeamStatus::Skipped {
                reason: SkipReason::Cached
            }
        )
    }

    /// Texte du placeholder affiché quand le beam n'a aucune sortie,
    /// adapté à son statut.
    pub fn empty_placeholder(&self) -> &'static str {
        match self.status {
            BeamStatus::Pending => "(en attente de démarrage)",
            BeamStatus::Running => "(pas encore de sortie)",
            _ => "(aucune sortie)",
        }
    }

    /// Itère les lignes de logs telles qu'elles sont affichées : stdout, puis
    /// séparateur et stderr si présent. Si aucune sortie, une unique ligne
    /// placeholder. Source unique partagée par le rendu et la recherche.
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

    /// Index de la première ligne visuelle (après wrap) correspondant à la
    /// ligne logique `logical_line`, à la largeur `width`. Permet de convertir
    /// un index de ligne logique en offset de scroll visuel.
    pub fn visual_offset(&self, logical_line: usize, width: u16) -> u16 {
        self.iter_log_lines()
            .take(logical_line)
            .map(|(t, _)| visual_rows(t, width))
            .sum()
    }

    /// Nombre total de lignes visuelles (après wrap) à la largeur `width`.
    pub fn total_visual_rows(&self, width: u16) -> u16 {
        self.iter_log_lines()
            .map(|(t, _)| visual_rows(t, width))
            .sum()
    }

    /// Index de la ligne logique affichée à l'offset visuel `offset`
    /// (la ligne logique en haut de l'écran). Inverse de `visual_offset`.
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

/// Retire les séquences d'échappement ANSI et les caractères de contrôle d'une
/// ligne de log capturée. Les outils (deptrac, phpcs, ...) émettent des codes
/// couleur et de positionnement bruts : laissés tels quels, ratatui les écrirait
/// dans le terminal qui les réinterpréterait, corrompant l'affichage (texte
/// décalé, restes de l'écran précédent). Le retour chariot est retiré car il
/// réécrirait la ligne ; la tabulation est conservée.
pub fn sanitize_log_line(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek() {
                // CSI : ESC [ ... octet final dans 0x40..=0x7E (ex. couleurs SGR).
                Some('[') => {
                    chars.next();
                    for n in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC : ESC ] ... terminé par BEL (0x07) ou ST (ESC \).
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
                // Autre séquence d'échappement courte : on saute l'octet suivant.
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

/// Découpe une ligne logique en segments visuels d'au plus `width` caractères.
/// Découpe par caractères (déterministe), pour que l'index logique se convertisse
/// exactement en offset visuel. Une ligne vide produit un segment vide (1 ligne).
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

/// Nombre de lignes visuelles qu'occupe une ligne logique à la largeur `width`.
pub fn visual_rows(text: &str, width: u16) -> u16 {
    wrap_log_line(text, width).len() as u16
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

/// Recherche incrémentale dans les logs du beam sélectionné.
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

    /// Y a-t-il une requête active (saisie en cours ou validée non vide) ?
    pub fn is_active(&self) -> bool {
        self.input_active || !self.query.is_empty()
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Index de la ligne logique du match courant, ou None si aucun match.
    pub fn current_line(&self) -> Option<usize> {
        self.matches.get(self.current).copied()
    }

    /// Recalcule les lignes correspondant à la requête (insensible à la casse).
    /// Ne retient que les lignes Stdout/Stderr ; requête vide => aucun match.
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

    /// Recalcule les correspondances en conservant la ligne logique courante.
    /// Utilisé pendant l'exécution : les nouvelles sorties peuvent faire
    /// apparaître des correspondances sans réinitialiser la navigation `n`/`N`.
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

    /// Réinitialise complètement la recherche.
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

// ── Actions retournées par handle_key ────────────────────────────

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
}

pub struct PickerState {
    pub beams: Vec<PickerBeam>,
    pub selected: usize,
    pub search: String,
    /// Mode saisie du filtre actif (`/`). Hors de ce mode les lettres sont des
    /// commandes ; aligné sur la recherche de logs du runner.
    pub search_input: bool,
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
            search_input: false,
            // Panneau des dépendances visible d'emblée ; `d` le replie.
            show_deps: true,
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
        // Ctrl+C quitte en toutes circonstances.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(PickerAction::Quit);
        }

        let count = self.filtered().len();

        // Mode saisie du filtre (`/`) : la frappe alimente le filtre. Entrée
        // verrouille et sort, Échap efface et sort. Même modèle que la recherche
        // de logs du runner.
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

        // Mode commande : les lettres sont des raccourcis.
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
                    self.checked[idx] = !self.checked[idx];
                }
            }
            KeyCode::Char('d') => self.show_deps = !self.show_deps,
            _ => {}
        }
        None
    }

    fn select_next(&mut self, count: usize) {
        if count > 0 {
            self.selected = if self.selected + 1 >= count { 0 } else { self.selected + 1 };
        }
    }

    fn select_prev(&mut self, count: usize) {
        if count > 0 {
            self.selected = if self.selected == 0 { count - 1 } else { self.selected - 1 };
        }
    }

    /// Action de lancement : les beams cochés s'ils existent, sinon le beam
    /// sélectionné dans la liste filtrée.
    fn launch(&self) -> Option<PickerAction> {
        let checked = self.selected_beam_indices();
        if !checked.is_empty() {
            let names = checked.iter().map(|&i| self.beams[i].name.clone()).collect();
            return Some(PickerAction::Launch(names));
        }
        self.filtered()
            .get(self.selected)
            .map(|(_, b, _)| PickerAction::Launch(vec![b.name.clone()]))
    }
}

// ── ExecutionState ───────────────────────────────────────────────

pub struct ExecutionState {
    pub beams: Vec<BeamView>,
    pub selected: usize,
    pub done: Option<bool>,
    pub focus: FocusPanel,
    pub show_deps: bool,
    /// Filtre de la liste de beams (saisi via `/` quand le focus est sur les
    /// beams). Vide = tous les beams visibles.
    pub beam_filter: String,
    /// Mode saisie du filtre de beams actif.
    pub filter_input: bool,
}

impl ExecutionState {
    pub fn new(beam_info: Vec<(String, Vec<String>)>) -> Self {
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
        }
    }

    /// Indices des beams correspondant au filtre courant, dans l'ordre
    /// d'exécution (pas de réordonnancement : la liste reste stable). Tous les
    /// beams si le filtre est vide.
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

    /// Recentre la sélection sur le premier beam visible si la sélection
    /// courante est masquée par le filtre. À appeler après chaque édition du
    /// filtre.
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

    /// Sélectionne le premier beam au statut `Failed`. Renvoie `true` si trouvé,
    /// ne modifie rien sinon (ex. échec dû uniquement à des `Cancelled`).
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
        let pos = visible.iter().position(|&i| i == self.selected).unwrap_or(0);
        let next = if pos + 1 >= visible.len() { 0 } else { pos + 1 };
        self.selected = visible[next];
    }

    pub fn select_prev(&mut self) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return;
        }
        let pos = visible.iter().position(|&i| i == self.selected).unwrap_or(0);
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
    pub scroll_locked: bool, // true = l'utilisateur a scrollé manuellement
}

impl LogViewState {
    pub fn new(beam_index: usize) -> Self {
        LogViewState {
            beam_index,
            scroll: 0,
            scroll_locked: false,
        }
    }

    /// Offset maximal de scroll (en lignes visuelles) : on ne scrolle pas
    /// au-delà du dernier écran complet.
    fn max_scroll(total_visual: u16, height: u16) -> u16 {
        total_visual.saturating_sub(height)
    }

    /// Déplace le scroll de `delta` lignes visuelles, borné à [0, max_scroll].
    /// Une montée verrouille l'auto-scroll ; atteindre le bas le réactive.
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

    /// Va en haut des logs et verrouille l'auto-scroll.
    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
        self.scroll_locked = true;
    }

    /// Va en bas des logs et réactive l'auto-scroll.
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

    /// Appelé à chaque tick : si auto-scroll actif, coller au bas.
    pub fn auto_scroll(&mut self, total_visual: u16, height: u16) {
        if !self.scroll_locked {
            self.scroll = Self::max_scroll(total_visual, height);
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
            beams: beam_names
                .into_iter()
                .map(|n| BeamView::new(n, vec![]))
                .collect(),
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
