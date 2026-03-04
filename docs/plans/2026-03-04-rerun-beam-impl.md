# Re-run Beam Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Appuyer sur `r` sur un beam Failed/Cancelled relance ce beam et ses dépendances échouées, sans re-exécuter les dépendances déjà réussies.

**Architecture:** Ajout d'un paramètre `pre_success` au Scheduler (beams traités comme déjà réussis sans émettre d'événements), propagation du graphe de dépendances dans la TUI, closure de re-run passée depuis `main.rs`.

**Tech Stack:** Rust, tokio, ratatui, crossterm, petgraph (déjà utilisé dans aurora-core)

---

## Contexte codebase

- `crates/aurora-core/src/scheduler.rs` — `Scheduler::run(self, root: &str)` à modifier
- `crates/aurora-tui/src/app.rs` — `BeamView`, `ExecutionState`, `ExecutionAction`
- `crates/aurora-tui/src/lib.rs` — `run_execution_tui(beam_names: Vec<String>, rx, ...)` à modifier
- `crates/aurora/src/main.rs` — point d'entrée, orchestre scheduler + TUI
- `crates/aurora-tui/src/widgets/help_popup.rs` — aide clavier
- Tests existants dans `crates/aurora-tui/tests/app_state_test.rs` et `log_view_test.rs` utilisent `ExecutionState::new(vec!["name".to_string()])` — à mettre à jour

---

## Task 1 : Scheduler — paramètre `pre_success`

**Files:**
- Modify: `crates/aurora-core/src/scheduler.rs:68`
- Test: `crates/aurora-core/tests/scheduler_rerun_test.rs` (à créer)

**Contexte:** `Scheduler::run` prend un `root: &str` et exécute tous les beams transitifs. On ajoute `pre_success: &[String]` — les beams dans cette liste sont traités comme "déjà réussis" : pas d'événement émis, leurs dépendants sont débloqués normalement.

**Step 1 : Écrire le test échouant**

Créer `crates/aurora-core/tests/scheduler_rerun_test.rs` :

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use aurora_core::scheduler::{Scheduler, SchedulerEvent};
use aurora_core::ast::{Beam, Run};
use aurora_executor_api::{Executor, ExecutionInput, ExecutionOutput};
use tokio::sync::mpsc;

struct AlwaysSuccessExecutor;

#[async_trait]
impl Executor for AlwaysSuccessExecutor {
    fn name(&self) -> &str { "success" }
    async fn execute(&self, _: ExecutionInput) -> Result<ExecutionOutput> {
        Ok(ExecutionOutput { exit_code: 0, stdout: vec![], stderr: vec![] })
    }
}

fn beam(name: &str, deps: Vec<&str>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: Some(Run { commands: vec!["echo ok".to_string()], executor: None }),
    }
}

