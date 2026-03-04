# TUI Progress Bar Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Afficher une barre de progression ASCII dans la status bar, remplaçant le `[X/Y]` dans le titre du panneau beams.

**Architecture:** `StatusContext::Execution` reçoit `done_count` et `total`. `render_status_bar` génère dynamiquement `[████░░░░] 5/8` avant les hints. `split_layout.rs` calcule les valeurs depuis `exec.beams`. `beam_list.rs` retire le compteur du titre.

**Tech Stack:** Rust, ratatui — workspace cargo à `/home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora`.

---

### Task 1 : Mettre à jour `StatusContext::Execution` et générer la barre

**Files:**
- Modify: `crates/aurora-tui/src/widgets/status_bar.rs`

#### État actuel complet de `status_bar.rs`

```rust
use ratatui::{layout::Rect, style::{Color, Style}, widgets::Paragraph, Frame};

pub enum StatusContext {
    Picker,
    Execution { done: bool },
    LogView,
}

pub fn render_status_bar(f: &mut Frame, area: Rect, ctx: StatusContext) {
    let help = match ctx {
        StatusContext::Picker => {
            " [↑↓] nav  [Space] sélec  [Tab] deps  [Enter] lancer  [?] aide  [q] quitter "
        }
        StatusContext::Execution { done: false } => {
            " [↑↓/jk] beam  [PgUp/Dn] scroll  [G] bas  [y] copier  [?] aide  [q] annuler "
        }
        StatusContext::Execution { done: true } => {
            " [↑↓/jk] beam  [r] re-run  [y] copier  [?] aide  [q] quitter "
        }
        StatusContext::LogView => {
            " [Esc] retour  [↑↓/PgUp/Dn] scroll  [G] bas  [y] copier  [q] quitter "
        }
    };
    let bar = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    f.render_widget(bar, area);
}
```

**Step 1 : Écrire le test qui échoue**

Créer `crates/aurora-tui/tests/status_bar_test.rs` :

```rust
use aurora_tui::widgets::status_bar::{build_progress_bar, StatusContext};

#[test]
fn progress_bar_empty_when_no_beams_done() {
    let bar = build_progress_bar(0, 8);
    assert_eq!(bar, "[░░░░░░░░░░░░░░░░] 0/8");
}

#[test]
fn progress_bar_half_done() {
    let bar = build_progress_bar(4, 8);
    assert_eq!(bar, "[████████░░░░░░░░] 4/8");
}

#[test]
fn progress_bar_fully_done() {
    let bar = build_progress_bar(8, 8);
    assert_eq!(bar, "[████████████████] 8/8");
}

#[test]
fn progress_bar_zero_total_returns_empty() {
    let bar = build_progress_bar(0, 0);
    assert_eq!(bar, "");
}
```

**Step 2 : Vérifier que le test échoue**

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo test -p aurora-tui progress_bar 2>&1 | head -20
```

Attendu : erreur de compilation — `build_progress_bar` n'existe pas.

**Step 3 : Implémenter**

Remplacer `status_bar.rs` entièrement par :

```rust
use ratatui::{layout::Rect, style::{Color, Style}, widgets::Paragraph, Frame};

const BAR_WIDTH: usize = 16;

pub enum StatusContext {
    Picker,
    Execution { done: bool, done_count: usize, total: usize },
    LogView,
}

/// Génère une barre ASCII de progression. Retourne "" si total == 0.
pub fn build_progress_bar(done_count: usize, total: usize) -> String {
    if total == 0 {
        return String::new();
    }
    let filled = (done_count * BAR_WIDTH) / total;
    let empty = BAR_WIDTH - filled;
    format!(
        "[{}{}] {}/{}",
        "█".repeat(filled),
        "░".repeat(empty),
        done_count,
        total
    )
}

pub fn render_status_bar(f: &mut Frame, area: Rect, ctx: StatusContext) {
    let (text, color) = match ctx {
        StatusContext::Picker => (
            " [↑↓] nav  [Space] sélec  [Tab] deps  [Enter] lancer  [?] aide  [q] quitter ".to_string(),
            Color::DarkGray,
        ),
        StatusContext::Execution { done: false, done_count, total } => {
            let bar = build_progress_bar(done_count, total);
            let prefix = if bar.is_empty() { String::new() } else { format!(" {} ", bar) };
            (
                format!("{}[↑↓/jk] beam  [Tab] focus  [PgUp/Dn] scroll  [G] bas  [y] copier  [?] aide  [q] annuler ", prefix),
                Color::DarkGray,
            )
        }
        StatusContext::Execution { done: true, done_count, total } => {
            let bar = build_progress_bar(done_count, total);
            let prefix = if bar.is_empty() { String::new() } else { format!(" {} ", bar) };
            let success = done_count == total;
            (
                format!("{}[↑↓/jk] beam  [y] copier  [?] aide  [q] quitter ", prefix),
                if success { Color::Green } else { Color::Red },
            )
        }
        StatusContext::LogView => (
            " [Esc] retour  [↑↓/PgUp/Dn] scroll  [G] bas  [y] copier  [q] quitter ".to_string(),
            Color::DarkGray,
        ),
    };
    let bar = Paragraph::new(text.as_str()).style(Style::default().fg(color));
    f.render_widget(bar, area);
}
```

**Step 4 : Vérifier que les tests passent**

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo test -p aurora-tui progress_bar 2>&1
```

