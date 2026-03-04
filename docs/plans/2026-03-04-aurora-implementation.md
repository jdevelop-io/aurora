# Aurora — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build Aurora, a Rust task runner with HCL-inspired Beamfile DSL, parallel DAG execution, input/output caching, local + Docker executors, WASM plugin system, and a ratatui TUI with execution view and beam picker.

**Architecture:** Rust workspace with 6 crates: `aurora-core` (parser + DAG + scheduler + cache), `aurora-executor-api` (shared traits), `aurora-executor-local`, `aurora-executor-docker`, `aurora-tui`, and `aurora` (CLI binary). Async runtime: tokio. Parser: pest PEG grammar. Plugin host: extism (WASM).

**Tech Stack:** Rust, tokio, ratatui + crossterm, pest + pest_derive, petgraph, sha2, glob, serde_json, clap, extism, bollard (or docker CLI shim), anyhow, thiserror

---

## Task 1: Workspace Rust setup

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/aurora-core/Cargo.toml`
- Create: `crates/aurora-core/src/lib.rs`
- Create: `crates/aurora-executor-api/Cargo.toml`
- Create: `crates/aurora-executor-api/src/lib.rs`
- Create: `crates/aurora-executor-local/Cargo.toml`
- Create: `crates/aurora-executor-local/src/lib.rs`
- Create: `crates/aurora-executor-docker/Cargo.toml`
- Create: `crates/aurora-executor-docker/src/lib.rs`
- Create: `crates/aurora-tui/Cargo.toml`
- Create: `crates/aurora-tui/src/lib.rs`
- Create: `crates/aurora/Cargo.toml`
- Create: `crates/aurora/src/main.rs`

**Step 1: Créer le workspace Cargo.toml**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/aurora",
    "crates/aurora-core",
    "crates/aurora-tui",
    "crates/aurora-executor-api",
    "crates/aurora-executor-local",
    "crates/aurora-executor-docker",
]

[workspace.dependencies]
anyhow = "1"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

**Step 2: Créer chaque crate**

```bash
mkdir -p crates/aurora/src \
         crates/aurora-core/src \
         crates/aurora-tui/src \
         crates/aurora-executor-api/src \
         crates/aurora-executor-local/src \
         crates/aurora-executor-docker/src
```

`crates/aurora-core/Cargo.toml` :
```toml
[package]
name = "aurora-core"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
pest = "2"
pest_derive = "2"
petgraph = "0.6"
sha2 = "0.10"
glob = "0.3"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
```

`crates/aurora-executor-api/Cargo.toml` :
```toml
[package]
name = "aurora-executor-api"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
```

`crates/aurora-executor-local/Cargo.toml` :
```toml
[package]
name = "aurora-executor-local"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = { workspace = true }
tokio = { workspace = true }
aurora-executor-api = { path = "../aurora-executor-api" }
```

`crates/aurora-executor-docker/Cargo.toml` :
```toml
[package]
name = "aurora-executor-docker"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = { workspace = true }
tokio = { workspace = true }
aurora-executor-api = { path = "../aurora-executor-api" }
```

`crates/aurora-tui/Cargo.toml` :
```toml
[package]
name = "aurora-tui"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = { workspace = true }
ratatui = "0.29"
crossterm = "0.28"
tokio = { workspace = true }
aurora-core = { path = "../aurora-core" }
```

`crates/aurora/Cargo.toml` :
```toml
[package]
name = "aurora"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "aurora"
path = "src/main.rs"

[dependencies]
anyhow = { workspace = true }
tokio = { workspace = true }
clap = { version = "4", features = ["derive"] }
aurora-core = { path = "../aurora-core" }
aurora-tui = { path = "../aurora-tui" }
aurora-executor-api = { path = "../aurora-executor-api" }
aurora-executor-local = { path = "../aurora-executor-local" }
aurora-executor-docker = { path = "../aurora-executor-docker" }
```

Stubs initiaux pour que ça compile :
```rust
// crates/aurora-core/src/lib.rs
pub mod ast;
pub mod parser;
pub mod dag;
pub mod cache;
pub mod scheduler;
```

```rust
// crates/aurora/src/main.rs
fn main() {
    println!("aurora v0.1.0");
}
```

**Step 3: Vérifier que le workspace compile**

```bash
cargo check --workspace
```
Attendu : `Finished` sans erreurs (juste des warnings "module not found" si les sous-modules ne sont pas encore créés — OK pour l'instant).

**Step 4: Commit**

```bash
git add -A
git commit -m "✨ feat: init Aurora Rust workspace with crate structure"
```

---

## Task 2: AST types (aurora-core)

**Files:**
- Create: `crates/aurora-core/src/ast.rs`
- Create: `crates/aurora-core/tests/ast_test.rs`

**Step 1: Écrire le test de construction d'AST**

```rust
// crates/aurora-core/tests/ast_test.rs
use aurora_core::ast::*;

#[test]
fn test_beam_with_all_fields() {
    let beam = Beam {
        name: "phpstan".to_string(),
        description: Some("Static analysis".to_string()),
        depends_on: vec!["composer".to_string()],
        inputs: vec!["src/**/*.php".to_string()],
        outputs: vec![],
        skip_if: Some("test -z \"$CHANGED\"".to_string()),
        condition: None,
        run: Some(Run {
            commands: vec!["phpstan analyse".to_string()],
            executor: Some(ExecutorConfig {
                name: "docker".to_string(),
                config: [("image".to_string(), "omega-tools:v1".to_string())]
                    .into_iter().collect(),
            }),
        }),
    };
    assert_eq!(beam.name, "phpstan");
    assert!(beam.skip_if.is_some());
    assert!(beam.run.as_ref().unwrap().executor.is_some());
}

#[test]
fn test_aggregate_beam() {
    let beam = Beam {
        name: "qa".to_string(),
        description: Some("Full QA".to_string()),
        depends_on: vec!["lint".to_string(), "test".to_string()],
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: None,
    };
    assert!(beam.run.is_none());
    assert_eq!(beam.depends_on.len(), 2);
}
```

**Step 2: Vérifier que le test ne compile pas**

```bash
cargo test -p aurora-core 2>&1 | head -20
```
Attendu : erreur de compilation (module `ast` vide)

**Step 3: Implémenter l'AST**

```rust
// crates/aurora-core/src/ast.rs
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BeamFile {
    pub config: Option<AuroraConfig>,
    pub variables: Vec<Variable>,
    pub environment: Option<Environment>,
    pub beams: Vec<Beam>,
}

#[derive(Debug, Clone)]
pub struct AuroraConfig {
    pub version: String,
    pub default: Option<String>,
    pub max_parallelism: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub name: String,
    pub default: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Environment {
    /// Évaluées séquentiellement dans l'ordre de déclaration
    pub vars: Vec<EnvVar>,
}

#[derive(Debug, Clone)]
pub struct EnvVar {
    pub name: String,
    pub value: EnvValue,
}

#[derive(Debug, Clone)]
pub enum EnvValue {
    Literal(String),
    Shell(String),
}

#[derive(Debug, Clone)]
pub struct Beam {
    pub name: String,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub skip_if: Option<String>,
    pub condition: Option<Condition>,
    pub run: Option<Run>,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub op: ConditionOp,
    pub clauses: Vec<ConditionClause>,
}

#[derive(Debug, Clone)]
pub enum ConditionOp {
    Any,
    All,
}

#[derive(Debug, Clone)]
pub enum ConditionClause {
    Shell(String),
}

#[derive(Debug, Clone)]
pub struct Run {
    pub commands: Vec<String>,
    pub executor: Option<ExecutorConfig>,
}

#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub name: String,
    pub config: HashMap<String, String>,
}
```

**Step 4: Lancer les tests**

```bash
cargo test -p aurora-core 2>&1
```
Attendu : `2 passed`

**Step 5: Commit**

```bash
git add crates/aurora-core/src/ast.rs crates/aurora-core/tests/ast_test.rs
git commit -m "✨ feat(core): add BeamFile AST types"
```

---

## Task 3: Parser pest (aurora-core)

**Files:**
- Create: `crates/aurora-core/src/parser/aurora.pest`
- Create: `crates/aurora-core/src/parser/mod.rs`
- Modify: `crates/aurora-core/src/lib.rs`
- Create: `crates/aurora-core/tests/parser_test.rs`

**Step 1: Écrire les tests parser**

```rust
// crates/aurora-core/tests/parser_test.rs
use aurora_core::parser::parse;

#[test]
fn test_parse_minimal_beamfile() {
    let input = r#"
beam "hello" {
  description = "Say hello"
  run {
    commands = ["echo hello"]
  }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.beams.len(), 1);
    assert_eq!(bf.beams[0].name, "hello");
    assert_eq!(bf.beams[0].description.as_deref(), Some("Say hello"));
    let run = bf.beams[0].run.as_ref().unwrap();
    assert_eq!(run.commands, vec!["echo hello"]);
}

