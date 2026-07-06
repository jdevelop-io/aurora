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

#[derive(Debug, Clone)]
pub struct Beam {
    pub name: String,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Beam-local variables. Same `variable {}` syntax as the top level, but
    /// scoped to this beam: they shadow a global of the same name and are not
    /// reachable by `--var` (which targets globals only).
    pub variables: Vec<Variable>,
    /// Working directory for this beam. When set, the beam's run commands,
    /// inputs/outputs and gates all resolve against this directory. Relative
    /// paths join onto the Beamfile directory; absolute paths replace it.
    pub dir: Option<String>,
    pub skip_if: Option<String>,
    pub condition: Option<Condition>,
    pub run: Option<Run>,
    pub allow_failure: bool,
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
