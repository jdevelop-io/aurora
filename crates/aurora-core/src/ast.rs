use std::collections::BTreeMap;
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
    /// Evaluated sequentially in declaration order
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

/// One `depends_on` edge. The short string form parses into an entry with an
/// empty `params` map; the bound object form (added with beam params) carries
/// explicit bindings for the dependency's declared params.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub beam: String,
    pub params: BTreeMap<String, String>,
}

impl Dependency {
    pub fn named(beam: impl Into<String>) -> Self {
        Self {
            beam: beam.into(),
            params: BTreeMap::new(),
        }
    }
}

/// A declared beam parameter: `param "name" { default = "..." description = "..." }`.
/// Declaration order is preserved: it doubles as the CLI positional order.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub default: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Beam {
    pub name: String,
    pub description: Option<String>,
    pub depends_on: Vec<Dependency>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Declared named parameters, in declaration order. Replaces beam-local
    /// `variable {}` blocks: a param may be bound by a dependent through the
    /// `depends_on` object form, or supplied positionally when this beam is
    /// the invoked target.
    pub params: Vec<Param>,
    /// Beam-scoped `environment {}` block. Evaluated the same way as the
    /// top-level block, but only visible to this beam's own execution.
    pub environment: Option<Environment>,
    /// Resolved param bindings of this instance. Empty before expansion and for
    /// a beam that declares no params. Sorted, so the instance id and the cache
    /// key derive from it deterministically.
    pub bindings: BTreeMap<String, String>,
    /// Working directory for this beam. When set, the beam's run commands,
    /// inputs/outputs and gates all resolve against this directory. Relative
    /// paths join onto the Beamfile directory; absolute paths replace it.
    pub dir: Option<String>,
    pub skip_if: Option<String>,
    pub condition: Option<Condition>,
    pub run: Option<Run>,
    pub allow_failure: bool,
    /// Evaluated per-instance `environment {}` overlay (filled after
    /// expansion, empty when the beam declares no block). Shadows the global
    /// environment for this instance only, in execution and in the cache key.
    pub env_overlay: BTreeMap<String, String>,
}

impl Beam {
    /// The dependency names, without bindings. Post-expansion these are
    /// instance ids; the DAG, the watch closure and the TUI consume this.
    pub fn dependency_names(&self) -> Vec<String> {
        self.depends_on.iter().map(|d| d.beam.clone()).collect()
    }
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