#[test]
fn test_parse_aurora_block() {
    let input = r#"
aurora {
  version = "1"
  default = "qa"
}

beam "qa" {
  depends_on = ["lint", "test"]
}
"#;
    let bf = parse(input).unwrap();
    let cfg = bf.config.as_ref().unwrap();
    assert_eq!(cfg.version, "1");
    assert_eq!(cfg.default.as_deref(), Some("qa"));
    assert_eq!(bf.beams[0].depends_on, vec!["lint", "test"]);
}

#[test]
fn test_parse_environment_block() {
    let input = r#"
environment {
  BRANCH = shell("git branch --show-current")
  MODE   = "production"
}

beam "b" {
  run { commands = ["echo $MODE"] }
}
"#;
    let bf = parse(input).unwrap();
    let env = bf.environment.as_ref().unwrap();
    assert_eq!(env.vars.len(), 2);
    matches!(&env.vars[0].value, aurora_core::ast::EnvValue::Shell(_));
    matches!(&env.vars[1].value, aurora_core::ast::EnvValue::Literal(_));
}

#[test]
fn test_parse_variable_block() {
    let input = r#"
variable "image" {
  default     = "ubuntu:22.04"
  description = "Docker image"
}

beam "b" {
  run { commands = ["echo"] }
}
"#;
    let bf = parse(input).unwrap();
    assert_eq!(bf.variables.len(), 1);
    assert_eq!(bf.variables[0].name, "image");
    assert_eq!(bf.variables[0].default, "ubuntu:22.04");
}

#[test]
fn test_parse_executor_docker() {
    let input = r#"
beam "phpstan" {
  depends_on = ["composer"]
  skip_if    = "test -z \"$FILES\""
  run {
    commands = ["phpstan analyse"]
    executor "docker" {
      image = "omega-tools:v1"
    }
  }
}
"#;
    let bf = parse(input).unwrap();
    let beam = &bf.beams[0];
    assert_eq!(beam.skip_if.as_deref(), Some("test -z \"$FILES\""));
    let exec = beam.run.as_ref().unwrap().executor.as_ref().unwrap();
    assert_eq!(exec.name, "docker");
    assert_eq!(exec.config.get("image").unwrap(), "omega-tools:v1");
}

#[test]
fn test_parse_inputs_outputs_caching() {
    let input = r#"
beam "composer" {
  inputs  = ["composer.json", "composer.lock"]
  outputs = ["vendor"]
  run {
    commands = ["composer install"]
  }
}
"#;
    let bf = parse(input).unwrap();
    let beam = &bf.beams[0];
    assert_eq!(beam.inputs, vec!["composer.json", "composer.lock"]);
    assert_eq!(beam.outputs, vec!["vendor"]);
}

#[test]
fn test_parse_condition_any() {
    let input = r#"
beam "deptrac" {
  condition {
    any = [
      { shell = "test -n \"$A\"" },
      { shell = "test -n \"$B\"" }
    ]
  }
  run { commands = ["deptrac"] }
}
"#;
    let bf = parse(input).unwrap();
    let cond = bf.beams[0].condition.as_ref().unwrap();
    matches!(cond.op, aurora_core::ast::ConditionOp::Any);
    assert_eq!(cond.clauses.len(), 2);
}

#[test]
fn test_parse_empty_aggregate_beam() {
    let input = r#"
beam "all" {
  depends_on = ["a", "b", "c"]
}
"#;
    let bf = parse(input).unwrap();
    assert!(bf.beams[0].run.is_none());
}

#[test]
fn test_parse_error_on_invalid_syntax() {
    let input = "beam { broken }";
    assert!(parse(input).is_err());
}
```

**Step 2: Vérifier que les tests ne compilent pas**

```bash
cargo test -p aurora-core 2>&1 | head -5
```
Attendu : erreur "cannot find function `parse`"

**Step 3: Écrire la grammaire pest**

```pest
// crates/aurora-core/src/parser/aurora.pest

// Whitespace et commentaires silencieux
WHITESPACE = _{ " " | "\t" | "\r" | "\n" }
COMMENT    = _{ "#" ~ (!"\n" ~ ANY)* ~ "\n"? }

// Primitives
ident        = @{ ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "_" | "-")* }
string_inner = @{ (!"\"" ~ !"\\" ~ ANY | "\\" ~ ANY)* }
string       = ${ "\"" ~ string_inner ~ "\"" }
number       = @{ ASCII_DIGIT+ }

// Listes de strings : ["a", "b"]
string_list = { "[" ~ (string ~ ("," ~ string)*)? ~ "]" }

// === Blocks top-level ===
beamfile = { SOI ~ block* ~ EOI }
block    = { aurora_block | variable_block | environment_block | beam_block }

// aurora { version = "1"  default = "qa"  max_parallelism = 8 }
aurora_block = { "aurora" ~ "{" ~ aurora_field* ~ "}" }
aurora_field = { aurora_version | aurora_default | aurora_parallelism }
aurora_version     = { "version"         ~ "=" ~ string }
aurora_default     = { "default"         ~ "=" ~ string }
aurora_parallelism = { "max_parallelism" ~ "=" ~ number }

// variable "name" { default = "val"  description = "..." }
variable_block = { "variable" ~ string ~ "{" ~ variable_field* ~ "}" }
variable_field = { var_default | var_description }
var_default     = { "default"     ~ "=" ~ string }
var_description = { "description" ~ "=" ~ string }

// environment { NAME = shell("...") | NAME = "..." }
environment_block = { "environment" ~ "{" ~ env_var* ~ "}" }
env_var           = { ident ~ "=" ~ env_value }
env_value         = { shell_call | string }
shell_call        = { "shell" ~ "(" ~ string ~ ")" }

// beam "name" { ... }
beam_block  = { "beam" ~ string ~ "{" ~ beam_field* ~ "}" }
beam_field  = {
    beam_description |
    beam_depends_on  |
    beam_inputs      |
    beam_outputs     |
    beam_skip_if     |
    beam_condition   |
    beam_run
}
beam_description = { "description" ~ "=" ~ string }
beam_depends_on  = { "depends_on"  ~ "=" ~ string_list }
beam_inputs      = { "inputs"      ~ "=" ~ string_list }
beam_outputs     = { "outputs"     ~ "=" ~ string_list }
beam_skip_if     = { "skip_if"     ~ "=" ~ string }

// condition { any = [...] | all = [...] }
beam_condition      = { "condition" ~ "{" ~ condition_body ~ "}" }
condition_body      = { condition_any | condition_all }
condition_any       = { "any" ~ "=" ~ "[" ~ condition_clause* ~ "]" }
condition_all       = { "all" ~ "=" ~ "[" ~ condition_clause* ~ "]" }
condition_clause    = { "{" ~ clause_shell ~ "}" ~ ","? }
clause_shell        = { "shell" ~ "=" ~ string }

// run { commands = [...]  executor "name" { ... } }
beam_run        = { "run" ~ "{" ~ run_field* ~ "}" }
run_field       = { run_commands | run_executor }
run_commands    = { "commands" ~ "=" ~ string_list }
run_executor    = { "executor" ~ string ~ "{" ~ executor_field* ~ "}" }
executor_field  = { ident ~ "=" ~ (string | var_ref) }
var_ref         = @{ "var." ~ ident }
```

**Step 4: Écrire le parser (conversion Pairs → AST)**

```rust
// crates/aurora-core/src/parser/mod.rs
use crate::ast::*;
use anyhow::{bail, Context, Result};
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;

#[derive(Parser)]
#[grammar = "parser/aurora.pest"]
struct AuroraParser;