Attendu : 4 tests PASS.

**Step 5 : Vérifier que tous les tests passent**

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo test -p aurora-tui 2>&1 | grep "test result"
```

Attendu : tous PASS (il y aura des erreurs de compilation sur `split_layout.rs` tant que Task 2 n'est pas faite — normal).

**Step 6 : Commit**

```bash
git add crates/aurora-tui/src/widgets/status_bar.rs crates/aurora-tui/tests/status_bar_test.rs
git commit -m "✨ feat(tui): add progress bar to execution status bar"
```

---

### Task 2 : Mettre à jour `split_layout.rs`

**Files:**
- Modify: `crates/aurora-tui/src/execution/split_layout.rs`

#### État actuel complet de `split_layout.rs`

```rust
use crate::app::{ExecutionState, FocusPanel, LogViewState};
use crate::execution::{beam_list, log_panel};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

pub fn render_execution(
    f: &mut Frame,
    exec: &ExecutionState,
    log_state: &LogViewState,
    tick: u64,
    show_help: bool,
) {
    let area = f.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);

    let beams_focused = exec.focus == FocusPanel::Beams;
    beam_list::render_beam_list(f, exec, tick, split[0], beams_focused);

    let beam = &exec.beams[log_state.beam_index];
    log_panel::render_log_panel(f, beam, log_state, split[1], !beams_focused);

    crate::widgets::status_bar::render_status_bar(
        f,
        outer[1],
        crate::widgets::status_bar::StatusContext::Execution { done: exec.done.is_some() },
    );

    if show_help {
        crate::widgets::help_popup::render_help_popup(
            f,
            area,
            crate::widgets::help_popup::HelpContext::Execution,
        );
    }
}
```

`BeamStatus` doit être importé pour calculer `done_count`. Il vient de `aurora_core::scheduler::BeamStatus`.

**Step 1 : Modifier `split_layout.rs`**

Remplacer l'appel à `render_status_bar` par :

```rust
use aurora_core::scheduler::BeamStatus;

// Dans render_execution, avant render_status_bar :
let total = exec.beams.len();
let done_count = exec.beams.iter().filter(|b| {
    matches!(
        b.status,
        BeamStatus::Success { .. }
            | BeamStatus::Failed { .. }
            | BeamStatus::Skipped { .. }
            | BeamStatus::Cancelled
    )
}).count();

crate::widgets::status_bar::render_status_bar(
    f,
    outer[1],
    crate::widgets::status_bar::StatusContext::Execution {
        done: exec.done.is_some(),
        done_count,
        total,
    },
);
```

**Step 2 : Vérifier la compilation**

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo build -p aurora-tui 2>&1
```

Attendu : OK.

**Step 3 : Lancer tous les tests**

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo test -p aurora-tui 2>&1 | grep "test result"
```

Attendu : tous PASS.

**Step 4 : Commit**

```bash
git add crates/aurora-tui/src/execution/split_layout.rs
git commit -m "✨ feat(tui): pass done_count and total to status bar"
```

---

### Task 3 : Retirer `[X/Y]` du titre du panneau beams

**Files:**
- Modify: `crates/aurora-tui/src/execution/beam_list.rs`

#### Lignes concernées dans `beam_list.rs` (lignes 28-32 actuelles)

```rust
let title = match state.done {
    Some(true) => format!(" Aurora ✔ Done [{}/{}] ", done_count, state.beams.len()),
    Some(false) => format!(" Aurora ✕ Failed [{}/{}] ", done_count, state.beams.len()),
    None => format!(" Aurora  Running... [{}/{}] ", done_count, state.beams.len()),
};
```

**Step 1 : Modifier le titre**

Remplacer par :

```rust
let title = match state.done {
    Some(true) => " Aurora ✔ Done ".to_string(),
    Some(false) => " Aurora ✕ Failed ".to_string(),
    None => " Aurora  Running... ".to_string(),
};
```

Le `done_count` calculé en haut de la fonction devient alors inutilisé — supprimer aussi le bloc de calcul `done_count` (lignes ~14-26 actuelles) :

```rust
// SUPPRIMER ce bloc :
let done_count = state
    .beams
    .iter()
    .filter(|b| { matches!(...) })
    .count();
```

**Step 2 : Compiler**

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo build -p aurora-tui 2>&1
```

Attendu : OK, aucun warning `unused variable`.

**Step 3 : Lancer tous les tests**

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo test -p aurora-tui 2>&1 | grep "test result"
```

Attendu : tous PASS.

**Step 4 : Commit**

```bash
git add crates/aurora-tui/src/execution/beam_list.rs
git commit -m "♻️ refactor(tui): remove [X/Y] counter from beam list title"
```

---

## Vérification finale

```bash
cd /home/jeandenis.vidot@SAFTI.local/Github/jdevelop-io/aurora && cargo test -p aurora-tui && cargo build --release -p aurora
```

Tests attendus au total : ≥ 27 (23 existants + 4 nouveaux `progress_bar_*`).
