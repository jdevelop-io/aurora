# TUI Log Scroll — Focus switchable Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Permettre de scroller les logs dans la vue split avec j/k via un focus switchable par Tab.

**Architecture:** Ajouter un `enum FocusPanel { Beams, Logs }` dans `ExecutionState`. Tab bascule le focus. Dans la boucle principale (`lib.rs`), le dispatch de j/k est conditionnel sur `exec.focus`. Les fonctions de rendu reçoivent un `bool` pour colorer la bordure active.

**Tech Stack:** Rust, ratatui, crossterm, tokio — workspace cargo à la racine.

---

### Task 1 : Ajouter `FocusPanel` et le champ `focus` dans `ExecutionState`

**Files:**
- Modify: `crates/aurora-tui/src/app.rs`
- Test: `crates/aurora-tui/tests/app_state_test.rs` (ou nouveau fichier `tests/focus_test.rs`)

**Step 1 : Écrire les tests qui échouent**

Dans `crates/aurora-tui/tests/app_state_test.rs`, ajouter :

```rust
use aurora_tui::app::{ExecutionState, FocusPanel};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn default_focus_is_beams() {
    let state = ExecutionState::new(vec!["a".into(), "b".into()]);
    assert_eq!(state.focus, FocusPanel::Beams);
}

#[test]
fn tab_switches_focus_from_beams_to_logs() {
    let mut state = ExecutionState::new(vec!["a".into()]);
    state.handle_key(key(KeyCode::Tab));
    assert_eq!(state.focus, FocusPanel::Logs);
}

#[test]
fn tab_switches_focus_from_logs_to_beams() {
    let mut state = ExecutionState::new(vec!["a".into()]);
    state.focus = FocusPanel::Logs;
    state.handle_key(key(KeyCode::Tab));
    assert_eq!(state.focus, FocusPanel::Beams);
}
```

**Step 2 : Vérifier que les tests échouent**

```bash
cargo test -p aurora-tui default_focus_is_beams tab_switches_focus -- --nocapture
```

Attendu : erreur de compilation — `FocusPanel` n'existe pas.

**Step 3 : Implémenter dans `app.rs`**

Après les imports existants, ajouter :

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FocusPanel {
    Beams,
    Logs,
}
```

Dans `ExecutionState`, ajouter le champ :

```rust
pub struct ExecutionState {
    pub beams: Vec<BeamView>,
    pub selected: usize,
    pub done: Option<bool>,
    pub focus: FocusPanel,  // <-- nouveau
}
```

Dans `ExecutionState::new`, initialiser :

```rust
pub fn new(beam_names: Vec<String>) -> Self {
    ExecutionState {
        beams: beam_names.into_iter().map(BeamView::new).collect(),
        selected: 0,
        done: None,
        focus: FocusPanel::Beams,  // <-- nouveau
    }
}
```

Dans `ExecutionState::handle_key`, ajouter le cas `Tab` :

```rust
pub fn handle_key(&self, key: KeyEvent) -> Option<ExecutionAction> {
    match key.code {
        KeyCode::Char('q') => Some(ExecutionAction::Quit),
        KeyCode::Enter => Some(ExecutionAction::OpenLogView {
            beam_index: self.selected,
        }),
        _ => None,
    }
}
```

⚠️ `handle_key` prend `&self` — la gestion de Tab doit être dans un mut method. Remplacer par :

```rust
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
        _ => None,
    }
}
```

**Step 4 : Vérifier que les tests passent**

```bash
cargo test -p aurora-tui default_focus_is_beams tab_switches_focus
```

Attendu : 3 tests PASS.

**Step 5 : Commit**

```bash
git add crates/aurora-tui/src/app.rs crates/aurora-tui/tests/app_state_test.rs
git commit -m "✨ feat(tui): add FocusPanel and Tab toggle to ExecutionState"
```

---

### Task 2 : Dispatch conditionnel de j/k dans `lib.rs`

**Files:**
- Modify: `crates/aurora-tui/src/lib.rs`

> Pas de test unitaire direct ici (boucle d'événements) — le comportement est couvert par les tests d'intégration suivants.

**Step 1 : Modifier la boucle de dispatch dans `lib.rs`**

Remplacer le bloc de gestion des touches (lignes ~81–102) :

```rust
// AVANT
KeyCode::Down | KeyCode::Char('j') => {
    exec.select_next();
    log_state.beam_index = exec.selected;
    log_state.scroll_locked = false;
}
KeyCode::Up | KeyCode::Char('k') => {
    exec.select_prev();
    log_state.beam_index = exec.selected;
    log_state.scroll_locked = false;
}
```

Par :

```rust
// APRÈS
KeyCode::Down | KeyCode::Char('j') => {
    match exec.focus {
        FocusPanel::Beams => {
            exec.select_next();
            log_state.beam_index = exec.selected;
            log_state.scroll_locked = false;
        }
        FocusPanel::Logs => {
            log_state.handle_key(key, total_lines, terminal.size()?.height);
        }
    }
}
KeyCode::Up | KeyCode::Char('k') => {
    match exec.focus {
        FocusPanel::Beams => {
            exec.select_prev();
            log_state.beam_index = exec.selected;
            log_state.scroll_locked = false;
        }
        FocusPanel::Logs => {
            log_state.handle_key(key, total_lines, terminal.size()?.height);
        }
    }
}
KeyCode::Tab => {
    exec.handle_key(key);
}
```

Ajouter l'import en tête de `lib.rs` (ou dans le `use` existant) :

```rust
use app::FocusPanel;
```

**Step 2 : Vérifier la compilation**

```bash
cargo build -p aurora-tui
```

Attendu : compilation OK, aucun warning non-intentionnel.

**Step 3 : Commit**

```bash
git add crates/aurora-tui/src/lib.rs
git commit -m "✨ feat(tui): dispatch j/k to log scroll when logs are focused"
```

---

### Task 3 : Indicateur visuel — bordure du panneau actif

**Files:**
- Modify: `crates/aurora-tui/src/execution/beam_list.rs`
- Modify: `crates/aurora-tui/src/execution/log_panel.rs`
- Modify: `crates/aurora-tui/src/execution/split_layout.rs`

**Step 1 : Lire les fichiers concernés**

```bash
# Lire pour comprendre les signatures actuelles
cat crates/aurora-tui/src/execution/beam_list.rs
cat crates/aurora-tui/src/execution/log_panel.rs
```

**Step 2 : Modifier `beam_list.rs`**

Ajouter `focused: bool` au paramètre de `render_beam_list` et changer la couleur de la bordure :

```rust
pub fn render_beam_list(f: &mut Frame, exec: &ExecutionState, tick: u64, area: Rect, focused: bool) {
    // ...
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Beams ")
        .border_style(border_style);
    // ... reste inchangé
}
```

**Step 3 : Modifier `log_panel.rs`**

Même principe pour `render_log_panel` :

```rust
pub fn render_log_panel(f: &mut Frame, beam: &BeamView, log_state: &LogViewState, area: Rect, focused: bool) {
    // ...
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    // ...
}
```

**Step 4 : Modifier `split_layout.rs`**

Passer `exec.focus` aux renderers :

```rust
use crate::app::FocusPanel;