pub fn parse(input: &str) -> Result<BeamFile> {
    let pairs = AuroraParser::parse(Rule::beamfile, input)
        .context("Failed to parse Beamfile")?;

    let mut bf = BeamFile {
        config: None,
        variables: vec![],
        environment: None,
        beams: vec![],
    };

    for pair in pairs {
        match pair.as_rule() {
            Rule::beamfile => {
                for block in pair.into_inner() {
                    match block.as_rule() {
                        Rule::block => parse_block(block.into_inner().next().unwrap(), &mut bf)?,
                        Rule::EOI => {}
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(bf)
}

fn parse_block(pair: Pair<Rule>, bf: &mut BeamFile) -> Result<()> {
    match pair.as_rule() {
        Rule::aurora_block    => bf.config = Some(parse_aurora_block(pair)?),
        Rule::variable_block  => bf.variables.push(parse_variable_block(pair)?),
        Rule::environment_block => bf.environment = Some(parse_environment_block(pair)?),
        Rule::beam_block      => bf.beams.push(parse_beam_block(pair)?),
        _ => {}
    }
    Ok(())
}

fn parse_aurora_block(pair: Pair<Rule>) -> Result<AuroraConfig> {
    let mut cfg = AuroraConfig { version: "1".to_string(), default: None, max_parallelism: None };
    for field in pair.into_inner() {
        match field.as_rule() {
            Rule::aurora_version     => cfg.version = unquote(field.into_inner().next().unwrap()),
            Rule::aurora_default     => cfg.default = Some(unquote(field.into_inner().next().unwrap())),
            Rule::aurora_parallelism => cfg.max_parallelism = Some(field.into_inner().next().unwrap().as_str().parse()?),
            _ => {}
        }
    }
    Ok(cfg)
}

fn parse_variable_block(pair: Pair<Rule>) -> Result<Variable> {
    let mut inner = pair.into_inner();
    let name = unquote(inner.next().unwrap());
    let mut var = Variable { name, default: String::new(), description: None };
    for field in inner {
        match field.as_rule() {
            Rule::var_default     => var.default = unquote(field.into_inner().next().unwrap()),
            Rule::var_description => var.description = Some(unquote(field.into_inner().next().unwrap())),
            _ => {}
        }
    }
    Ok(var)
}

fn parse_environment_block(pair: Pair<Rule>) -> Result<Environment> {
    let mut vars = vec![];
    for var in pair.into_inner() {
        if var.as_rule() == Rule::env_var {
            let mut inner = var.into_inner();
            let name = inner.next().unwrap().as_str().to_string();
            let val_pair = inner.next().unwrap();
            let value = match val_pair.as_rule() {
                Rule::shell_call => EnvValue::Shell(unquote(val_pair.into_inner().next().unwrap())),
                Rule::string     => EnvValue::Literal(unquote(val_pair)),
                _ => bail!("unexpected env_value"),
            };
            vars.push(EnvVar { name, value });
        }
    }
    Ok(Environment { vars })
}

fn parse_beam_block(pair: Pair<Rule>) -> Result<Beam> {
    let mut inner = pair.into_inner();
    let name = unquote(inner.next().unwrap());
    let mut beam = Beam {
        name,
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: None,
    };
    for field in inner {
        match field.as_rule() {
            Rule::beam_description => beam.description = Some(unquote(field.into_inner().next().unwrap())),
            Rule::beam_depends_on  => beam.depends_on = parse_string_list(field.into_inner().next().unwrap()),
            Rule::beam_inputs      => beam.inputs = parse_string_list(field.into_inner().next().unwrap()),
            Rule::beam_outputs     => beam.outputs = parse_string_list(field.into_inner().next().unwrap()),
            Rule::beam_skip_if     => beam.skip_if = Some(unquote(field.into_inner().next().unwrap())),
            Rule::beam_condition   => beam.condition = Some(parse_condition(field)?),
            Rule::beam_run         => beam.run = Some(parse_run(field)?),
            _ => {}
        }
    }
    Ok(beam)
}

fn parse_condition(pair: Pair<Rule>) -> Result<Condition> {
    let body = pair.into_inner().next().unwrap();
    let (op, clauses_pair) = match body.as_rule() {
        Rule::condition_any => (ConditionOp::Any, body),
        Rule::condition_all => (ConditionOp::All, body),
        _ => bail!("unexpected condition body"),
    };
    let clauses = clauses_pair.into_inner()
        .filter(|p| p.as_rule() == Rule::condition_clause)
        .map(|clause| {
            let shell_str = clause.into_inner().next().unwrap()
                .into_inner().next().unwrap();
            ConditionClause::Shell(unquote(shell_str))
        })
        .collect();
    Ok(Condition { op, clauses })
}

fn parse_run(pair: Pair<Rule>) -> Result<Run> {
    let mut commands = vec![];
    let mut executor = None;
    for field in pair.into_inner() {
        match field.as_rule() {
            Rule::run_commands => {
                commands = parse_string_list(field.into_inner().next().unwrap());
            }
            Rule::run_executor => {
                let mut inner = field.into_inner();
                let name = unquote(inner.next().unwrap());
                let mut config = HashMap::new();
                for kv in inner {
                    if kv.as_rule() == Rule::executor_field {
                        let mut kv_inner = kv.into_inner();
                        let key = kv_inner.next().unwrap().as_str().to_string();
                        let val = kv_inner.next().unwrap();
                        let value = match val.as_rule() {
                            Rule::string  => unquote(val),
                            Rule::var_ref => val.as_str().to_string(), // "var.name" — résolu plus tard
                            _ => val.as_str().to_string(),
                        };
                        config.insert(key, value);
                    }
                }
                executor = Some(ExecutorConfig { name, config });
            }
            _ => {}
        }
    }
    Ok(Run { commands, executor })
}

fn parse_string_list(pair: Pair<Rule>) -> Vec<String> {
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::string)
        .map(unquote)
        .collect()
}

/// Retire les guillemets d'une paire Rule::string, et gère les échappements
fn unquote(pair: Pair<Rule>) -> String {
    // string_inner ou string selon contexte
    let raw = if pair.as_rule() == Rule::string {
        pair.into_inner().next().map(|p| p.as_str()).unwrap_or("")
    } else {
        pair.as_str()
    };
    raw.replace("\\\"", "\"")
       .replace("\\\\", "\\")
       .replace("\\n", "\n")
       .replace("\\t", "\t")
}
```

**Step 5: Exposer le parser depuis lib.rs**

```rust
// crates/aurora-core/src/lib.rs
pub mod ast;
pub mod parser;
pub mod dag;
pub mod cache;
pub mod scheduler;
```

Et créer des stubs vides pour `dag`, `cache`, `scheduler` :
```bash
touch crates/aurora-core/src/dag.rs
touch crates/aurora-core/src/cache.rs
touch crates/aurora-core/src/scheduler.rs
```

**Step 6: Lancer les tests**

```bash
cargo test -p aurora-core --test parser_test 2>&1
```
Attendu : `9 passed` (ajuster la grammaire si certains tests échouent)

**Step 7: Commit**

```bash
git add crates/aurora-core/src/parser/ crates/aurora-core/tests/parser_test.rs crates/aurora-core/src/lib.rs
git commit -m "✨ feat(core): implement pest PEG parser for Beamfile DSL"
```

---

## Task 4: DAG engine (aurora-core)

**Files:**
- Modify: `crates/aurora-core/src/dag.rs`
- Create: `crates/aurora-core/tests/dag_test.rs`
- Add dependency: `petgraph` dans aurora-core/Cargo.toml (déjà présent)

**Step 1: Écrire les tests DAG**

```rust
// crates/aurora-core/tests/dag_test.rs
use aurora_core::dag::{BeamGraph, DagError};

#[test]
fn test_topological_levels_simple() {
    // qa -> [lint, test]
    // lint -> [composer]
    // test -> [composer]
    // composer -> []
    let deps = vec![
        ("qa",       vec!["lint", "test"]),
        ("lint",     vec!["composer"]),
        ("test",     vec!["composer"]),
        ("composer", vec![]),
    ];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let levels = graph.execution_levels("qa").unwrap();

    // Niveau 0 : composer
    // Niveau 1 : lint, test (en parallèle)
    // Niveau 2 : qa
    assert_eq!(levels.len(), 3);
    assert_eq!(levels[0], vec!["composer"]);
    let mut l1 = levels[1].clone(); l1.sort();
    assert_eq!(l1, vec!["lint", "test"]);
    assert_eq!(levels[2], vec!["qa"]);
}

#[test]
fn test_cycle_detection() {
    let deps = vec![
        ("a", vec!["b"]),
        ("b", vec!["c"]),
        ("c", vec!["a"]),
    ];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let result = graph.execution_levels("a");
    assert!(matches!(result, Err(DagError::Cycle(_))));
}

#[test]
fn test_unknown_dependency() {
    let deps = vec![
        ("qa", vec!["nonexistent"]),
    ];
    let result = BeamGraph::from_deps(deps);
    assert!(matches!(result, Err(DagError::UnknownBeam(_))));
}

#[test]
fn test_transitive_dependencies() {
    let deps = vec![
        ("qa",       vec!["lint"]),
        ("lint",     vec!["composer"]),
        ("composer", vec![]),
        ("unrelated", vec![]),
    ];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let mut transitive = graph.transitive_deps("qa");
    transitive.sort();
    // qa doit inclure lint et composer, mais pas "unrelated"
    assert_eq!(transitive, vec!["composer", "lint", "qa"]);
}

#[test]
fn test_dependents_of() {
    let deps = vec![
        ("qa",       vec!["lint"]),
        ("lint",     vec!["composer"]),
        ("composer", vec![]),
    ];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let mut dependents = graph.direct_dependents("composer");
    dependents.sort();
    assert_eq!(dependents, vec!["lint"]);
}
```

**Step 2: Implémenter le DAG**

```rust
// crates/aurora-core/src/dag.rs
use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::Bfs;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DagError {
    #[error("Unknown beam referenced: {0}")]
    UnknownBeam(String),
    #[error("Cycle detected involving: {0}")]
    Cycle(String),
}

pub struct BeamGraph {
    graph: DiGraph<String, ()>,
    index: HashMap<String, NodeIndex>,
}

impl BeamGraph {
    /// Construit le graphe à partir d'une liste (nom, dépendances).
    /// Vérifie que toutes les dépendances existent.
    pub fn from_deps<S: AsRef<str>>(
        deps: Vec<(S, Vec<S>)>,
    ) -> Result<Self, DagError> {
        let mut graph = DiGraph::new();
        let mut index = HashMap::new();

        // 1. Ajouter tous les noeuds
        for (name, _) in &deps {
            let idx = graph.add_node(name.as_ref().to_string());
            index.insert(name.as_ref().to_string(), idx);
        }

        // 2. Ajouter les arêtes (dep → beam, sens : dep must run before beam)
        for (name, beam_deps) in &deps {
            let src = *index.get(name.as_ref()).unwrap();
            for dep in beam_deps {
                let dst = index.get(dep.as_ref())
                    .ok_or_else(|| DagError::UnknownBeam(dep.as_ref().to_string()))?;
                // dep → name (dep runs before name)
                graph.add_edge(*dst, src, ());
            }
        }

        Ok(BeamGraph { graph, index })
    }

    /// Retourne les niveaux d'exécution parallèles pour un beam cible.
    /// Chaque niveau contient les beams exécutables simultanément.
    pub fn execution_levels(&self, root: &str) -> Result<Vec<Vec<String>>, DagError> {
        // Extraire le sous-graphe transitif
        let nodes = self.transitive_deps(root);

        // Vérifier les cycles sur le sous-graphe
        let sub = self.subgraph(&nodes);
        if is_cyclic_directed(&sub.graph) {
            return Err(DagError::Cycle(root.to_string()));
        }

        // Tri topologique avec petgraph
        let sorted = toposort(&sub.graph, None)
            .map_err(|_| DagError::Cycle(root.to_string()))?;

        // Calculer les niveaux (algorithme longest-path)
        let mut levels: HashMap<NodeIndex, usize> = HashMap::new();
        for &node in &sorted {
            let max_dep_level = sub.graph
                .neighbors_directed(node, petgraph::Direction::Incoming)
                .filter_map(|dep| levels.get(&dep))
                .max()
                .copied()
                .map(|l| l + 1)
                .unwrap_or(0);
            levels.insert(node, max_dep_level);
        }

        let max_level = levels.values().copied().max().unwrap_or(0);
        let mut result: Vec<Vec<String>> = vec![vec![]; max_level + 1];
        for (node, level) in &levels {
            result[*level].push(sub.graph[*node].clone());
        }

        Ok(result)
    }

    /// Retourne tous les beams dans le sous-graphe transitif (dépendances + root).
    pub fn transitive_deps(&self, root: &str) -> Vec<String> {
        let root_idx = match self.index.get(root) {
            Some(idx) => *idx,
            None => return vec![],
        };

        // BFS inverse (depuis root, suivre les arêtes entrantes)
        let mut bfs = Bfs::new(&self.graph, root_idx);
        let mut result = vec![];
        // BFS ne suit que les arêtes sortantes de petgraph.
        // On doit utiliser une approche custom : reverse DFS/BFS
        fn dfs(g: &DiGraph<String, ()>, node: NodeIndex, visited: &mut Vec<NodeIndex>) {
            if visited.contains(&node) { return; }
            visited.push(node);
            for dep in g.neighbors_directed(node, petgraph::Direction::Incoming) {
                dfs(g, dep, visited);
            }
        }
        let mut visited = vec![];
        dfs(&self.graph, root_idx, &mut visited);
        visited.iter().map(|&idx| self.graph[idx].clone()).collect()
    }

    /// Dépendants directs d'un beam (beams qui en dépendent).
    pub fn direct_dependents(&self, beam: &str) -> Vec<String> {
        let idx = match self.index.get(beam) {
            Some(idx) => *idx,
            None => return vec![],
        };
        self.graph
            .neighbors_directed(idx, petgraph::Direction::Outgoing)
            .map(|n| self.graph[n].clone())
            .collect()
    }

    fn subgraph(&self, nodes: &[String]) -> BeamGraph {
        let mut new_graph = DiGraph::new();
        let mut new_index = HashMap::new();
        for name in nodes {
            let idx = new_graph.add_node(name.clone());
            new_index.insert(name.clone(), idx);
        }
        for name in nodes {
            if let Some(&src) = self.index.get(name) {
                for dep in self.graph.neighbors_directed(src, petgraph::Direction::Incoming) {
                    let dep_name = &self.graph[dep];
                    if let (Some(&new_src), Some(&new_dst)) =
                        (new_index.get(dep_name), new_index.get(name))
                    {
                        new_graph.add_edge(new_src, new_dst, ());
                    }
                }
            }
        }
        BeamGraph { graph: new_graph, index: new_index }
    }
}
```

**Step 3: Lancer les tests**

```bash
cargo test -p aurora-core --test dag_test 2>&1
```
Attendu : `5 passed`

**Step 4: Commit**

```bash
git add crates/aurora-core/src/dag.rs crates/aurora-core/tests/dag_test.rs
git commit -m "✨ feat(core): implement DAG engine with topological sort and cycle detection"
```

---

## Task 5: Executor API (aurora-executor-api)

**Files:**
- Modify: `crates/aurora-executor-api/src/lib.rs`
- Create: `crates/aurora-executor-api/tests/executor_api_test.rs`

**Step 1: Tests**

```rust
// crates/aurora-executor-api/tests/executor_api_test.rs
use aurora_executor_api::{ExecutionInput, ExecutionOutput};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_execution_input_builder() {
    let input = ExecutionInput {
        commands: vec!["echo hello".to_string()],
        env: HashMap::from([("KEY".to_string(), "val".to_string())]),
        working_dir: PathBuf::from("/tmp"),
        config: serde_json::json!({}),
    };
    assert_eq!(input.commands.len(), 1);
    assert_eq!(input.env.get("KEY").unwrap(), "val");
}

#[test]
fn test_execution_output_success() {
    let output = ExecutionOutput {
        exit_code: 0,
        stdout: b"hello\n".to_vec(),
        stderr: vec![],
    };
    assert!(output.success());
}

#[test]
fn test_execution_output_failure() {
    let output = ExecutionOutput { exit_code: 1, stdout: vec![], stderr: b"error".to_vec() };
    assert!(!output.success());
}
```

**Step 2: Implémenter**

```rust
// crates/aurora-executor-api/src/lib.rs
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Result;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionInput {
    pub commands: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl ExecutionOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Trait implementé par chaque executor (local, docker, plugins WASM)
#[async_trait::async_trait]
pub trait Executor: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput>;
}
```

Ajouter `async-trait` dans `aurora-executor-api/Cargo.toml` :
```toml
async-trait = "0.1"
```

**Step 3: Tests**

```bash
cargo test -p aurora-executor-api 2>&1
```
Attendu : `3 passed`

**Step 4: Commit**

```bash
git add crates/aurora-executor-api/
git commit -m "✨ feat(executor-api): define Executor trait and ExecutionInput/Output types"
```

---

## Task 6: Executor local (aurora-executor-local)

**Files:**
- Modify: `crates/aurora-executor-local/src/lib.rs`
- Create: `crates/aurora-executor-local/tests/local_executor_test.rs`

**Step 1: Tests**

```rust
// crates/aurora-executor-local/tests/local_executor_test.rs
use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;

#[tokio::test]
async fn test_execute_echo() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo hello".to_string()],
        env: HashMap::new(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "hello");
}

#[tokio::test]
async fn test_execute_multi_commands() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec![
            "echo line1".to_string(),
            "echo line2".to_string(),
        ],
        env: HashMap::new(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("line1"));
    assert!(stdout.contains("line2"));
}

#[tokio::test]
async fn test_execute_failing_command() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["false".to_string()], // toujours exit 1
        env: HashMap::new(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
    };
    let output = executor.execute(input).await.unwrap();
    assert_ne!(output.exit_code, 0);
}

#[tokio::test]
async fn test_env_vars_passed() {
    let executor = LocalExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo $MY_VAR".to_string()],
        env: HashMap::from([("MY_VAR".to_string(), "aurora_test".to_string())]),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({}),
    };
    let output = executor.execute(input).await.unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("aurora_test"));
}
```

**Step 2: Implémenter LocalExecutor**

```rust
// crates/aurora-executor-local/src/lib.rs
use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use tokio::process::Command;

