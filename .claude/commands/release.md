---
description: Bump la version, commit, tag et push pour déclencher le workflow de release
argument-hint: <X.Y.Z | patch | minor | major>
allowed-tools: Bash, Read, Edit
---

Tu prépares et déclenches une release d'Aurora. Argument fourni : `$ARGUMENTS`.

La version vit dans `Cargo.toml` racine (`[workspace.package] version`), tous les
crates en héritent. Le workflow `.github/workflows/release.yml` se déclenche au push
d'un tag `v*`.

Exécute les étapes dans l'ordre. **Arrête-toi immédiatement et explique** si une
vérification échoue, sans rien committer ni pousser.

## 1. Calculer la version cible

- Lis la version actuelle : la ligne `version = "X.Y.Z"` en début de `Cargo.toml`.
- Selon `$ARGUMENTS` :
  - `X.Y.Z` ou `vX.Y.Z` (semver exact) → utilise ce numéro (sans le `v`).
  - `patch` → incrémente Z. `minor` → incrémente Y, Z=0. `major` → incrémente X, Y=0, Z=0.
  - vide ou autre → **arrête-toi** et rappelle l'usage : `/release <X.Y.Z | patch | minor | major>`.
- Note `NEW` (ex. `0.3.0`) et `TAG = vNEW` (ex. `v0.3.0`). Vérifie que `NEW` est bien
  strictement supérieur à la version actuelle, sinon arrête-toi.

## 2. Garde-fous (tout doit passer avant de continuer)

```bash
git status --porcelain        # doit être vide (arbre propre)
git rev-parse --abbrev-ref HEAD   # doit être "main"
git fetch origin --tags --quiet
git rev-list --left-right --count main...origin/main   # main ne doit pas être en retard
git tag -l "$TAG"             # doit être vide (tag local inexistant)
git ls-remote --tags origin "$TAG"   # doit être vide (tag distant inexistant)
```

Si l'arbre n'est pas propre, si on n'est pas sur `main`, si `main` est en retard sur
`origin/main`, ou si le tag existe déjà : arrête-toi et explique.

## 3. Bumper les fichiers

- `Cargo.toml` : remplace la ligne `version = "<actuelle>"` (début de ligne) par `version = "NEW"`.
- `Cargo.lock` : resynchronise via cargo (étape 4, le build met à jour les entrées `aurora*`).
- `README.md` :
  - Section `## Status` : la mention `Project at v…` doit pointer sur `vNEW`.
  - Exemple d'install : `AURORA_VERSION=v…` doit pointer sur `vNEW`.

```bash
sed -i -E 's/^version = "[0-9]+\.[0-9]+\.[0-9]+"/version = "NEW"/' Cargo.toml
sed -i -E 's/(Project at v)[0-9]+\.[0-9]+(\.[0-9]+)?/\1NEW/' README.md
sed -i -E 's/(AURORA_VERSION=v)[0-9]+\.[0-9]+\.[0-9]+/\1NEW/' README.md
```

(Remplace `NEW` par le numéro réel dans les commandes.)

## 4. Valider (cargo)

```bash
cargo build --release --quiet   # met aussi Cargo.lock à jour avec NEW
cargo test --quiet
```

Si le build ou les tests échouent : arrête-toi, n'engage rien. Vérifie ensuite que les
6 entrées `aurora*` de `Cargo.lock` sont bien passées à `NEW`.

## 5. Commit, tag, push

```bash
git add Cargo.toml Cargo.lock README.md
git commit -m ":bookmark: chore(release): vNEW"
git tag -a "vNEW" -m "vNEW"
git push origin main
git push origin "vNEW"
```

## 6. Rapport

Confirme la version publiée et donne les liens de suivi :
- Workflow : `https://github.com/jdevelop-io/aurora/actions`
- Release : `https://github.com/jdevelop-io/aurora/releases/tag/vNEW`

N'ajoute aucune attribution Claude/Anthropic dans le commit ou le tag.