#[tokio::test]
async fn pre_success_beams_emit_no_events() {
    let beams = vec![
        beam("dep", vec![]),
        beam("main", vec!["dep"]),
    ];
    let mut executors: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    executors.insert("local".into(), Arc::new(AlwaysSuccessExecutor));

    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(
        beams,
        executors,
        tx,
        None,
        PathBuf::from("/tmp"),
        std::env::vars().collect(),
    );

    // "dep" est déjà réussi — ne doit émettre aucun événement
    scheduler.run("main", &["dep".to_string()]).await.unwrap();

    let mut events = vec![];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }

    // Aucun BeamStarted ni BeamCompleted pour "dep"
    let dep_events: Vec<_> = events.iter().filter(|e| match e {
        SchedulerEvent::BeamStarted { name } | SchedulerEvent::BeamCompleted { name, .. } => name == "dep",
        _ => false,
    }).collect();
    assert!(dep_events.is_empty(), "dep ne doit pas émettre d'événements : {:?}", dep_events);

    // "main" doit avoir été exécuté normalement
    let main_started = events.iter().any(|e| matches!(e, SchedulerEvent::BeamStarted { name } if name == "main"));
    assert!(main_started, "main doit avoir été démarré");
}
```

**Step 2 : Vérifier que le test échoue**

```bash
cargo test -p aurora-core -- pre_success_beams_emit_no_events 2>&1
```
Expected: erreur de compilation (paramètre `pre_success` inexistant).

**Step 3 : Implémenter dans `scheduler.rs`**

Changer la signature :
```rust
pub async fn run(self, root: &str, pre_success: &[String]) -> Result<bool> {
```

Dans la boucle des levels, ajouter AVANT la vérification `cancelled` :
```rust
for beam_name in level {
    // Beam déjà réussi — silencieux, dépendants débloqués
    if pre_success.contains(beam_name) {
        continue;
    }
    if cancelled.contains(beam_name) {
        // ... code existant
    }
    // ... reste du code existant
}
```

**Step 4 : Mettre à jour l'appel existant dans `main.rs`**

Dans `crates/aurora/src/main.rs`, la ligne `scheduler.run(&target_clone)` devient :
```rust
scheduler.run(&target_clone, &[]).await
```

**Step 5 : Vérifier que le test passe**

```bash
cargo test -p aurora-core -- pre_success_beams_emit_no_events 2>&1
```
Expected: PASS

**Step 6 : Build complet**

```bash
cargo build 2>&1
```
Expected: aucune erreur.

**Step 7 : Commit**

```bash
git add crates/aurora-core/src/scheduler.rs crates/aurora-core/tests/scheduler_rerun_test.rs crates/aurora/src/main.rs
git commit -m "✨ feat(scheduler): add pre_success param to skip already-done beams"
```

---

## Task 2 : BeamView — ajout de `depends_on` + mise à jour des call sites

**Files:**
- Modify: `crates/aurora-tui/src/app.rs:8-26`
- Modify: `crates/aurora-tui/tests/app_state_test.rs` (call sites)
- Modify: `crates/aurora-tui/tests/log_view_test.rs` (call sites)

**Contexte:** `BeamView::new` prend actuellement juste un `name: String`. On ajoute `depends_on: Vec<String>`. `ExecutionState::new` passe de `Vec<String>` à `Vec<(String, Vec<String>)>`.

**Step 1 : Écrire un test pour la nouvelle signature**

Dans `crates/aurora-tui/tests/app_state_test.rs`, ajouter en fin de fichier :

```rust
#[test]
fn beam_view_stores_depends_on() {
    use aurora_tui::app::BeamView;
    let beam = BeamView::new("deploy".to_string(), vec!["build".to_string()]);
    assert_eq!(beam.depends_on, vec!["build".to_string()]);
}
```

**Step 2 : Vérifier que ce test échoue**

```bash
cargo test -p aurora-tui -- beam_view_stores_depends_on 2>&1
```
Expected: erreur de compilation (`BeamView::new` ne prend pas `depends_on`).

**Step 3 : Modifier `BeamView` dans `app.rs`**

```rust
#[derive(Debug, Clone)]
pub struct BeamView {
    pub name: String,
    pub depends_on: Vec<String>,  // NOUVEAU
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
    // ... reste inchangé
}
```

Modifier `ExecutionState::new` :
```rust
pub fn new(beam_info: Vec<(String, Vec<String>)>) -> Self {
    ExecutionState {
        beams: beam_info.into_iter().map(|(name, deps)| BeamView::new(name, deps)).collect(),
        selected: 0,
        done: None,
        focus: FocusPanel::Beams,
    }
}
```

**Step 4 : Mettre à jour les tests existants**

Dans `crates/aurora-tui/tests/app_state_test.rs`, remplacer toutes les occurrences de `ExecutionState::new(vec!["...".to_string()])` par `ExecutionState::new(vec![("...".to_string(), vec![])])` :

- `execution_q_returns_quit` : `ExecutionState::new(vec![("build".to_string(), vec![]), ("test".to_string(), vec![])])`
- `execution_enter_opens_log_view` : `ExecutionState::new(vec![("build".to_string(), vec![])])`
- `default_focus_is_beams` : `ExecutionState::new(vec![("a".to_string(), vec![]), ("b".to_string(), vec![])])`
- `tab_switches_focus_from_beams_to_logs` : `ExecutionState::new(vec![("a".to_string(), vec![])])`
- `tab_switches_focus_from_logs_to_beams` : idem

Dans `crates/aurora-tui/tests/log_view_test.rs` :
- `focus_beams_by_default_and_tab_toggles` : `ExecutionState::new(vec![("a".to_string(), vec![]), ("b".to_string(), vec![])])`
- `select_next_does_not_affect_log_scroll_position` : `ExecutionState::new(vec![("a".to_string(), vec![]), ("b".to_string(), vec![]), ("c".to_string(), vec![])])`

**Step 5 : Tous les tests doivent passer**

```bash
cargo test -p aurora-tui 2>&1
```
Expected: tous les tests PASS (y compris le nouveau `beam_view_stores_depends_on`).

**Step 6 : Commit**

```bash
git add crates/aurora-tui/src/app.rs crates/aurora-tui/tests/app_state_test.rs crates/aurora-tui/tests/log_view_test.rs
git commit -m "✨ feat(tui): add depends_on to BeamView, update ExecutionState::new signature"
```

---

## Task 3 : ExecutionState — compute_rerun, reset_for_rerun, ExecutionAction::Rerun, touche `r`

**Files:**
- Modify: `crates/aurora-tui/src/app.rs:59-69` (ExecutionAction) et `243-258` (handle_key)
- Test: `crates/aurora-tui/tests/rerun_test.rs` (à créer)

**Contexte:** On ajoute la logique de calcul des beams à relancer. `compute_rerun` traverse le graphe `depends_on` depuis le beam sélectionné (en BFS) et sépare les beams en `to_rerun` (Failed/Cancelled) et `pre_success` (Success/Skipped). `reset_for_rerun` remet les beams listés à Pending. La touche `r` retourne `ExecutionAction::Rerun`.

**Step 1 : Créer `crates/aurora-tui/tests/rerun_test.rs`**

```rust
use aurora_tui::app::{BeamView, ExecutionState, ExecutionAction};
use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn make_state() -> ExecutionState {
    // test   (Success)
    //   └── build (Failed)
    //         └── deploy (Failed)  ← selected
    ExecutionState::new(vec![
        ("test".to_string(), vec![]),
        ("build".to_string(), vec!["test".to_string()]),
        ("deploy".to_string(), vec!["build".to_string()]),
    ])
}

fn set_done(state: &mut ExecutionState) {
    state.done = Some(false);
    // test = Success
    state.apply_event(SchedulerEvent::BeamStarted { name: "test".to_string() });
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "test".to_string(),
        status: BeamStatus::Success { duration: Duration::from_secs(1), cached: false },
    });
    // build = Failed
    state.apply_event(SchedulerEvent::BeamStarted { name: "build".to_string() });
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "build".to_string(),
        status: BeamStatus::Failed { exit_code: 1, duration: Duration::from_secs(1) },
    });
    // deploy = Cancelled
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "deploy".to_string(),
        status: BeamStatus::Cancelled,
    });
}