pub struct LocalExecutor;

impl LocalExecutor {
    pub fn new() -> Self { LocalExecutor }
}

impl Default for LocalExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Executor for LocalExecutor {
    fn name(&self) -> &str { "local" }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        // Joindre les commandes avec set -e pour stopper au premier échec
        let script = format!("set -e\n{}", input.commands.join("\n"));

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .current_dir(&input.working_dir)
            .envs(&input.env)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let output = child.wait_with_output().await?;

        Ok(ExecutionOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
```

Ajouter `async-trait` dans `aurora-executor-local/Cargo.toml`.

**Step 3: Tests**

```bash
cargo test -p aurora-executor-local 2>&1
```
Attendu : `4 passed`

**Step 4: Commit**

```bash
git add crates/aurora-executor-local/
git commit -m "✨ feat(executor-local): implement local shell executor with tokio"
```

---

## Task 7: Caching (aurora-core)

**Files:**
- Modify: `crates/aurora-core/src/cache.rs`
- Create: `crates/aurora-core/tests/cache_test.rs`

**Step 1: Tests**

```rust
// crates/aurora-core/tests/cache_test.rs
use aurora_core::cache::BeamCache;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_cache_miss_on_first_run() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    // Aucun cache → miss
    assert!(!cache.is_valid("phpstan", "abc123", &[]));
}

#[test]
fn test_cache_hit_after_save() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("phpstan", "abc123").unwrap();
    assert!(cache.is_valid("phpstan", "abc123", &[]));
}

