# Aurora — Design Document

**Date :** 2026-03-04
**Statut :** Validé

## Vue d'ensemble

Aurora est un task runner / build tool open source, alternative à make, just et taskfile. Il utilise un DSL HCL-inspired défini dans un fichier `Beamfile`. Les cibles de build sont appelées **beams**. Aurora intègre une TUI avec une bonne UX pour visualiser l'exécution parallèle des beams et leurs dépendances.

**Projet cobaye :** `omega` (remplacement de `qad` + `qad.yaml.dist`)

---

## Stack technique

| Composant      | Choix                     |
|---------------|--------------------------|
| Langage        | Rust                      |
| Async runtime  | tokio                     |
| TUI            | ratatui                   |
| Parser DSL     | pest (PEG grammar)        |
| Plugin system  | extism (WASM)             |
| Caching        | SHA-256 (sha2 crate)      |
| Distribution   | Binaire statique cross-platform |

---

## Structure du projet (workspace Rust)

```
aurora/
├── Cargo.toml                    # workspace root
├── Beamfile                      # dogfood : Aurora se build avec Aurora
├── crates/
│   ├── aurora/                   # binaire CLI (entrypoint)
│   ├── aurora-core/              # parser, DAG engine, scheduler
│   ├── aurora-tui/               # interface ratatui
│   ├── aurora-executor-api/      # trait + types partagés (host↔plugin WASM)
│   ├── aurora-executor-local/    # executor local (built-in, natif)
│   └── aurora-executor-docker/   # executor docker (built-in, natif)
├── plugins/
│   └── executor-ssh/             # exemple plugin WASM communautaire
└── docs/
    └── plans/
```

---

## DSL Beamfile

### Syntaxe complète

```hcl
# Beamfile

aurora {
  version         = "1"
  default         = "qa"       # beam exécuté par défaut (sans argument)
  max_parallelism = 8          # optionnel, défaut : num_cpus
}

# Variables overridables via CLI (--var key=value)
variable "docker_image" {
  default     = "omega-tools:v1.86.1"
  description = "Docker image for QA tools"
}

# Environnement : évaluation séquentielle (ordre de déclaration)
# Les variables peuvent référencer les précédentes via $VAR
environment {
  CURRENT_BRANCH = shell("git branch --show-current")
  CI_TARGET      = shell("case \"$CURRENT_BRANCH\" in hotfix/*|master) echo origin/master ;; *) echo origin/develop ;; esac")
  LAST_COMMIT    = shell("git merge-base $CI_TARGET HEAD")
  CHANGED_FILES  = shell("git diff --name-status $LAST_COMMIT | awk '$1 ~ /^(A|M|R)/ { print $NF }'")
}

# Beam simple
beam "composer" {
  description = "Install Composer dependencies"

  inputs  = ["composer.json", "composer.lock"]   # caching : hash des inputs
  outputs = ["vendor"]                            # skip si inputs inchangés + outputs présents

  run {
    commands = ["make vendor"]

    executor "docker" {
      image   = var.docker_image
      volumes = [".:/app:rw"]     # optionnel, défaut : CWD monté en /app
    }
  }
}

# Beam avec skip_if (shorthand)
beam "phpstan" {
  description = "Static analysis"
  depends_on  = ["composer"]
  skip_if     = "test -z \"$CHANGED_CONTEXTS\""

  run {
    commands = ["phpstan analyse $CHANGED_CONTEXTS"]
    executor "docker" { image = var.docker_image }
  }
}

# Beam avec condition étendue (any / all)
beam "deptrac" {
  description = "Architecture dependency check"
  depends_on  = ["composer", "depfile"]

  condition {
    any = [
      { shell = "test -n \"$CHANGED_CONTEXTS\"" },
      { shell = "test -n \"$CHANGED_DEPFILE\"" }
    ]
  }

  run {
    commands = [
      "deptrac analyse --config-file=ci/ddd.deptrac.yaml",
      "deptrac analyse --config-file=ci/clean.deptrac.yaml"
    ]
    executor "docker" { image = var.docker_image }
  }
}

# Beam local (sans executor = shell local)
beam "depfile" {
  description = "Generate DDD deptrac config"
  inputs      = ["src/**/*", "bin/generate-ddd-deptrac"]
  outputs     = ["ci/ddd.deptrac.yaml"]

  run {
    commands = ["bin/generate-ddd-deptrac"]
  }
}

# Beam agrégat (orchestration pure, pas de run)
beam "qa" {
  description = "Full QA pipeline"
  depends_on  = ["prepare", "fmt", "lint", "test"]
}
```

### Règles DSL

- **`aurora {}`** : config globale (version, default, max_parallelism)
- **`variable {}`** : valeur par défaut overridable via `--var key=val`
- **`environment {}`** : variables évaluées séquentiellement, disponibles dans tous les beams
- **`beam {}`** :
  - `depends_on` : liste de beams prérequis (DAG explicite)
  - `inputs/outputs` : caching basé sur hash SHA-256
  - `skip_if` : shell shorthand pour condition simple
  - `condition { any/all }` : conditions composées
  - `run {}` : commandes + executor optionnel
  - Beam sans `run` = agrégat (depend_on uniquement)
