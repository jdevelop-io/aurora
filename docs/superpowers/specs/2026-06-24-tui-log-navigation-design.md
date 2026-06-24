# TUI : navigation et repères dans les logs

Date : 2026-06-24
Crate : `aurora-tui`

## Objectif

Améliorer la navigation clavier dans les logs et donner un repère visuel de
position : scroll demi-page, aller en haut/bas, scrollbar, indicateur de
position. Pas de capture souris (on garde l'émulation du terminal, qui préserve
la sélection de texte native).

## Préalable : modèle de scroll en lignes visuelles

Le rendu enroule les lignes longues (wrap), donc une ligne logique occupe
plusieurs lignes à l'écran. Le scroll doit s'exprimer en lignes visuelles, avec
une vraie borne basse, sinon la scrollbar et l'indicateur affichent une position
fausse.

`LogViewState` est corrigé :

- `max_scroll = total_visual - hauteur_panneau` ; « bas » affiche le dernier
  écran complet (et non une seule ligne en haut).
- méthodes centralisées : `scroll_lines(delta)`, `scroll_to_top()`,
  `scroll_to_bottom(total_visual, hauteur)`, `auto_scroll(total_visual, hauteur)`.
- la boucle passe partout `beam.total_visual_rows(largeur)` et la hauteur réelle
  du panneau (via `log_panel_width`, on ajoute le calcul de hauteur).

Les tests de scroll existants sont réécrits pour cette sémantique. Les tests de
focus et de sélection ne changent pas.

## Items

### 1. Scroll demi-page

`Ctrl+U` / `Ctrl+D` scrollent d'une demi-hauteur de panneau. Gérés dans la
boucle (besoin des modificateurs) via `scroll_lines(±hauteur/2)`.

### 2. `g` début / `G` fin

`g` ajoute « aller en haut » (verrouille le scroll en haut). `G` garde « aller
en bas » et réactive l'auto-scroll.

### 3. Scrollbar logs

Widget `Scrollbar` de ratatui sur le bord droit du panneau, affiché seulement si
le contenu déborde. `content_length = total_visual`, `viewport = hauteur`,
`position = scroll`.

### 4. Indicateur de position

Dans le titre du panneau : « X/Y » en lignes logiques (plus parlant que le
visuel). Nouvelle méthode `BeamView::logical_line_at_visual(offset, largeur)`
pour retrouver la ligne logique en haut de l'écran.

## Cohérence recherche

Le saut de recherche borne son offset à `max_scroll`, pour que le match reste
visible même près de la fin des logs.

## Aide

Le popup d'aide (`?`) est complété avec `g`, `Ctrl+U`, `Ctrl+D`.

## Tests

- `scroll_lines` : clamp haut/bas, verrouillage à la montée, déverrouillage en
  bas.
- `scroll_to_bottom` / `auto_scroll` : position = `total_visual - hauteur`.
- `logical_line_at_visual` : bonne ligne logique pour un offset donné, avec wrap.
- Réécriture des tests de scroll existants pour le modèle visuel.

Scrollbar et titre restent vérifiés à l'œil.

## Hors périmètre

Capture souris, scrollbar sur la liste des beams, recherche par regex.