#[test]
fn test_cache_miss_on_hash_change() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("phpstan", "abc123").unwrap();
    assert!(!cache.is_valid("phpstan", "def456", &[]));
}

#[test]
fn test_cache_miss_if_output_missing() {
    let tmp = tempdir().unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("composer", "abc123").unwrap();
    // vendor/ n'existe pas
    let vendor = tmp.path().join("vendor").to_string_lossy().to_string();
    assert!(!cache.is_valid("composer", "abc123", &[vendor]));
}

#[test]
fn test_cache_hit_if_output_present() {
    let tmp = tempdir().unwrap();
    let vendor = tmp.path().join("vendor");
    fs::create_dir_all(&vendor).unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    cache.save("composer", "abc123").unwrap();
    assert!(cache.is_valid("composer", "abc123", &[vendor.to_string_lossy().to_string()]));
}

#[test]
fn test_hash_files() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("file.txt"), b"content").unwrap();
    let cache = BeamCache::new(tmp.path().to_path_buf());
    let hash = cache.hash_inputs_at(tmp.path(), &["file.txt".to_string()]).unwrap();
    assert!(!hash.is_empty());
    // Même contenu → même hash
    let hash2 = cache.hash_inputs_at(tmp.path(), &["file.txt".to_string()]).unwrap();
    assert_eq!(hash, hash2);
}
```

**Step 2: Implémenter BeamCache**

```rust
// crates/aurora-core/src/cache.rs
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    inputs_hash: String,
}

pub struct BeamCache {
    cache_dir: PathBuf,
}

impl BeamCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        fs::create_dir_all(&cache_dir).ok();
        Self { cache_dir }
    }

    fn entry_path(&self, beam_name: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.json", beam_name))
    }

    pub fn is_valid(&self, beam_name: &str, inputs_hash: &str, outputs: &[String]) -> bool {
        // Vérifier le cache
        let Ok(content) = fs::read_to_string(self.entry_path(beam_name)) else {
            return false;
        };
        let Ok(entry) = serde_json::from_str::<CacheEntry>(&content) else {
            return false;
        };
        if entry.inputs_hash != inputs_hash {
            return false;
        }
        // Vérifier que les outputs existent
        outputs.iter().all(|out| Path::new(out).exists())
    }

    pub fn save(&self, beam_name: &str, inputs_hash: &str) -> Result<()> {
        let entry = CacheEntry { inputs_hash: inputs_hash.to_string() };
        let content = serde_json::to_string_pretty(&entry)?;
        fs::write(self.entry_path(beam_name), content)?;
        Ok(())
    }

    pub fn invalidate(&self, beam_name: &str) -> Result<()> {
        let path = self.entry_path(beam_name);
        if path.exists() { fs::remove_file(path)?; }
        Ok(())
    }

    /// Hash SHA-256 de tous les fichiers correspondant aux patterns glob,
    /// relatifs à `base_dir`.
    pub fn hash_inputs_at(&self, base_dir: &Path, patterns: &[String]) -> Result<String> {
        let mut hasher = Sha256::new();
        let mut files: Vec<PathBuf> = vec![];

        for pattern in patterns {
            let full_pattern = base_dir.join(pattern).to_string_lossy().to_string();
            for entry in glob::glob(&full_pattern)? {
                let path = entry?;
                if path.is_file() { files.push(path); }
            }
        }

        files.sort();
        for file in files {
            let content = fs::read(&file)?;
            hasher.update(file.to_string_lossy().as_bytes());
            hasher.update(b"\0");
            hasher.update(&content);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}
```

**Step 3: Tests**

```bash
cargo test -p aurora-core --test cache_test 2>&1
```
Attendu : `6 passed`

**Step 4: Commit**

```bash
git add crates/aurora-core/src/cache.rs crates/aurora-core/tests/cache_test.rs
git commit -m "✨ feat(core): implement SHA-256 file-based caching for beam inputs/outputs"
```

---

## Task 8: Scheduler (aurora-core)

**Files:**
- Modify: `crates/aurora-core/src/scheduler.rs`
- Create: `crates/aurora-core/tests/scheduler_test.rs`

Le scheduler orchestre l'exécution parallèle via tokio. Il émet des `SchedulerEvent` via un channel mpsc que la TUI consomme.

**Step 1: Tests**

```rust
// crates/aurora-core/tests/scheduler_test.rs
use aurora_core::scheduler::{Scheduler, SchedulerEvent, BeamStatus};
use aurora_core::ast::{Beam, BeamFile, Run};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::sync::Arc;
use tokio::sync::mpsc;

fn make_beam(name: &str, deps: Vec<&str>, commands: Vec<&str>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: if commands.is_empty() { None } else {
            Some(Run {
                commands: commands.iter().map(|s| s.to_string()).collect(),
                executor: None,
            })
        },
    }
}

#[tokio::test]
async fn test_scheduler_simple() {
    let beams = vec![
        make_beam("a", vec![], vec!["echo a"]),
        make_beam("b", vec!["a"], vec!["echo b"]),
    ];
    let executor: Arc<dyn Executor> = Arc::new(LocalExecutor::new());
    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(beams, executor, tx, None);
    scheduler.run("b").await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    // a doit être success avant b
    let success_beams: Vec<_> = events.iter()
        .filter_map(|e| if let SchedulerEvent::BeamCompleted { name, status: BeamStatus::Success { .. } } = e { Some(name.as_str()) } else { None })
        .collect();
    assert!(success_beams.contains(&"a"));
    assert!(success_beams.contains(&"b"));
    let a_pos = success_beams.iter().position(|&n| n == "a").unwrap();
    let b_pos = success_beams.iter().position(|&n| n == "b").unwrap();
    assert!(a_pos < b_pos);
}

#[tokio::test]
async fn test_scheduler_failed_cancels_dependents() {
    let beams = vec![
        make_beam("a", vec![], vec!["false"]), // échoue
        make_beam("b", vec!["a"], vec!["echo b"]),
    ];
    let executor: Arc<dyn Executor> = Arc::new(LocalExecutor::new());
    let (tx, mut rx) = mpsc::channel(32);
    Scheduler::new(beams, executor, tx, None).run("b").await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    let failed = events.iter().any(|e| matches!(e, SchedulerEvent::BeamCompleted { name, status: BeamStatus::Failed { .. } } if name == "a"));
    let cancelled = events.iter().any(|e| matches!(e, SchedulerEvent::BeamCompleted { name, status: BeamStatus::Cancelled } if name == "b"));
    assert!(failed, "a should have failed");
    assert!(cancelled, "b should be cancelled");
}
```

**Step 2: Implémenter le Scheduler**

```rust
// crates/aurora-core/src/scheduler.rs
use crate::ast::{Beam, BeamFile, EnvValue};
use crate::dag::BeamGraph;
use aurora_executor_api::{Executor, ExecutionInput};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub enum BeamStatus {
    Pending,
    Running,
    Success { duration: Duration, cached: bool },
    Skipped { reason: SkipReason },
    Failed { exit_code: i32, duration: Duration },
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    Cached,
    ConditionFalse,
}

#[derive(Debug)]
pub enum SchedulerEvent {
    BeamStarted { name: String },
    BeamCompleted { name: String, status: BeamStatus },
    BeamOutput { name: String, line: String, is_stderr: bool },
    AllDone { success: bool },
}

pub struct Scheduler {
    beams: HashMap<String, Beam>,
    executor: Arc<dyn Executor>,
    tx: mpsc::Sender<SchedulerEvent>,
    max_parallelism: Option<usize>,
}

impl Scheduler {
    pub fn new(
        beams: Vec<Beam>,
        executor: Arc<dyn Executor>,
        tx: mpsc::Sender<SchedulerEvent>,
        max_parallelism: Option<usize>,
    ) -> Self {
        Self {
            beams: beams.into_iter().map(|b| (b.name.clone(), b)).collect(),
            executor,
            tx,
            max_parallelism,
        }
    }