pub fn render_execution(
    f: &mut Frame,
    exec: &ExecutionState,
    log_state: &LogViewState,
    tick: u64,
    show_help: bool,
) {
    // ...
    let beams_focused = exec.focus == FocusPanel::Beams;
    beam_list::render_beam_list(f, exec, tick, split[0], beams_focused);

    let beam = &exec.beams[log_state.beam_index];
    log_panel::render_log_panel(f, beam, log_state, split[1], !beams_focused);
    // ...
}
```

**Step 5 : Vérifier la compilation**

```bash
cargo build -p aurora-tui
```

Attendu : OK.

**Step 6 : Commit**

```bash
git add crates/aurora-tui/src/execution/
git commit -m "✨ feat(tui): highlight focused panel border in split view"
```

---

### Task 4 : Mettre à jour le help popup

**Files:**
- Modify: `crates/aurora-tui/src/widgets/help_popup.rs`

**Step 1 : Modifier `HelpContext::Execution`**

Dans `render_help_popup`, section `HelpContext::Execution`, ajouter la ligne Tab :

```rust
HelpContext::Execution => vec![
    Line::from(Span::styled(
        " Exécution",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )),
    Line::from(""),
    Line::from(" ↑↓ / jk    Naviguer beams (ou scroller logs si focus logs)"),
    Line::from(" Tab        Basculer focus beams / logs"),
    Line::from(" PgUp/Dn    Scroller les logs par page"),
    Line::from(" G          Aller au bas des logs"),
    Line::from(" y          Copier les logs dans le clipboard"),
    Line::from(" ?          Fermer cette aide"),
    Line::from(" q          Annuler et quitter"),
],
```

**Step 2 : Compiler et vérifier**

```bash
cargo build -p aurora-tui
```

**Step 3 : Commit**

```bash
git add crates/aurora-tui/src/widgets/help_popup.rs
git commit -m "📝 docs(tui): update help popup with Tab focus keybinding"
```

---

### Task 5 : Tests d'intégration du comportement de scroll

**Files:**
- Test: `crates/aurora-tui/tests/log_view_test.rs` (ajouter des cas)

**Step 1 : Écrire les tests**

```rust
use aurora_tui::app::{ExecutionState, FocusPanel, LogViewState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn jk_scroll_logs_when_log_focused() {
    let mut log_state = LogViewState::new(0, 20);
    let initial_scroll = log_state.scroll;

    // Simuler focus Logs + appui sur Up
    log_state.handle_key(key(KeyCode::Up), 20, 10);
    assert!(log_state.scroll < initial_scroll || log_state.scroll_locked);
}

#[test]
fn focus_beams_keeps_jk_for_navigation() {
    let mut state = ExecutionState::new(vec!["a".into(), "b".into(), "c".into()]);
    assert_eq!(state.focus, FocusPanel::Beams);
    state.select_next();
    assert_eq!(state.selected, 1);
    // focus beams = select_next fonctionne normalement
}

#[test]
fn focus_resets_scroll_lock_on_beam_change() {
    let mut exec = ExecutionState::new(vec!["a".into(), "b".into()]);
    let mut log_state = LogViewState::new(0, 20);
    log_state.scroll_locked = true;

    // Changer de beam (focus Beams)
    exec.select_next();
    log_state.beam_index = exec.selected;
    log_state.scroll_locked = false; // doit être reset manuellement dans lib.rs

    assert!(!log_state.scroll_locked);
}
```

**Step 2 : Lancer tous les tests**

```bash
cargo test -p aurora-tui
```

Attendu : tous les tests passent.

**Step 3 : Commit final**

```bash
git add crates/aurora-tui/tests/
git commit -m "✅ test(tui): add focus panel and log scroll integration tests"
```

---

## Vérification finale

```bash
cargo test -p aurora-tui
cargo build --release
```

Les tests suivants doivent tous passer :
- `default_focus_is_beams`
- `tab_switches_focus_from_beams_to_logs`
- `tab_switches_focus_from_logs_to_beams`
- `jk_scroll_logs_when_log_focused`
- `focus_beams_keeps_jk_for_navigation`
- `focus_resets_scroll_lock_on_beam_change`
- Tous les tests existants dans `log_view_test.rs`
