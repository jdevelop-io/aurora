# TUI UX/DX Refactor — Design Document

**Date :** 2026-03-04
**Scope :** aurora-tui — state machine, layout splitté, picker amélioré, raccourcis avancés

---

## Contexte

La TUI actuelle est fonctionnelle mais présente plusieurs frictions :

- Navigation/scroll des logs laborieux (pas d'auto-scroll, pas de PgUp/PgDn)
- La liste des beams et les logs sont en mode exclusif (impossible de voir les deux)
- Recherche dans le picker : correspondance exacte uniquement, pas de fuzzy
- `Tab` pour les dépendances mentionné dans l'aide mais non implémenté
- Pas de multi-sélection dans le picker
- `q` quitte sans annuler proprement les tâches en cours
- Architecture mélange state, events et render sans séparation claire

## Objectif

Refonte architecturale propre (state machine + composants Ratatui) qui débloque :

1. Layout splitté permanent pendant l'exécution
2. Auto-scroll intelligent dans les logs
3. Fuzzy search + multi-select + panel deps dans le picker
4. Raccourcis avancés (help popup, re-run, clipboard, annulation propre)

---

## Section 1 : State Machine

### Structure

```rust
enum AppScreen {
    Picker(PickerState),
    Execution(ExecutionState),
    LogView(LogViewState),
    DepsPanel(DepsPanelState),
    HelpPopup { previous: Box<AppScreen> },
    Done { success: bool },
}
```

### Principe

Chaque variant gère ses propres transitions via `handle_key(KeyEvent) -> Option<AppScreen>`. Plus de `match app.mode` éparpillés. Le `App` struct devient :

```rust
pub struct App {
    pub screen: AppScreen,
    pub rx: mpsc::Receiver<SchedulerEvent>,
}
```

### Invariant clé

La navigation reste toujours possible pendant l'exécution des beams. Le layout splitté assure que liste et logs sont accessibles simultanément.

---

## Section 2 : Execution View — Layout splitté

### Layout

```
┌─────────────────────────────────────────────────────────────┐
│ Aurora — Running...                              [3/5 done] │
├──────────────────┬──────────────────────────────────────────┤
│ ⣴  build         │ build — Logs                             │
│ ✔  test    12.3s │ > cargo build --release                  │
│ ─  deploy        │ Compiling aurora v0.1.0                  │
│ ◌  lint          │ ...                                      │
│ ✕  check   2.1s  │ ── stderr ──                             │
│                  │ warning: unused variable `x`             │
├──────────────────┴──────────────────────────────────────────┤
│ [↑↓/jk] beam  [Enter] focus logs  [?] aide  [q] quitter    │
└─────────────────────────────────────────────────────────────┘
```

Ratio : 30% liste / 70% logs (fixe, pas de redimensionnement).

### Auto-scroll intelligent

| État | Comportement |
|------|-------------|
| `scroll_locked: false` | Les logs suivent automatiquement les nouvelles lignes |
| Scroll manuel `↑/k` | `scroll_locked: true` — auto-scroll suspendu |
| `G` ou atteint le bas | `scroll_locked: false` — auto-scroll reprend |
| Beam terminé | Retour automatique en bas si `scroll_locked: false` |

### Touches

| Touche | Action |
|--------|--------|
| `↑↓` / `jk` | Naviguer dans la liste des beams |
| `Enter` | Focus plein écran sur les logs du beam sélectionné |
| `PgUp/PgDn` | Scroll par page dans le panneau logs |
| `G` | Aller au bas des logs (déverrouille l'auto-scroll) |
| `?` | Help popup |
| `q` | Annulation propre + quitter |

---

## Section 3 : Picker amélioré

### Fuzzy search

Algorithme maison (sans dépendance externe), scoring :

1. Match exact du nom → score max
2. Sous-chaîne dans le nom → score haut
3. Caractères non-contigus dans l'ordre (fuzzy) → score moyen
4. Match dans la description → score bas

Les résultats sont triés par score décroissant. Les caractères matchés sont highlighted.

### Multi-sélection

- `Space` : toggle sélection (affiche `[ ]` / `[x]` en préfixe)
- `Enter` avec 0 sélectionné : lance le beam survolé
- `Enter` avec N sélectionnés : lance tous les beams cochés
- Titre dynamique : `Aurora — Choisir un beam (3 sélectionnés)`

### Panel dépendances (Tab)

```
┌─────────────────────────────────────────────────────────────┐
│  Aurora — Choisir un beam              [build]              │
├──────────────────────────┬──────────────────────────────────┤
│ [x] build                │  Dépendances de build:           │
│ [ ] test                 │                                  │
│ [ ] deploy               │  ├── check                       │
│ > cargo bu               │  └── lint                        │
│                          │                                  │
│                          │  Sera lancé par: deploy          │
├──────────────────────────┴──────────────────────────────────┤
│ [↑↓] naviguer  [Space] sélec  [Tab] deps  [Enter] lancer   │
└─────────────────────────────────────────────────────────────┘
```

`Tab` bascule l'affichage du panel dépendances. Le panel affiche :
- Les prérequis du beam sélectionné (dépendances directes)
- Les beams qui dépendent de lui ("sera lancé par")

---

## Section 4 : Raccourcis avancés

### Help popup (`?`)

Overlay centré, liste tous les raccourcis contextuels selon l'écran actif. Fermeture par `Esc` ou `?`. Implémenté via `HelpPopup { previous: Box<AppScreen> }` qui retourne à l'écran précédent.

### Re-run (`r`)

Disponible sur un beam `Failed` ou en mode `Done`. Nécessite un channel retour `mpsc::Sender<SchedulerCommand>` dans aurora-core (à définir). Relance uniquement le beam ciblé (et ses dépendances échouées).

### Copier les logs (`y`)

Dans le panneau logs : copie le contenu stdout+stderr du beam dans le clipboard via `arboard`. Fallback gracieux si pas de clipboard (SSH sans X11) : notification non-bloquante affichée 2s dans la status bar.

### Annulation propre (`q` en Running)

Actuellement `q` quitte sans canceller les processus. Nouveau comportement :
- `q` en mode Running : envoie un signal d'annulation au scheduler → attend la confirmation → quitte
- `Ctrl+C` : même comportement
- Besoin d'un `CancellationToken` partagé entre TUI et scheduler

---

## Structure de fichiers cible

```
crates/aurora-tui/src/
├── lib.rs              # Point d'entrée, setup terminal
├── app.rs              # App struct + AppScreen enum + transitions
├── picker/
│   ├── mod.rs          # PickerState, PickerBeam
│   ├── fuzzy.rs        # Algorithme de scoring fuzzy
│   ├── view.rs         # render_picker (Widget impl)
│   └── deps_panel.rs   # render_deps_panel
├── execution/
│   ├── mod.rs          # ExecutionState, LogViewState
│   ├── split_layout.rs # Layout 30/70 + focus logs
│   ├── beam_list.rs    # render_beam_list (Widget impl)
│   └── log_panel.rs    # render_log_panel + auto-scroll
└── widgets/
    ├── status_bar.rs   # StatusBar contextuelle
    └── help_popup.rs   # HelpPopup overlay
```

---

## Dépendances à ajouter

| Crate | Usage |
|-------|-------|
| `arboard` | Clipboard cross-platform pour `y` |

Pas de framework TUI supplémentaire (ratatui pur).

## Impact sur aurora-core

- Nouveau `SchedulerCommand` enum avec variant `Cancel` et `ReRun { beam: String }`
- Nouveau `CancellationToken` partagé (via `tokio_util::sync::CancellationToken` déjà dispo)

---

## Non-inclus dans ce scope

- Thème/couleurs personnalisables
- Redimensionnement du split avec `<`/`>`
- Historique des exécutions
- Export des logs vers fichier