    pub async fn run(self, root: &str) -> Result<bool> {
        let deps: Vec<(String, Vec<String>)> = self.beams.values()
            .map(|b| (b.name.clone(), b.depends_on.clone()))
            .collect();
        let graph = BeamGraph::from_deps(deps)?;
        let levels = graph.execution_levels(root)?;

        let semaphore = self.max_parallelism.map(|n| Arc::new(Semaphore::new(n)));
        let mut overall_success = true;
        let mut cancelled: Vec<String> = vec![];

        for level in &levels {
            let mut set = JoinSet::new();
            for beam_name in level {
                if cancelled.contains(beam_name) {
                    let _ = self.tx.send(SchedulerEvent::BeamCompleted {
                        name: beam_name.clone(),
                        status: BeamStatus::Cancelled,
                    }).await;
                    continue;
                }
                let beam = self.beams[beam_name].clone();
                let executor = self.executor.clone();
                let tx = self.tx.clone();
                let sem = semaphore.clone();

                set.spawn(async move {
                    let _permit = if let Some(s) = sem {
                        Some(s.acquire_owned().await.unwrap())
                    } else { None };

                    let _ = tx.send(SchedulerEvent::BeamStarted { name: beam.name.clone() }).await;

                    // Beam agrégat (pas de run)
                    if beam.run.is_none() {
                        let _ = tx.send(SchedulerEvent::BeamCompleted {
                            name: beam.name.clone(),
                            status: BeamStatus::Success { duration: Duration::ZERO, cached: false },
                        }).await;
                        return (beam.name, true);
                    }

                    // Évaluer skip_if
                    if let Some(cond) = &beam.skip_if {
                        let skip = tokio::process::Command::new("sh")
                            .arg("-c").arg(cond)
                            .status().await
                            .map(|s| s.success()).unwrap_or(false);
                        if skip {
                            let _ = tx.send(SchedulerEvent::BeamCompleted {
                                name: beam.name.clone(),
                                status: BeamStatus::Skipped { reason: SkipReason::ConditionFalse },
                            }).await;
                            return (beam.name, true);
                        }
                    }

                    let run = beam.run.as_ref().unwrap();
                    let input = ExecutionInput {
                        commands: run.commands.clone(),
                        env: std::env::vars().collect(),
                        working_dir: PathBuf::from("."),
                        config: serde_json::json!({}),
                    };

                    let start = Instant::now();
                    let result = executor.execute(input).await;

                    match result {
                        Ok(output) => {
                            let duration = start.elapsed();
                            let success = output.success();
                            let status = if success {
                                BeamStatus::Success { duration, cached: false }
                            } else {
                                BeamStatus::Failed { exit_code: output.exit_code, duration }
                            };
                            let _ = tx.send(SchedulerEvent::BeamCompleted {
                                name: beam.name.clone(),
                                status,
                            }).await;
                            (beam.name, success)
                        }
                        Err(e) => {
                            let _ = tx.send(SchedulerEvent::BeamCompleted {
                                name: beam.name.clone(),
                                status: BeamStatus::Failed { exit_code: -1, duration: start.elapsed() },
                            }).await;
                            (beam.name, false)
                        }
                    }
                });
            }

            while let Some(result) = set.join_next().await {
                if let Ok((name, success)) = result {
                    if !success {
                        overall_success = false;
                        // Annuler les dépendants directs
                        let dependents = graph.direct_dependents(&name);
                        cancelled.extend(dependents);
                    }
                }
            }
        }

        let _ = self.tx.send(SchedulerEvent::AllDone { success: overall_success }).await;
        Ok(overall_success)
    }
}
```

**Step 3: Tests**

```bash
cargo test -p aurora-core --test scheduler_test 2>&1
```
Attendu : `2 passed`

**Step 4: Commit**

```bash
git add crates/aurora-core/src/scheduler.rs crates/aurora-core/tests/scheduler_test.rs
git commit -m "✨ feat(core): implement parallel DAG scheduler with tokio and cancel-on-failure"
```

---

## Task 9: Executor Docker (aurora-executor-docker)

**Files:**
- Modify: `crates/aurora-executor-docker/src/lib.rs`
- Create: `crates/aurora-executor-docker/tests/docker_executor_test.rs`

Stratégie v1 : shell out vers `docker run` (plus simple que bollard, portable).

**Step 1: Tests (nécessitent Docker installé)**

```rust
// crates/aurora-executor-docker/tests/docker_executor_test.rs
// Ces tests nécessitent Docker. Marqués #[ignore] par défaut.
use aurora_executor_api::{ExecutionInput, Executor};
use aurora_executor_docker::DockerExecutor;
use std::collections::HashMap;

#[tokio::test]
#[ignore = "requires docker"]
async fn test_docker_echo() {
    let executor = DockerExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo hello_from_docker".to_string()],
        env: HashMap::new(),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({ "image": "alpine:3.19" }),
    };
    let output = executor.execute(input).await.unwrap();
    assert_eq!(output.exit_code, 0);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("hello_from_docker"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn test_docker_env_vars() {
    let executor = DockerExecutor::new();
    let input = ExecutionInput {
        commands: vec!["echo $MY_VAR".to_string()],
        env: HashMap::from([("MY_VAR".to_string(), "aurora_docker".to_string())]),
        working_dir: std::env::current_dir().unwrap(),
        config: serde_json::json!({ "image": "alpine:3.19" }),
    };
    let output = executor.execute(input).await.unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("aurora_docker"));
}
```

**Step 2: Implémenter DockerExecutor**

```rust
// crates/aurora-executor-docker/src/lib.rs
use anyhow::{bail, Result};
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use tokio::process::Command;

pub struct DockerExecutor;

impl DockerExecutor {
    pub fn new() -> Self { DockerExecutor }
}

impl Default for DockerExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Executor for DockerExecutor {
    fn name(&self) -> &str { "docker" }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let image = input.config["image"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Docker executor requires 'image' config"))?
            .to_string();

        let working_dir_str = input.working_dir.to_string_lossy().to_string();
        let volumes = input.config["volumes"]
            .as_array()
            .map(|v| v.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![format!("{}:/app:rw", working_dir_str)]);

        let script = format!("set -e\n{}", input.commands.join("\n"));

        let mut cmd = Command::new("docker");
        cmd.arg("run").arg("--rm");
        cmd.arg("-w").arg("/app");

        for vol in &volumes {
            cmd.arg("-v").arg(vol);
        }

        for (k, v) in &input.env {
            cmd.arg("-e").arg(format!("{}={}", k, v));
        }

        cmd.arg(&image)
           .arg("sh")
           .arg("-c")
           .arg(&script)
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());

        let output = cmd.output().await?;

