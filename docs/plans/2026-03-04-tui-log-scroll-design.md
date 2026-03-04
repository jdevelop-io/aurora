# Design : scroll des logs dans la vue split TUI

**Date :** 2026-03-04
**Contexte :** La vue split (beams à gauche, logs à droite) ne permet pas de scroller les logs avec j/k car ces touches sont capturées pour la navigation entre beams.

## Problème

Dans `lib.rs`, `KeyCode::Up | Char('k')` et `KeyCode::Down | Char('j')` appelaient `exec.select_prev/next()`. La logique de scroll ligne par ligne existait dans `LogViewState::handle_key` mais n'était jamais accessible depuis la vue split.

## Solution retenue : focus switchable avec Tab

`Tab` bascule le focus entre le panneau beams (gauche) et le panneau logs (droite). Le comportement de j/k dépend du focus actif.

## Architecture

### Nouveau type dans `app.rs`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FocusPanel { Beams, Logs }
```

### `ExecutionState` — champ ajouté

```rust
pub struct ExecutionState {
    // ... existant ...
    pub focus: FocusPanel,  // nouveau
}
```

- Défaut : `FocusPanel::Beams`
- `Tab` dans `handle_key` bascule `focus`

### Dispatch clavier dans `lib.rs`

| Touche | Focus Beams | Focus Logs |
|--------|-------------|------------|
| `j/k/↑↓` | Naviguer beams | Scroller logs ligne/ligne |
| `PageUp/Dn` | Scroller logs | Scroller logs |
| `G` | Bas des logs | Bas des logs |
| `Tab` | → Focus Logs | → Focus Beams |
| `q/Ctrl+C` | Quitter | Quitter |

Quand on change de beam (focus Beams), `scroll_locked` est remis à `false`.

### Indicateur visuel

- Bordure du panneau actif : `Color::Yellow`
- Bordure du panneau inactif : couleur par défaut

Les fonctions `render_beam_list` et `render_log_panel` reçoivent un `bool focused` pour adapter la couleur de bordure.

### Help popup

Ajouter dans `HelpContext::Execution` :
```
 Tab         Basculer focus beams/logs
```

## Fichiers impactés

- `crates/aurora-tui/src/app.rs` — `FocusPanel`, champ dans `ExecutionState`
- `crates/aurora-tui/src/lib.rs` — dispatch conditionnel selon `exec.focus`
- `crates/aurora-tui/src/execution/beam_list.rs` — paramètre `focused: bool`
- `crates/aurora-tui/src/execution/log_panel.rs` — paramètre `focused: bool`
- `crates/aurora-tui/src/execution/split_layout.rs` — passer `exec.focus` aux renderers
- `crates/aurora-tui/src/widgets/help_popup.rs` — documenter Tab

## Tests à ajouter

- `tab_switches_focus_from_beams_to_logs`
- `tab_switches_focus_from_logs_to_beams`
- `jk_scroll_logs_when_log_focused`
- `jk_navigate_beams_when_beam_focused`
