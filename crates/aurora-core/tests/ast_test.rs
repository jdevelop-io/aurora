use aurora_core::ast::*;

#[test]
fn test_beam_with_all_fields() {
    let beam = Beam {
        name: "phpstan".to_string(),
        description: Some("Static analysis".to_string()),
        depends_on: vec!["composer".to_string()],
        inputs: vec!["src/**/*.php".to_string()],
        outputs: vec![],
        dir: None,
        skip_if: Some("test -z \"$CHANGED\"".to_string()),
        condition: None,
        run: Some(Run {
            commands: vec!["phpstan analyse".to_string()],
            executor: Some(ExecutorConfig {
                name: "docker".to_string(),
                config: [("image".to_string(), "omega-tools:v1".to_string())]
                    .into_iter()
                    .collect(),
            }),
        }),
        allow_failure: false,
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
        dir: None,
        skip_if: None,
        condition: None,
        run: None,
        allow_failure: false,
    };
    assert!(beam.run.is_none());
    assert_eq!(beam.depends_on.len(), 2);
}

#[test]
fn test_condition_any() {
    let cond = Condition {
        op: ConditionOp::Any,
        clauses: vec![
            ConditionClause::Shell("test -n \"$A\"".to_string()),
            ConditionClause::Shell("test -n \"$B\"".to_string()),
        ],
    };
    assert!(matches!(cond.op, ConditionOp::Any));
    assert_eq!(cond.clauses.len(), 2);
}

#[test]
fn test_environment_sequential_vars() {
    let env = Environment {
        vars: vec![
            EnvVar {
                name: "BRANCH".to_string(),
                value: EnvValue::Shell("git branch --show-current".to_string()),
            },
            EnvVar {
                name: "MODE".to_string(),
                value: EnvValue::Literal("production".to_string()),
            },
        ],
    };
    assert_eq!(env.vars.len(), 2);
    assert!(matches!(&env.vars[0].value, EnvValue::Shell(_)));
    assert!(matches!(&env.vars[1].value, EnvValue::Literal(_)));
}

#[test]
fn test_beamfile_full() {
    let bf = BeamFile {
        config: Some(AuroraConfig {
            version: "1".to_string(),
            default: Some("qa".to_string()),
            max_parallelism: Some(4),
        }),
        variables: vec![Variable {
            name: "image".to_string(),
            default: "ubuntu:22.04".to_string(),
            description: Some("Docker image".to_string()),
        }],
        environment: None,
        beams: vec![],
    };
    assert_eq!(bf.config.as_ref().unwrap().default.as_deref(), Some("qa"));
    assert_eq!(bf.variables[0].name, "image");
}