        Ok(ExecutionOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
```

**Step 3: Tests unitaires (sans Docker)**

```bash
cargo test -p aurora-executor-docker 2>&1
```
Attendu : `0 passed` (les tests sont `#[ignore]`), pas d'erreur de compilation.

**Step 3b: Tests avec Docker (optionnel)**

```bash
cargo test -p aurora-executor-docker -- --ignored 2>&1
```

**Step 4: Commit**

```bash
git add crates/aurora-executor-docker/
git commit -m "✨ feat(executor-docker): implement Docker executor via docker CLI"
```

---

## Task 10: TUI — vue exécution (aurora-tui)

**Files:**
- Modify: `crates/aurora-tui/src/lib.rs`
- Create: `crates/aurora-tui/src/execution_view.rs`
- Create: `crates/aurora-tui/src/app.rs`

**Step 1: Définir l'App state**

```rust
// crates/aurora-tui/src/app.rs
use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct BeamView {
    pub name: String,
    pub status: BeamStatus,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub started_at: Option<Instant>,
}

impl BeamView {
    pub fn new(name: String) -> Self {
        BeamView {
            name,
            status: BeamStatus::Pending,
            stdout: vec![],
            stderr: vec![],
            started_at: None,
        }
    }

    pub fn status_symbol(&self) -> &str {
        match &self.status {
            BeamStatus::Pending   => "─",
            BeamStatus::Running   => "⣴", // animé dans la TUI
            BeamStatus::Success { cached: true, .. } => "✦",
            BeamStatus::Success { cached: false, .. } => "✔",
            BeamStatus::Skipped { .. }  => "◌",
            BeamStatus::Failed { .. }   => "✕",
            BeamStatus::Cancelled       => "✕",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum AppMode {
    Running,
    LogView,
    Done { success: bool },
}

pub struct App {
    pub beams: Vec<BeamView>,
    pub mode: AppMode,
    pub selected: usize,
    pub log_scroll: u16,
}

impl App {
    pub fn new(beam_names: Vec<String>) -> Self {
        App {
            beams: beam_names.into_iter().map(BeamView::new).collect(),
            mode: AppMode::Running,
            selected: 0,
            log_scroll: 0,
        }
    }

    pub fn apply_event(&mut self, event: SchedulerEvent) {
        match event {
            SchedulerEvent::BeamStarted { name } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = BeamStatus::Running;
                    b.started_at = Some(Instant::now());
                }
            }
            SchedulerEvent::BeamCompleted { name, status } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = status;
                }
            }
            SchedulerEvent::BeamOutput { name, line, is_stderr } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    if is_stderr { b.stderr.push(line); } else { b.stdout.push(line); }
                }
            }
            SchedulerEvent::AllDone { success } => {
                self.mode = AppMode::Done { success };
            }
        }
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.beams.len().saturating_sub(1));
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}
```

**Step 2: Vue exécution ratatui**

```rust
// crates/aurora-tui/src/execution_view.rs
use crate::app::{App, AppMode};
use aurora_core::scheduler::BeamStatus;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

const SPINNER_FRAMES: &[&str] = &["⣇", "⣦", "⣴", "⣸", "⢹", "⠻", "⠟", "⡏"];

pub fn render_execution(f: &mut Frame, app: &App, tick: u64) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    if app.mode == (AppMode::LogView) {
        render_log_view(f, app, chunks[0]);
    } else {
        render_beam_list(f, app, tick, chunks[0]);
    }
    render_status_bar(f, app, chunks[1]);
}

fn render_beam_list(f: &mut Frame, app: &App, tick: u64, area: Rect) {
    let items: Vec<ListItem> = app.beams.iter().enumerate().map(|(i, beam)| {
        let symbol = match &beam.status {
            BeamStatus::Running => SPINNER_FRAMES[(tick / 2 % SPINNER_FRAMES.len() as u64) as usize],
            _ => beam.status_symbol(),
        };
        let color = status_color(&beam.status);
        let duration_str = match &beam.status {
            BeamStatus::Success { duration, .. } => format!(" [{:.2}s]", duration.as_secs_f32()),
            BeamStatus::Failed { duration, .. }  => format!(" [{:.2}s]", duration.as_secs_f32()),
            BeamStatus::Running => {
                if let Some(t) = beam.started_at {
                    format!(" [{:.0}s]", t.elapsed().as_secs_f32())
                } else { String::new() }
            }
            _ => String::new(),
        };

        let line = Line::from(vec![
            Span::styled(format!("  {}  ", symbol), Style::default().fg(color)),
            Span::styled(format!("{:<20}", beam.name), Style::default()
                .fg(if i == app.selected { Color::White } else { Color::Gray })
                .add_modifier(if i == app.selected { Modifier::BOLD } else { Modifier::empty() })),
            Span::styled(duration_str, Style::default().fg(Color::DarkGray)),
        ]);
        ListItem::new(line)
    }).collect();

    let title = match &app.mode {
        AppMode::Done { success: true }  => " Aurora ✔ Done ",
        AppMode::Done { success: false } => " Aurora ✕ Failed ",
        _ => " Aurora  Running... ",
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}

fn render_log_view(f: &mut Frame, app: &App, area: Rect) {
    let beam = &app.beams[app.selected];
    let mut lines: Vec<Line> = beam.stdout.iter()
        .map(|l| Line::from(l.as_str()))
        .collect();
    if !beam.stderr.is_empty() {
        lines.push(Line::from(Span::styled("── stderr ──", Style::default().fg(Color::Red))));
        lines.extend(beam.stderr.iter().map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(Color::Red)))));
    }
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(format!(" {} — Logs ", beam.name)))
        .wrap(Wrap { trim: false })
        .scroll((app.log_scroll, 0));
    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        AppMode::LogView => " [Esc] retour  [↑↓] scroll  [q] quitter ",
        AppMode::Done { .. } => " [↑↓] naviguer  [Enter] logs  [r] retry  [q] quitter ",
        AppMode::Running => " [↑↓] naviguer  [Enter] logs  [q] annuler ",
    };
    let bar = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    f.render_widget(bar, area);
}

fn status_color(status: &BeamStatus) -> Color {
    match status {
        BeamStatus::Success { .. } => Color::Green,
        BeamStatus::Skipped { .. } => Color::Cyan,
        BeamStatus::Failed { .. }  => Color::Red,
        BeamStatus::Cancelled      => Color::Magenta,
        BeamStatus::Running        => Color::Yellow,
        BeamStatus::Pending        => Color::DarkGray,
    }
}
```

**Step 3: lib.rs principal (event loop)**

```rust
// crates/aurora-tui/src/lib.rs
pub mod app;
pub mod execution_view;
pub mod picker_view;

use anyhow::Result;
use app::{App, AppMode};
use aurora_core::scheduler::SchedulerEvent;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

pub async fn run_execution_tui(
    beam_names: Vec<String>,
    mut rx: mpsc::Receiver<SchedulerEvent>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(beam_names);
    let mut tick: u64 = 0;

    loop {
        // Drainer les événements du scheduler
        while let Ok(evt) = rx.try_recv() {
            let is_done = matches!(evt, SchedulerEvent::AllDone { .. });
            app.apply_event(evt);
            if is_done { break; }
        }

        terminal.draw(|f| execution_view::render_execution(f, &app, tick))?;
        tick += 1;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down  | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up    | KeyCode::Char('k') => app.select_prev(),
                    KeyCode::Enter => {
                        app.mode = if app.mode == AppMode::LogView {
                            AppMode::Done { success: true } // TODO : restore correct state
                        } else {
                            AppMode::LogView
                        };
                    }
                    KeyCode::Esc => {
                        if app.mode == AppMode::LogView {
                            app.mode = AppMode::Running;
                        }
                    }
                    _ => {}
                }
            }
        }

        if matches!(app.mode, AppMode::Done { .. }) && !event::poll(Duration::ZERO)? {
            // Laisser la TUI affichée jusqu'à une touche
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
```

Créer un stub `picker_view.rs` :
```bash
touch crates/aurora-tui/src/picker_view.rs
```

**Step 4: Vérifier que ça compile**

```bash
cargo build -p aurora-tui 2>&1
```

**Step 5: Commit**

```bash
git add crates/aurora-tui/
git commit -m "✨ feat(tui): implement execution view with ratatui (beam list, log view, status bar)"
```

---

## Task 11: TUI — Picker (aurora-tui)

**Files:**
- Modify: `crates/aurora-tui/src/picker_view.rs`
- Modify: `crates/aurora-tui/src/lib.rs`

**Step 1: Picker view**

```rust
// crates/aurora-tui/src/picker_view.rs
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub struct PickerState {
    pub beams: Vec<PickerBeam>,
    pub selected: usize,
    pub search: String,
    pub show_deps: bool,
}

pub struct PickerBeam {
    pub name: String,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
}

impl PickerState {
    pub fn filtered(&self) -> Vec<&PickerBeam> {
        if self.search.is_empty() {
            self.beams.iter().collect()
        } else {
            self.beams.iter()
                .filter(|b| b.name.contains(&self.search) ||
                    b.description.as_deref().map(|d| d.contains(&self.search)).unwrap_or(false))
                .collect()
        }
    }

    pub fn selected_beam(&self) -> Option<&PickerBeam> {
        self.filtered().into_iter().nth(self.selected)
    }
}

pub fn render_picker(f: &mut Frame, state: &PickerState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search bar
            Constraint::Min(0),    // beam list
            Constraint::Length(1), // help
        ])
        .split(area);

    // Search bar
    let search = Paragraph::new(format!(" {} ", state.search))
        .block(Block::default().borders(Borders::ALL).title(" Aurora — Choisir un beam "));
    f.render_widget(search, chunks[0]);

    // Beam list
    let filtered = state.filtered();
    let items: Vec<ListItem> = filtered.iter().enumerate().map(|(i, beam)| {
        let selected = i == state.selected;
        let prefix = if selected { "▶ " } else { "  " };
        let desc = beam.description.as_deref().unwrap_or("");
        let line = Line::from(vec![
            Span::styled(
                format!("{}{:<20}  {}", prefix, beam.name, desc),
                if selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                },
            ),
        ]);
        ListItem::new(line)
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(list, chunks[1]);

    // Help
    let help = Paragraph::new(" [↑↓] naviguer  [/] rechercher  [Tab] dépendances  [Enter] lancer  [q] quitter ")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, chunks[2]);
}
```

**Step 2: Fonction picker dans lib.rs**

```rust
// Ajouter à crates/aurora-tui/src/lib.rs

use picker_view::{PickerState, PickerBeam};

/// Lance le picker interactif. Retourne le nom du beam sélectionné.
pub fn run_picker(beam_info: Vec<(String, Option<String>, Vec<String>)>) -> Result<Option<String>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = PickerState {
        beams: beam_info.into_iter().map(|(name, desc, deps)| PickerBeam {
            name, description: desc, depends_on: deps,
        }).collect(),
        selected: 0,
        search: String::new(),
        show_deps: false,
    };

    let mut result = None;

    loop {
        terminal.draw(|f| picker_view::render_picker(f, &state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let filtered_count = state.filtered().len();
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Enter => {
                        result = state.selected_beam().map(|b| b.name.clone());
                        break;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.selected = (state.selected + 1).min(filtered_count.saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.selected = state.selected.saturating_sub(1);
                    }
                    KeyCode::Char('/') => { state.search.clear(); }
                    KeyCode::Backspace => { state.search.pop(); }
                    KeyCode::Char(c) => {
                        state.search.push(c);
                        state.selected = 0;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(result)
}
```

**Step 3: Compiler**

```bash
cargo build -p aurora-tui 2>&1
```

**Step 4: Commit**

```bash
git add crates/aurora-tui/src/picker_view.rs crates/aurora-tui/src/lib.rs
git commit -m "✨ feat(tui): add interactive beam picker with search"
```

---

## Task 12: CLI entrypoint (aurora)

**Files:**
- Modify: `crates/aurora/src/main.rs`

**Step 1: Implémenter le CLI**

```rust
// crates/aurora/src/main.rs
use anyhow::{bail, Result};
use aurora_core::{parser::parse, scheduler::Scheduler};
use aurora_executor_api::Executor;
use aurora_executor_docker::DockerExecutor;
use aurora_executor_local::LocalExecutor;
use clap::{Arg, Command};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Command::new("aurora")
        .version("0.1.0")
        .about("Aurora — task runner with HCL-inspired Beamfile DSL")
        .arg(Arg::new("beam").help("Beam to run").index(1))
        .arg(Arg::new("no-cache").long("no-cache").action(clap::ArgAction::SetTrue))
        .arg(Arg::new("dry-run").long("dry-run").action(clap::ArgAction::SetTrue))
        .arg(Arg::new("list").long("list").short('l').action(clap::ArgAction::SetTrue))
        .arg(Arg::new("var").long("var").action(clap::ArgAction::Append)
             .help("Override variable: --var key=value"));

    let matches = cli.get_matches();

    // Trouver et lire le Beamfile
    let beamfile_path = find_beamfile()?;
    let content = fs::read_to_string(&beamfile_path)?;
    let mut beam_file = parse(&content)?;

    // Appliquer les overrides de variables
    if let Some(vars) = matches.get_many::<String>("var") {
        for var_str in vars {
            let (key, val) = var_str.split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid --var format, expected key=value"))?;
            if let Some(v) = beam_file.variables.iter_mut().find(|v| v.name == key) {
                v.default = val.to_string();
            }
        }
    }

    // --list
    if matches.get_flag("list") {
        println!("Available beams:");
        for beam in &beam_file.beams {
            let desc = beam.description.as_deref().unwrap_or("");
            println!("  {:<20}  {}", beam.name, desc);
        }
        return Ok(());
    }

    // --dry-run
    if matches.get_flag("dry-run") {
        let target = resolve_target(&beam_file, matches.get_one::<String>("beam").map(|s| s.as_str()))?;
        println!("Would execute beam: {}", target);
        return Ok(());
    }

    // Résoudre le beam cible
    let target = if let Some(beam_name) = matches.get_one::<String>("beam") {
        beam_name.clone()
    } else if let Some(picker_result) = aurora_tui::run_picker(
        beam_file.beams.iter().map(|b| (b.name.clone(), b.description.clone(), b.depends_on.clone())).collect()
    )? {
        picker_result
    } else {
        return Ok(());
    };

    // Choisir l'executor (en v1 : local par défaut, docker si beam a executor docker)
    // Le scheduler détermine l'executor par beam — pour simplifier v1, on passe local
    let executor: Arc<dyn Executor> = Arc::new(LocalExecutor::new());

    let (tx, rx) = mpsc::channel(128);
    let beam_names: Vec<String> = beam_file.beams.iter().map(|b| b.name.clone()).collect();

    // Lancer le scheduler en background
    let beams = beam_file.beams.clone();
    let scheduler = Scheduler::new(beams, executor, tx, beam_file.config.as_ref().and_then(|c| c.max_parallelism));

    let target_clone = target.clone();
    tokio::spawn(async move {
        if let Err(e) = scheduler.run(&target_clone).await {
            eprintln!("Scheduler error: {}", e);
        }
    });

    // Lancer la TUI
    aurora_tui::run_execution_tui(beam_names, rx).await?;

    Ok(())
}

fn find_beamfile() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join("Beamfile");
        if candidate.exists() { return Ok(candidate); }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => bail!("No Beamfile found in current directory or any parent"),
        }
    }
}

fn resolve_target<'a>(
    beam_file: &'a aurora_core::ast::BeamFile,
    explicit: Option<&str>,
) -> Result<String> {
    if let Some(name) = explicit {
        Ok(name.to_string())
    } else if let Some(cfg) = &beam_file.config {
        if let Some(default) = &cfg.default {
            return Ok(default.clone());
        }
    }
    bail!("No beam specified and no default configured in aurora {{ }}")
}
```

**Step 2: Build**

```bash
cargo build -p aurora 2>&1
```

**Step 3: Smoke test**

```bash
# Dans un répertoire avec un Beamfile simple
cat > /tmp/test-Beamfile << 'EOF'
aurora {
  version = "1"
  default = "hello"
}

beam "hello" {
  description = "Say hello"
  run {
    commands = ["echo Hello from Aurora!"]
  }
}
EOF

cd /tmp && aurora --list
# Attendu : affiche hello

aurora hello
# Attendu : TUI avec beam "hello" success
```

**Step 4: Commit**

```bash
git add crates/aurora/src/main.rs
git commit -m "✨ feat(cli): implement aurora CLI with clap, picker, and execution dispatch"
```

---

## Task 13: Plugin system WASM (extism)

**Step 1: Ajouter extism à aurora/Cargo.toml**

```toml
extism = "1"
```

**Step 2: Créer le plugin loader**

```rust
// crates/aurora/src/plugins.rs
use anyhow::Result;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use extism::{Manifest, Plugin, Wasm};
use std::path::PathBuf;
use async_trait::async_trait;

pub struct WasmExecutor {
    name: String,
    plugin_path: PathBuf,
}

impl WasmExecutor {
    pub fn load(name: String, path: PathBuf) -> Result<Self> {
        // Valider que le fichier existe et est un WASM valide
        if !path.exists() { anyhow::bail!("Plugin not found: {:?}", path); }
        Ok(WasmExecutor { name, plugin_path: path })
    }
}

#[async_trait]
impl Executor for WasmExecutor {
    fn name(&self) -> &str { &self.name }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let plugin_path = self.plugin_path.clone();
        let input_json = serde_json::to_vec(&input)?;

        // Exécuter le plugin WASM en thread bloquant (extism n'est pas async)
        let result = tokio::task::spawn_blocking(move || -> Result<ExecutionOutput> {
            let wasm = Wasm::file(&plugin_path);
            let manifest = Manifest::new([wasm]);
            let mut plugin = Plugin::new(&manifest, [], false)?;
            let output_bytes = plugin.call::<&[u8], &[u8]>("execute", &input_json)?;
            Ok(serde_json::from_slice(output_bytes)?)
        }).await??;

        Ok(result)
    }
}

/// Découverte des plugins WASM dans ~/.aurora/plugins/
pub fn discover_plugins() -> Vec<(String, PathBuf)> {
    let plugins_dir = dirs::home_dir()
        .map(|h| h.join(".aurora/plugins"))
        .unwrap_or_default();

    if !plugins_dir.exists() { return vec![]; }

    std::fs::read_dir(&plugins_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "wasm" {
                let name = path.file_stem()?.to_string_lossy().to_string();
                Some((name, path))
            } else { None }
        })
        .collect()
}
```

**Step 3: Build**

```bash
cargo build -p aurora 2>&1
```

**Step 4: Commit**

```bash
git add crates/aurora/src/plugins.rs crates/aurora/src/main.rs
git commit -m "✨ feat(plugins): add WASM executor plugin system via extism"
```

---

## Task 14: Dogfooding — Beamfile Aurora + migration omega

**Step 1: Écrire le Beamfile d'Aurora**

```hcl
# Beamfile (à la racine d'Aurora)

aurora {
  version = "1"
  default = "check"
}

beam "fmt" {
  description = "Format Rust code"
  run { commands = ["cargo fmt --all"] }
}

beam "clippy" {
  description = "Lint with clippy"
  run { commands = ["cargo clippy --workspace -- -D warnings"] }
}

beam "test" {
  description = "Run all tests"
  run { commands = ["cargo test --workspace"] }
}

beam "build" {
  description = "Build release binary"
  run { commands = ["cargo build --release"] }
}

beam "check" {
  description = "Format + lint + test"
  depends_on = ["fmt", "clippy", "test"]
}
```

**Step 2: Tester aurora sur lui-même**

```bash
cd /path/to/aurora
aurora check
```
Attendu : TUI avec les 3 beams exécutés en parallèle (clippy et test dépendent tous les deux de fmt → fmt d'abord, puis clippy+test en parallèle)

**Step 3: Migrer omega — valider le Beamfile existant**

```bash
cp /home/jeandenis.vidot@SAFTI.local/Gitlab/Safti/omega/Beamfile /tmp/omega-Beamfile-test

# Vérifier que aurora parse le fichier sans erreur
aurora --list --beamfile /tmp/omega-Beamfile-test
```
(Ajouter `--beamfile` flag si nécessaire, ou copier dans un dossier test)

**Step 4: Ajustements finaux Beamfile omega**

Vérifier et ajuster `var.docker_image` → doit être résolu avant d'appeler l'executor docker.
Ajouter résolution des variables dans le scheduler (remplacer `var.docker_image` dans les configs executor).

**Step 5: Commit final**

```bash
git add Beamfile
git commit -m "✨ feat: add Aurora Beamfile for dogfooding (check = fmt + clippy + test)"
```

---

## Rappel des commandes de test

```bash
# Tous les tests
cargo test --workspace

# Tests par crate
cargo test -p aurora-core
cargo test -p aurora-executor-local
cargo test -p aurora-executor-api

# Tests Docker (nécessitent docker)
cargo test -p aurora-executor-docker -- --ignored

# Build release
cargo build --release

# Smoke test minimal
echo 'aurora { version="1" default="hi" } beam "hi" { run { commands = ["echo hi"] } }' > /tmp/Beamfile
cd /tmp && aurora
```