#[test]
fn compute_rerun_returns_failed_and_cancelled_deps() {
    let mut state = make_state();
    set_done(&mut state);
    state.selected = 2; // deploy

    let (root, to_rerun, pre_success) = state.compute_rerun(2);

    assert_eq!(root, "deploy");
    // build (Failed) et deploy (Cancelled) doivent être dans to_rerun
    assert!(to_rerun.contains(&"build".to_string()));
    assert!(to_rerun.contains(&"deploy".to_string()));
    // test (Success) doit être dans pre_success
    assert!(pre_success.contains(&"test".to_string()));
    // test ne doit PAS être dans to_rerun
    assert!(!to_rerun.contains(&"test".to_string()));
}

#[test]
fn reset_for_rerun_clears_beam_state() {
    let mut state = make_state();
    set_done(&mut state);

    // Ajouter quelques logs à build
    state.beams[1].stdout.push("some output".to_string());

    state.reset_for_rerun(&["build".to_string(), "deploy".to_string()]);

    // build et deploy doivent être Pending
    assert!(matches!(state.beams[1].status, BeamStatus::Pending));
    assert!(matches!(state.beams[2].status, BeamStatus::Pending));
    // stdout effacé
    assert!(state.beams[1].stdout.is_empty());
    // done reset
    assert!(state.done.is_none());
    // test inchangé (Success)
    assert!(matches!(state.beams[0].status, BeamStatus::Success { .. }));
}

