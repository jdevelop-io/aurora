# Design : progress bar dans la status bar

**Date :** 2026-03-04
**Contexte :** Remplacer le `[X/Y]` dans le titre du panneau beams par une barre de progression ASCII dans la status bar.

## Comportement attendu

```
 [██████████░░░░░░] 5/8  [↑↓/jk] beam  [Tab] focus  [?] aide  [q] annuler
```

- Barre de 16 chars : `█` pour les beams terminés, `░` pour les restants
- Compteur `done/total` à droite de la barre
- En cours : couleur par défaut (gris)
- Terminé succès : barre verte
- Terminé avec échec : barre rouge

## Définition "terminé"

`BeamStatus::Success | Failed | Skipped | Cancelled` — tout beam qui ne tourne plus.

## Composants modifiés

### `status_bar.rs`

`StatusContext::Execution` reçoit deux champs supplémentaires :

```rust
StatusContext::Execution {
    done: bool,
    done_count: usize,
    total: usize,
}
```

Le rendu construit la barre dynamiquement :
- `filled = (done_count * BAR_WIDTH) / total`
- `BAR_WIDTH = 16`
- Couleur : gris si en cours, vert si `done && success`, rouge si `done && !success`
- Si `total == 0` : afficher seulement les hints (pas de barre)

### `split_layout.rs`

Calcule `done_count` et `total` depuis `exec.beams` et les passe au `StatusContext`.

### `beam_list.rs`

Retire `[X/Y]` du titre (la progression est maintenant dans la status bar).

## Fichiers impactés

- `crates/aurora-tui/src/widgets/status_bar.rs`
- `crates/aurora-tui/src/execution/split_layout.rs`
- `crates/aurora-tui/src/execution/beam_list.rs`
