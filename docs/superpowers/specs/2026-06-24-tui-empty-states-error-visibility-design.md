# TUI : états vides et erreurs visibles

Date : 2026-06-24
Crate : `aurora-tui`

## Objectif

Réduire les frictions UX les plus visibles de la TUI au quotidien :
les états vides non signalés et la difficulté à repérer un échec. Trois
changements isolés, chacun avec un périmètre clair.

## A. État vide du picker

Fichier : `crates/aurora-tui/src/picker/view.rs`

Quand la liste filtrée est vide, afficher un message centré grisé dans la
zone liste plutôt qu'un cadre vide :

- recherche active sans résultat : « Aucun beam ne correspond à « {search} » »
- aucun beam disponible : « Aucun beam disponible »

La navigation n'est pas modifiée. Le rendu détecte `filtered.is_empty()` et
choisit le message selon que `search` est vide ou non.

## B. Placeholder du panneau logs

Fichier : `crates/aurora-tui/src/execution/log_panel.rs`

Quand un beam n'a ni stdout ni stderr, afficher une ligne grisée adaptée au
statut au lieu d'un panneau blanc :

- `Pending` : « (en attente de démarrage) »
- `Running` : « (pas encore de sortie) »
- terminé (Success/Failed/Skipped/Cancelled) : « (aucune sortie) »

Le widget a déjà accès à `beam.status`, aucun nouveau paramètre.

## C. Auto-sélection du premier beam en échec

Fichiers : `crates/aurora-tui/src/app.rs`, `crates/aurora-tui/src/lib.rs`

Quand l'exécution se termine en échec (`AllDone { success: false }`),
sélectionner automatiquement le premier beam au statut `Failed` pour afficher
ses logs sans navigation manuelle.

- `ExecutionState::select_first_failed() -> bool` : place `self.selected` sur
  le premier beam `Failed`, renvoie `true` si trouvé, ne touche à rien sinon.
- Dans la boucle de `run_execution_tui`, à réception de `AllDone` avec
  `success == false`, appeler la méthode puis resynchroniser
  `log_state.beam_index = exec.selected` et `scroll_locked = false`.

On cible `Failed` uniquement, pas `Cancelled` : c'est le statut porteur de la
vraie erreur. La logique se déclenche à chaque `AllDone`, donc fonctionne aussi
après un re-run.

## Tests

- `select_first_failed` : sélectionne le premier `Failed`, ignore
  `Cancelled`/`Success`, ne bouge pas si aucun `Failed`. Unit test dans `app.rs`.
- États vides : rendus visuels. La logique de choix du message est triviale et
  vérifiée à l'œil ; pas de test de rendu dédié.

## Hors périmètre

Réordonnancement de la liste, recherche dans les logs, filtrage en exécution,
export des logs : reportés à des lots ultérieurs.