#[test]
fn r_key_returns_rerun_action_when_done_and_failed() {
    let mut state = make_state();
    set_done(&mut state);
    state.selected = 2; // deploy (Cancelled)

    let action = state.handle_key(key(KeyCode::Char('r')));

    assert!(matches!(action, Some(ExecutionAction::Rerun { .. })));
    if let Some(ExecutionAction::Rerun { root, pre_success }) = action {
        assert_eq!(root, "deploy");
        assert!(pre_success.contains(&"test".to_string()));
    }
}

#[test]
fn r_key_ignored_when_exec_still_running() {
    let mut state = make_state();
    // done = None → en cours
    state.selected = 1; // build

    let action = state.handle_key(key(KeyCode::Char('r')));
    assert!(action.is_none());
}

#[test]
fn r_key_ignored_on_success_beam() {
    let mut state = make_state();
    set_done(&mut state);
    state.selected = 0; // test (Success)

    let action = state.handle_key(key(KeyCode::Char('r')));
    assert!(action.is_none());
}
```

**Step 2 : Vérifier que les tests échouent**

```bash
cargo test -p aurora-tui -- rerun 2>&1
```
Expected: erreurs de compilation (`compute_rerun`, `reset_for_rerun`, `ExecutionAction::Rerun` inexistants).

**Step 3 : Implémenter dans `app.rs`**

Ajouter `Rerun` à `ExecutionAction` :
```rust
#[derive(Debug, PartialEq)]
pub enum ExecutionAction {
    Quit,
    OpenLogView { beam_index: usize },
    Rerun { root: String, pre_success: Vec<String> },
}
```

Ajouter dans `impl ExecutionState` :
```rust
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
        match &beam.status {
            BeamStatus::Failed { .. } | BeamStatus::Cancelled => {
                to_rerun.push(beam.name.clone());
            }
            BeamStatus::Success { .. } | BeamStatus::Skipped { .. } => {
                pre_success.push(beam.name.clone());
            }
            _ => {}
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
```

Ajouter dans `handle_key` (dans le `match key.code`) :
```rust
KeyCode::Char('r') => {
    if self.done.is_some() {
        let beam = &self.beams[self.selected];
        if matches!(beam.status, BeamStatus::Failed { .. } | BeamStatus::Cancelled) {
            let (root, to_rerun, pre_success) = self.compute_rerun(self.selected);
            self.reset_for_rerun(&to_rerun);
            return Some(ExecutionAction::Rerun { root, pre_success });
        }
    }
    None
}
```

**Step 4 : Vérifier que tous les tests passent**

```bash
cargo test -p aurora-tui 2>&1
```
Expected: tous PASS.

**Step 5 : Commit**

```bash
git add crates/aurora-tui/src/app.rs crates/aurora-tui/tests/rerun_test.rs
git commit -m "✨ feat(tui): add compute_rerun, reset_for_rerun, ExecutionAction::Rerun, r key"
```

---

## Task 4 : lib.rs + main.rs — câblage du re-run

**Files:**
- Modify: `crates/aurora-tui/src/lib.rs:21`
- Modify: `crates/aurora/src/main.rs:96-122`

**Contexte:** `run_execution_tui` reçoit désormais `beam_info: Vec<(String, Vec<String>)>` et une closure `rerun`. Dans la boucle événements, on gère `ExecutionAction::Rerun`. Dans `main.rs`, on construit la closure et on passe `beam_info`.

**Pas de nouveau test** — le câblage est vérifié par `cargo build`.

**Step 1 : Modifier `run_execution_tui` dans `lib.rs`**

Changer la signature :
```rust
pub async fn run_execution_tui(
    beam_info: Vec<(String, Vec<String>)>,
    mut rx: mpsc::Receiver<SchedulerEvent>,
    rerun: impl Fn(String, Vec<String>) -> mpsc::Receiver<SchedulerEvent>,
) -> Result<()> {
    tokio::task::block_in_place(move || {
        // ...
        let mut exec = ExecutionState::new(beam_info);  // au lieu de beam_names
```

Dans la boucle des touches, ajouter après le bloc `KeyCode::Char('y')` :
```rust
KeyCode::Char('r') => {
    if let Some(ExecutionAction::Rerun { root, pre_success }) = exec.handle_key(key) {
        log_state = LogViewState::new(exec.selected, 0);
        rx = rerun(root, pre_success);
    }
}
```

**Note :** Il faut aussi importer `ExecutionAction` en haut du fichier si pas déjà importé.
Ligne 7 : `use app::{ExecutionAction, ExecutionState, FocusPanel, LogViewState, PickerAction, PickerState};`

**Step 2 : Modifier `main.rs`**

Remplacer :
```rust
let beam_names: Vec<String> = beam_file.beams.iter()
    .filter(|b| b.name != "__multi__")
    .map(|b| b.name.clone())
    .collect();
```

Par :
```rust
let beam_info: Vec<(String, Vec<String>)> = beam_file.beams.iter()
    .filter(|b| b.name != "__multi__")
    .map(|b| (b.name.clone(), b.depends_on.clone()))
    .collect();
```

Avant `aurora_tui::run_execution_tui(...)`, ajouter la closure de re-run :
```rust
let rerun_beams = beam_file.beams.clone();
let rerun_executors = executors.clone();
let rerun_max_par = beam_file.config.as_ref().and_then(|c| c.max_parallelism);
let rerun_working_dir = working_dir.clone();
let rerun_env = env.clone();

let rerun = move |root: String, pre_success: Vec<String>| -> mpsc::Receiver<SchedulerEvent> {
    let (tx, rx) = mpsc::channel(128);
    let scheduler = Scheduler::new(
        rerun_beams.clone(),
        rerun_executors.clone(),
        tx,
        rerun_max_par,
        rerun_working_dir.clone(),
        rerun_env.clone(),
    );
    tokio::runtime::Handle::current().spawn(async move {
        if let Err(e) = scheduler.run(&root, &pre_success).await {
            eprintln!("Scheduler error: {}", e);
        }
    });
    rx
};

aurora_tui::run_execution_tui(beam_info, rx, rerun).await?;
```

**Step 3 : Build complet**

```bash
cargo build 2>&1
```
Expected: aucune erreur.

**Step 4 : Tests**

```bash
cargo test 2>&1
```
Expected: tous PASS.

**Step 5 : Commit**

```bash
git add crates/aurora-tui/src/lib.rs crates/aurora/src/main.rs
git commit -m "✨ feat(tui): wire rerun closure in run_execution_tui and main"
```

---

## Task 5 : Help popup + status bar

**Files:**
- Modify: `crates/aurora-tui/src/widgets/help_popup.rs:60-62`
- Modify: `crates/aurora-tui/src/widgets/status_bar.rs:37` (hint `[r]` dans la barre)

**Contexte:** Ajouter la touche `r` à l'aide et à la barre de statut.

**Pas de test** — visuel uniquement.

**Step 1 : Help popup**

Dans `HelpContext::Execution`, ajouter après la ligne `y` :
```rust
Line::from(" r          Re-lancer le beam (si Failed/Cancelled)"),
```

**Step 2 : Status bar — hint `[r]` quand exec terminée**

Dans `status_bar.rs`, `StatusContext::Execution { done: Some(true), ... }` et `done: Some(false)` :

Changer le texte de :
```
"{}[↑↓/jk] beam  [y] copier  [?] aide  [q] quitter "
```
en :
```
"{}[↑↓/jk] beam  [r] re-run  [y] copier  [?] aide  [q] quitter "
```

(uniquement les deux branches `done: Some(...)`)

**Step 3 : Build + tests**

```bash
cargo build && cargo test -p aurora-tui 2>&1
```
Expected: tout PASS.

**Step 4 : Install + commit**

```bash
cargo install --path crates/aurora
git add crates/aurora-tui/src/widgets/help_popup.rs crates/aurora-tui/src/widgets/status_bar.rs
git commit -m "✨ feat(tui): add r shortcut to help popup and status bar"
```
