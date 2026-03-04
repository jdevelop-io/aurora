# Re-run Beam Design

**Goal:** Appuyer sur `r` sur un beam Failed/Cancelled relance ce beam et ses dépendances échouées, sans re-exécuter les dépendances déjà réussies.

**Architecture:** Séparation en deux groupes (to_rerun / pre_success), nouveau Scheduler par re-run, réutilisation du même état TUI avec reset partiel.

---

## Composants

### `aurora-core/scheduler.rs`

`Scheduler::run(root: &str, pre_success: &[String])` — nouveau paramètre.

Les beams dans `pre_success` sont traités comme déjà réussis au début de chaque level : aucun événement émis, leurs dépendants sont débloqués normalement. Le DAG complet reste intact.

### `aurora-tui/app.rs`

- `BeamView` : ajout de `pub depends_on: Vec<String>`
- `ExecutionState::compute_rerun(selected_idx) -> (String, Vec<String>, Vec<String>)` : retourne `(root_name, to_rerun, pre_success)`. Traverse `depends_on` depuis le beam sélectionné, classe chaque beam selon son statut actuel.
- `ExecutionState::reset_for_rerun(names: &[String])` : remet les beams listés à `Pending`, vide `stdout`/`stderr`, `started_at = None`.
- `ExecutionAction::Rerun { root: String, pre_success: Vec<String> }` : nouvelle variante.
- `handle_key` : `r` déclenche `Rerun` si `exec.done.is_some()` et le beam sélectionné est `Failed` ou `Cancelled`.

### `aurora-tui/lib.rs`

`run_execution_tui` accepte `rerun: impl Fn(String, Vec<String>) -> mpsc::Receiver<SchedulerEvent>`.

Sur `ExecutionAction::Rerun { root, pre_success }` :
1. `exec.reset_for_rerun(&to_rerun)` (déduit depuis `compute_rerun`)
2. `exec.done = None`
3. `rx = rerun(root, pre_success)` → nouveau rx

### `aurora/main.rs`

Closure `rerun` capturée depuis `main` :
- Clone `beams`, `executors`, `working_dir`, `env`
- Crée un nouveau `Scheduler` avec la liste complète des beams
- Spawn via `Handle::current().spawn(scheduler.run(root, pre_success))`
- Retourne le nouveau `rx`

### Help popup

Ajout dans `HelpContext::Execution` : `r   Re-lancer le beam (si Failed/Cancelled)`.

---

## Flux d'exemple

Beams : `test` (Success) → `build` (Failed) → `deploy` (Failed/Cancelled)

`r` sur `deploy` :
- `to_rerun` = `["build", "deploy"]`
- `pre_success` = `["test"]`
- Scheduler reçoit tous les beams, `pre_success = ["test"]`
- Level 0 : `test` → pré-succès silencieux
- Level 1 : `build` → relancé
- Level 2 : `deploy` → relancé si `build` réussit

---

## Ce qui n'est PAS implémenté (YAGNI)

- Re-run pendant une exécution en cours
- Re-run depuis la log view
- Confirmation avant re-run
