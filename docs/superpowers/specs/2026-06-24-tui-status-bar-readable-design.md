# TUI : status bar lisible

Date : 2026-06-24
Crate : `aurora-tui`

## Problème

La status bar est rendue en une seule couleur `DarkGray` sur fond noir
(illisible), sans distinction touches / libellés, et déborde à droite (raccourcis
coupés). Elle n'expose pas non plus les raccourcis récents (`/`, `g`, `Ctrl+U/D`).

## Rendu

Reconstruire `render_status_bar` pour émettre une `Line` de spans colorés au lieu
d'un `Paragraph` mono-couleur.

Schéma : `{symbole} {Statut}  {fait}/{total} [{barre}]   {raccourcis}`.

## Couleurs

- Symbole + mot de statut : sémantique (Running = jaune, Done = vert,
  Failed = rouge).
- Compteur `14/19` : gris clair.
- Barre : portion pleine en couleur sémantique, portion vide en gris foncé.
- Raccourcis : touche en cyan, libellé en gris clair, séparateur `·` en gris
  foncé.

Le passage de `DarkGray` à `Gray` pour les libellés règle l'essentiel de la
lisibilité.

## Raccourcis essentiels (le reste reste dans l'aide `?`)

- Running : `↑↓ beam · Tab focus · / cherche · ? aide · q annuler`
- Done / Failed : `↑↓ beam · / cherche · r relancer · ? aide · q quitter`
- Picker : `↑↓ nav · Space sélec · Enter lancer · ? aide · q quitter`
- LogView : `↑↓ scroll · / cherche · ? aide · q quitter`

Les trois contextes reçoivent le même traitement de couleur.

## Barre de progression

`build_progress_bar` (qui renvoyait `[barre] d/t` en une chaîne) est remplacé par
`progress_fill(done, total) -> Option<(usize, usize)>` renvoyant (plein, vide),
ou `None` si `total == 0`. La barre est construite en spans colorés (plein
sémantique, vide gris foncé) et le compteur est placé avant la barre.

## Tests

- `progress_fill` : ratio plein/vide correct, `None` si total nul, plein/vide
  bornés à la largeur.
- Helper de construction des raccourcis : texte concaténé attendu
  (touches, libellés, séparateurs).
- Couleurs et agencement en spans vérifiés à l'œil.

## Hors périmètre

Configuration des couleurs, barre de progression animée, raccourcis
contextuels supplémentaires.
