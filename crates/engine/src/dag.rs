//! Directed Acyclic Graph for dependency resolution.

use std::collections::HashMap;

use aurora_core::{AuroraError, Beamfile, Result};
use petgraph::Direction;
use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};

/// Represents the dependency graph of beams.
#[derive(Debug)]
pub struct DependencyGraph {
    /// Mapping from beam name to node index.
    nodes: HashMap<String, NodeIndex>,

    /// The underlying directed graph.
    graph: DiGraph<String, ()>,
}

impl DependencyGraph {
    /// Creates a new dependency graph from a Beamfile.
    pub fn from_beamfile(beamfile: &Beamfile) -> Result<Self> {
        let mut graph = DiGraph::new();
        let mut nodes = HashMap::new();

        // Add all beams as nodes
        for name in beamfile.beam_names() {
            let idx = graph.add_node(name.to_string());
            nodes.insert(name.to_string(), idx);
        }

        // Add edges for dependencies
        for (name, beam) in &beamfile.beams {
            let from_idx = nodes[name];

            for dep in &beam.depends_on {
                let to_idx = nodes.get(dep).ok_or_else(|| {
                    AuroraError::BeamNotFound(format!(
                        "Beam '{}' depends on '{}' which does not exist",
                        name, dep
                    ))
                })?;

                // Edge goes from dependency to dependent (dep -> name)
                // This means: dep must run before name
                graph.add_edge(*to_idx, from_idx, ());
            }
        }

        let dag = Self { nodes, graph };

        // Check for cycles
        if let Some(cycle) = dag.detect_cycle() {
            return Err(AuroraError::CycleDetected(cycle));
        }

        Ok(dag)
    }

    /// Detects if there's a cycle in the graph and returns a description.
    pub fn detect_cycle(&self) -> Option<String> {
        if is_cyclic_directed(&self.graph) {
            // Find a cycle (simplified - just report that one exists)
            Some("Dependency cycle detected in beam definitions".to_string())
        } else {
            None
        }
    }

    /// Returns the topological order of beams needed to execute a target.
    pub fn topological_order(&self, target: &str) -> Result<Vec<String>> {
        let target_idx = self
            .nodes
            .get(target)
            .ok_or_else(|| AuroraError::BeamNotFound(target.to_string()))?;

        // Get all ancestors of the target (including the target itself)
        let mut required: HashMap<NodeIndex, ()> = HashMap::new();
        self.collect_ancestors(*target_idx, &mut required);

        // Filter the topological sort to only include required nodes
        let sorted = toposort(&self.graph, None).map_err(|_| {
            AuroraError::CycleDetected("Cycle detected during topological sort".to_string())
        })?;

        Ok(sorted
            .into_iter()
            .filter(|idx| required.contains_key(idx))
            .map(|idx| self.graph[idx].clone())
            .collect())
    }

    /// Collects all ancestors of a node (dependencies).
    fn collect_ancestors(&self, node: NodeIndex, visited: &mut HashMap<NodeIndex, ()>) {
        if visited.contains_key(&node) {
            return;
        }

        visited.insert(node, ());

        for neighbor in self.graph.neighbors_directed(node, Direction::Incoming) {
            self.collect_ancestors(neighbor, visited);
        }
    }

    /// Returns beams grouped by execution level (for parallel execution).
    /// Beams in the same level have no dependencies on each other.
    pub fn parallel_levels(&self, target: &str) -> Result<Vec<Vec<String>>> {
        let order = self.topological_order(target)?;

        if order.is_empty() {
            return Ok(Vec::new());
        }

        let mut levels: Vec<Vec<String>> = Vec::new();
        let mut beam_levels: HashMap<String, usize> = HashMap::new();

        for beam_name in order {
            let beam_idx = self.nodes[&beam_name];

            // Find the maximum level of all dependencies
            let max_dep_level = self
                .graph
                .neighbors_directed(beam_idx, Direction::Incoming)
                .filter_map(|dep_idx| {
                    let dep_name = &self.graph[dep_idx];
                    beam_levels.get(dep_name).copied()
                })
                .max()
                .map(|l| l + 1)
                .unwrap_or(0);

            beam_levels.insert(beam_name.clone(), max_dep_level);

            // Ensure we have enough levels
            while levels.len() <= max_dep_level {
                levels.push(Vec::new());
            }

            levels[max_dep_level].push(beam_name);
        }

        Ok(levels)
    }

    /// Returns all beam names in the graph.
    pub fn beam_names(&self) -> Vec<&str> {
        self.nodes.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::Beam;

    fn create_test_beamfile() -> Beamfile {
        let mut bf = Beamfile::new("test");

        // clean (no deps)
        bf.add_beam(Beam::new("clean"));

        // lint (no deps)
        bf.add_beam(Beam::new("lint"));

        // build (depends on clean, lint)
        bf.add_beam(
            Beam::new("build").with_depends_on(vec!["clean".to_string(), "lint".to_string()]),
        );

        // test (depends on build)
        bf.add_beam(Beam::new("test").with_depends_on(vec!["build".to_string()]));

        bf
    }

    #[test]
    fn test_topological_order() {
        let bf = create_test_beamfile();
        let dag = DependencyGraph::from_beamfile(&bf).unwrap();

        let order = dag.topological_order("test").unwrap();

        // test should come last
        assert_eq!(order.last(), Some(&"test".to_string()));

        // build should come before test
        let build_pos = order.iter().position(|x| x == "build").unwrap();
        let test_pos = order.iter().position(|x| x == "test").unwrap();
        assert!(build_pos < test_pos);

        // clean and lint should come before build
        let clean_pos = order.iter().position(|x| x == "clean").unwrap();
        let lint_pos = order.iter().position(|x| x == "lint").unwrap();
        assert!(clean_pos < build_pos);
        assert!(lint_pos < build_pos);
    }

    #[test]
    fn test_parallel_levels() {
        let bf = create_test_beamfile();
        let dag = DependencyGraph::from_beamfile(&bf).unwrap();

        let levels = dag.parallel_levels("test").unwrap();

        // Level 0: clean, lint (can run in parallel)
        assert_eq!(levels[0].len(), 2);
        assert!(levels[0].contains(&"clean".to_string()));
        assert!(levels[0].contains(&"lint".to_string()));

        // Level 1: build
        assert_eq!(levels[1], vec!["build".to_string()]);

        // Level 2: test
        assert_eq!(levels[2], vec!["test".to_string()]);
    }

    #[test]
    fn test_cycle_detection() {
        let mut bf = Beamfile::new("test");

        // Create a cycle: a -> b -> c -> a
        bf.add_beam(Beam::new("a").with_depends_on(vec!["c".to_string()]));
        bf.add_beam(Beam::new("b").with_depends_on(vec!["a".to_string()]));
        bf.add_beam(Beam::new("c").with_depends_on(vec!["b".to_string()]));

        let result = DependencyGraph::from_beamfile(&bf);
        assert!(result.is_err());
    }
}
