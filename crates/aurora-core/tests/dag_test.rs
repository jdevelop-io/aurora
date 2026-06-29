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

    // Level 0: composer (no deps)
    // Level 1: lint and test (parallel, both depend on composer)
    // Level 2: qa (depends on lint and test)
    assert_eq!(levels.len(), 3);
    assert_eq!(levels[0], vec!["composer"]);
    let mut l1 = levels[1].clone();
    l1.sort();
    assert_eq!(l1, vec!["lint", "test"]);
    assert_eq!(levels[2], vec!["qa"]);
}

#[test]
fn test_cycle_detection() {
    let deps = vec![
        ("a", vec!["b"]),
        ("b", vec!["c"]),
        ("c", vec!["a"]), // cycle!
    ];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let result = graph.execution_levels("a");
    assert!(matches!(result, Err(DagError::Cycle(_))));
}

#[test]
fn test_unknown_dependency_error() {
    let deps = vec![
        ("qa", vec!["nonexistent"]),
    ];
    let result = BeamGraph::from_deps(deps);
    assert!(matches!(result, Err(DagError::UnknownBeam(_))));
}

#[test]
fn test_transitive_deps_includes_all() {
    let deps = vec![
        ("qa",        vec!["lint"]),
        ("lint",      vec!["composer"]),
        ("composer",  vec![]),
        ("unrelated", vec![]),
    ];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let mut transitive = graph.transitive_deps("qa");
    transitive.sort();
    // qa -> lint -> composer; "unrelated" should NOT be included
    assert_eq!(transitive, vec!["composer", "lint", "qa"]);
}

#[test]
fn test_direct_dependents() {
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

#[test]
fn test_single_beam_no_deps() {
    let deps = vec![("hello", vec![])];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let levels = graph.execution_levels("hello").unwrap();
    assert_eq!(levels.len(), 1);
    assert_eq!(levels[0], vec!["hello"]);
}

#[test]
fn test_deep_dependency_chain_no_stack_overflow() {
    // Chaîne linéaire profonde b0 -> b1 -> ... -> b(N-1). Une traversée
    // récursive déborderait la pile ; la version itérative doit terminer.
    let n = 100_000usize;
    let mut deps: Vec<(String, Vec<String>)> = Vec::with_capacity(n);
    for i in 0..n {
        let d = if i + 1 < n { vec![format!("b{}", i + 1)] } else { vec![] };
        deps.push((format!("b{i}"), d));
    }
    let graph = BeamGraph::from_deps(deps).unwrap();
    let transitive = graph.transitive_deps("b0");
    assert_eq!(transitive.len(), n);
}

#[test]
fn test_unknown_root_beam() {
    let deps = vec![("a", vec![])];
    let graph = BeamGraph::from_deps(deps).unwrap();
    // Requesting levels for a beam not in the graph
    let levels = graph.execution_levels("nonexistent").unwrap();
    assert!(levels.is_empty());
}

#[test]
fn test_direct_dependencies() {
    let deps = vec![
        ("qa",       vec!["lint", "test"]),
        ("lint",     vec!["composer"]),
        ("test",     vec!["composer"]),
        ("composer", vec![]),
    ];
    let graph = BeamGraph::from_deps(deps).unwrap();
    let mut d = graph.direct_dependencies("qa");
    d.sort();
    assert_eq!(d, vec!["lint", "test"]);
    assert!(graph.direct_dependencies("composer").is_empty());
    assert!(graph.direct_dependencies("inconnu").is_empty());
}