- **Référence variable** : `var.name` (pas d'interpolation string `${...}`)

---

## DAG Engine & Exécution

### Pipeline d'exécution

```
aurora qa
    │
    ▼
1. Parse Beamfile → AST → BeamGraph (graphe orienté acyclique)
2. Résolution du beam cible → extraction du sous-graphe transitif
3. Détection de cycles → erreur explicite avec le cycle affiché
4. Topological sort (Kahn's algorithm)
5. Pool de tokio tasks : beam "ready" (tous deps complétés) → spawn task
6. Résultats → channel mpsc → TUI renderer (ratatui)
```

### Cycle de vie d'un beam

```
Pending → Ready → Running → Success
                          → Skipped (cached)
                          → Skipped (condition false)
                          → Failed → cascade Cancel sur dépendants directs
```

### Caching

- Hash SHA-256 de chaque fichier `inputs` → digest global → stocké dans `.aurora/cache/<beam-name>.json`
- Format du cache : `{ "inputs_hash": "...", "timestamp": "...", "success": true }`
- Avant exécution : hash actuel == stocké **et** outputs présents → `Skipped (cached)`
- Invalidation automatique : inputs modifiés, outputs manquants
- Invalidation manuelle : `--no-cache`, `--force <beam>`

### Flags CLI

```
aurora [beam]              # exécute un beam (défaut si pas d'arg)
aurora                     # lance le TUI picker
aurora --list              # liste tous les beams
aurora --graph [beam]      # affiche le DAG en ASCII
aurora --var key=val       # override variable
aurora --no-cache          # ignore le cache
aurora --force <beam>      # force réexécution d'un beam spécifique
aurora --dry-run           # affiche les beams qui seraient exécutés
aurora --concurrency N     # override max_parallelism
```

---

## Plugin System (WASM via extism)

### Architecture

Les executors built-in (local, docker) sont compilés **nativement** dans le binaire pour performance et zéro dépendance externe.

L'API plugin WASM est définie dans `aurora-executor-api` et permet à la communauté d'écrire des executors custom :

```rust
// Interface WASM (côté plugin)
pub trait ExecutorPlugin {
    fn name(&self) -> &str;
    fn execute(&self, input: ExecutionInput) -> ExecutionOutput;
}

pub struct ExecutionInput {
    pub commands: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
    pub config: serde_json::Value,  // config spécifique à l'executor
}

pub struct ExecutionOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
```

### Chargement des plugins

- Plugins built-in : compilés statiquement
- Plugins externes : chargés depuis `~/.aurora/plugins/*.wasm`
- Déclaration optionnelle dans le Beamfile :

```hcl
plugin "executor-ssh" {
  source = "~/.aurora/plugins/executor-ssh.wasm"
  # ou : source = "https://registry.aurora.dev/plugins/executor-ssh@1.0.0"
}
```

---

## TUI

### Mode exécution (`aurora qa`)

```
╔══════════════════════════════════════════════════════╗
║  Aurora  ▸ qa                              [00:23]   ║
╠══════════════════════════════════════════════════════╣
║  ✔  depfile          cached      [00:00]             ║
║  ✔  composer         success     [00:12]             ║
║  ⣴  node_modules     running...  [00:18]             ║
║  ─  phpstan          waiting                         ║
║  ─  eslint           waiting                         ║
║  ✕  phpcs            failed      [00:08]  ← focus    ║
╠══════════════════════════════════════════════════════╣
║  [↑↓] naviguer  [Enter] logs  [r] retry  [q] quit   ║
╚══════════════════════════════════════════════════════╝
```

- Spinners animés pour les beams en cours
- Affichage en temps réel (streaming logs dans la vue détail)
- `r` sur un beam Failed → relance uniquement ce beam (+ ses dépendants)
- `q` interrompt l'exécution (SIGTERM gracieux sur les beams en cours)

### Mode picker (`aurora` sans argument)

```
╔══════════════════════════════════════════════════════╗
║  Aurora  ▸ Choisir un beam              [/] search   ║
╠══════════════════════════════════════════════════════╣
║  ▶ qa          Run full QA pipeline                  ║
║    prepare     Prepare dependencies                  ║
║    fmt         Format all files                      ║
║    lint        Run all linters                       ║
║    test        Run all tests                         ║
╠══════════════════════════════════════════════════════╣
║  [Tab] dépendances  [Enter] lancer  [/] recherche   ║
╚══════════════════════════════════════════════════════╝
```

### Visualisation DAG (Tab dans le picker)

```
qa
├── prepare
│   ├── depfile
│   ├── node_modules
│   └── composer
├── fmt
│   ├── fmt-php  (needs: composer)
│   └── fmt-js   (needs: node_modules)
└── lint
    ├── phpstan  (needs: composer)
    └── eslint   (needs: node_modules)
```

---

## Non-objectifs v1

- Exécution distante SSH (executor plugin, v2)
- Support Kubernetes (executor plugin, communauté)
- Registry de plugins en ligne
- Watch mode (re-exécution sur changement de fichiers)
- Multi-Beamfile / workspace monorepo
- Import / include de Beamfiles externes

---

## Roadmap envisagée

| Version | Focus |
|---------|-------|
| v0.1    | Core : parser + DAG + executor local + TUI basique |
| v0.2    | Executor Docker + caching |
| v0.3    | TUI complète (picker + DAG view + logs streaming) |
| v0.4    | Plugin system WASM (extism) |
| v1.0    | Distribution open source, remplacement de qad sur omega |
