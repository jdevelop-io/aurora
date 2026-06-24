# TUI : recherche dans les logs

Date : 2026-06-24
Crate : `aurora-tui`

## Objectif

Permettre de chercher du texte dans les logs du beam sélectionné pendant et
après l'exécution, sans scroller ligne à ligne. Recherche incrémentale,
insensible à la casse, navigation entre les correspondances.

## Modèle d'interaction

- `/` ouvre le mode recherche : invite dans la barre de statut
  (` /requête  [3/12] `).
- Frappe incrémentale : à chaque caractère, recalcul des correspondances et
  saut au premier match. Casse insensible.
- `Entrée` valide : sortie du mode saisie, requête conservée (surlignage
  maintenu, `n`/`N` opérationnels).
- `Esc` annule : requête et surlignage effacés.
- `n` / `N` : match suivant / précédent, avec bouclage. Actifs uniquement si
  une requête est active.
- En saisie, toutes les touches alimentent la requête ; `Backspace` efface.

## Granularité

Navigation par ligne : `n`/`N` sautent à la prochaine ligne contenant la
requête. Toutes les occurrences sont surlignées (jaune) ; la ligne du match
courant reçoit un surlignage distinct (inversé).

## Architecture

Refactor préalable au service de la fonctionnalité : centraliser le calcul des
lignes de logs, aujourd'hui dupliqué dans `log_panel.rs` et répété dans
`lib.rs`.

Sur `BeamView` (`app.rs`) :

- `enum LogKind { Stdout, Stderr, Separator, Placeholder }`
- `iter_log_lines() -> impl Iterator<Item = (&str, LogKind)>` sans allocation
- `log_line_count() -> usize`

Le rendu et la recherche partagent cette source unique : les index de match et
l'offset de scroll restent alignés.

Nouvel état (`app.rs`) :

```rust
pub struct LogSearch {
    pub input_active: bool,   // mode saisie
    pub query: String,
    pub matches: Vec<usize>,  // index de lignes logiques
    pub current: usize,       // index dans matches
}
```

Méthodes :

- `recompute(lines)` : remplit `matches` avec les index de lignes Stdout/Stderr
  contenant `query` (insensible à la casse) ; requête vide -> aucun match.
- `next()` / `prev()` : avance/recule `current` avec bouclage ; no-op si
  `matches` vide.
- accès au numéro de ligne du match courant pour positionner le scroll.

Le saut place `LogViewState.scroll` sur la ligne du match et passe
`scroll_locked = true` (réutilise l'infra de scroll existante).

## Intégration boucle (`lib.rs`)

Au début du traitement clavier : si `search.input_active`, capter les touches
(saisie / Entrée / Esc / Backspace) avant tout le reste. Sinon, `/` active la
saisie, `n`/`N` naviguent si une requête est active. Au changement de beam
sélectionné, recalcul des matches pour le nouveau beam, requête conservée.

## Rendu

`log_panel::render_log_panel` reçoit `Option<&LogSearch>` :

- surligne les occurrences dans les lignes Stdout/Stderr,
- style distinct pour la ligne du match courant.

La barre de statut affiche l'invite et le compteur en mode recherche. L'aide
(`?`, contexte Execution et LogView) est complétée avec `/`, `n`, `N`.

## Tests

- `LogSearch::recompute` : index corrects, insensible à la casse, requête vide
  -> aucun match.
- `next` / `prev` : progression et bouclage ; comportement quand `matches` est
  vide.
- `BeamView::iter_log_lines` : ordre et `LogKind` corrects (stdout, séparateur,
  stderr ; placeholder quand le beam n'a aucune sortie).

Le surlignage reste vérifié à l'œil.

## Hors périmètre

Filtrage des beams, navigation occurrence par occurrence dans une ligne,
recherche par expression régulière.
