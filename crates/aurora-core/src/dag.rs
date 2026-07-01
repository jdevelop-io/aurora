use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DagError {
    #[error("Unknown beam referenced as dependency: '{0}'")]
    UnknownBeam(String),
    #[error("Cycle detected in beam dependency graph involving: '{0}'")]
    Cycle(String),
}

pub struct BeamGraph {
    /// Directed graph: edge A → B means "A must run before B" (A is a dep of B)
    graph: DiGraph<String, ()>,
    index: HashMap<String, NodeIndex>,
}

impl BeamGraph {
    /// Build the graph from a list of (beam_name, [dep_names]).
    /// Returns Err if a referenced dependency doesn't exist as a beam.
    pub fn from_deps<S: AsRef<str>>(deps: Vec<(S, Vec<S>)>) -> Result<Self, DagError> {
        let mut graph: DiGraph<String, ()> = DiGraph::new();
        let mut index = HashMap::new();

        // First pass: add all nodes
        for (name, _) in &deps {
            let idx = graph.add_node(name.as_ref().to_string());
            index.insert(name.as_ref().to_string(), idx);
        }

        // Second pass: add edges dep → beam
        for (name, beam_deps) in &deps {
            let beam_idx = *index.get(name.as_ref()).unwrap();
            for dep in beam_deps {
                let dep_idx = index
                    .get(dep.as_ref())
                    .ok_or_else(|| DagError::UnknownBeam(dep.as_ref().to_string()))?;
                // dep must complete before beam: edge dep_idx → beam_idx
                graph.add_edge(*dep_idx, beam_idx, ());
            }
        }

        // Reject cycles at construction, on the whole graph. This is consistent
        // with the unknown-dependency check above (also global): a cycle in any
        // branch fails the file, not only one reachable from a given target.
        if let Err(cycle) = toposort(&graph, None) {
            return Err(DagError::Cycle(graph[cycle.node_id()].clone()));
        }

        Ok(BeamGraph { graph, index })
    }

    /// Returns the set of beams in the transitive closure of `root`
    /// (i.e., root + all its transitive dependencies), as a Vec<String>.
    pub fn transitive_deps(&self, root: &str) -> Vec<String> {
        let root_idx = match self.index.get(root) {
            Some(&idx) => idx,
            None => return vec![],
        };

        // Iterative DFS (explicit stack): avoids stack overflow on long
        // dependency chains (a Beamfile could declare tens of thousands of
        // them). The HashSet gives O(1) membership, instead of the O(n) of a
        // Vec::contains that made the traversal quadratic.
        let mut seen: HashSet<NodeIndex> = HashSet::new();
        let mut order: Vec<NodeIndex> = vec![];
        let mut stack = vec![root_idx];
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            order.push(node);
            for dep in self.graph.neighbors_directed(node, Direction::Incoming) {
                if !seen.contains(&dep) {
                    stack.push(dep);
                }
            }
        }
        order.iter().map(|&idx| self.graph[idx].clone()).collect()
    }

    /// Returns every beam that transitively depends on `beam` (its full
    /// downstream closure), following outgoing edges. Excludes `beam` itself.
    ///
    /// Used to cancel the whole dependent subtree when a beam fails, so that a
    /// grandchild never runs after its intermediate prerequisite was cancelled.
    pub fn transitive_dependents(&self, beam: &str) -> Vec<String> {
        let start = match self.index.get(beam) {
            Some(&idx) => idx,
            None => return vec![],
        };
        let mut seen: HashSet<NodeIndex> = HashSet::new();
        let mut order: Vec<NodeIndex> = vec![];
        // `start` is marked seen without being collected: we want its
        // dependents, not itself. Iterative traversal to stay safe on long chains.
        seen.insert(start);
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for dependent in self.graph.neighbors_directed(node, Direction::Outgoing) {
                if seen.insert(dependent) {
                    order.push(dependent);
                    stack.push(dependent);
                }
            }
        }
        order.iter().map(|&idx| self.graph[idx].clone()).collect()
    }

    /// Returns the beams that directly depend on `beam` (its immediate dependents).
    pub fn direct_dependents(&self, beam: &str) -> Vec<String> {
        let idx = match self.index.get(beam) {
            Some(&idx) => idx,
            None => return vec![],
        };
        self.graph
            .neighbors_directed(idx, Direction::Outgoing)
            .map(|n| self.graph[n].clone())
            .collect()
    }

    /// Returns the beams that `beam` directly depends on (its immediate dependencies).
    pub fn direct_dependencies(&self, beam: &str) -> Vec<String> {
        let idx = match self.index.get(beam) {
            Some(&idx) => idx,
            None => return vec![],
        };
        self.graph
            .neighbors_directed(idx, Direction::Incoming)
            .map(|n| self.graph[n].clone())
            .collect()
    }

    /// Returns execution levels for running `root` and all its dependencies.
    /// Each level is a Vec of beam names that can run in parallel.
    /// Levels are ordered: level[0] runs first, level[N] runs last.
    pub fn execution_levels(&self, root: &str) -> Result<Vec<Vec<String>>, DagError> {
        let nodes = self.transitive_deps(root);
        if nodes.is_empty() {
            return Ok(vec![]);
        }

        // Build a subgraph of just the relevant nodes
        let sub = self.subgraph(&nodes);

        // Check for cycles
        if is_cyclic_directed(&sub.graph) {
            return Err(DagError::Cycle(root.to_string()));
        }

        // Topological sort
        let sorted = toposort(&sub.graph, None).map_err(|_| DagError::Cycle(root.to_string()))?;

        // Compute the level (longest path from any source) for each node
        // Level = max(level of all incoming neighbors) + 1, or 0 if no incoming
        let mut levels: HashMap<NodeIndex, usize> = HashMap::new();
        for &node in &sorted {
            let max_incoming = sub
                .graph
                .neighbors_directed(node, Direction::Incoming)
                .filter_map(|dep| levels.get(&dep))
                .max()
                .copied();
            levels.insert(node, max_incoming.map(|l| l + 1).unwrap_or(0));
        }

        // Group nodes by level
        let max_level = *levels.values().max().unwrap_or(&0);
        let mut result: Vec<Vec<String>> = vec![vec![]; max_level + 1];
        for (node_idx, level) in &levels {
            result[*level].push(sub.graph[*node_idx].clone());
        }

        Ok(result)
    }

    /// Build a subgraph containing only the specified nodes and edges between them.
    fn subgraph(&self, nodes: &[String]) -> BeamGraph {
        let mut new_graph: DiGraph<String, ()> = DiGraph::new();
        let mut new_index: HashMap<String, NodeIndex> = HashMap::new();

        for name in nodes {
            let idx = new_graph.add_node(name.clone());
            new_index.insert(name.clone(), idx);
        }

        for name in nodes {
            if let Some(&src) = self.index.get(name) {
                for dep in self.graph.neighbors_directed(src, Direction::Incoming) {
                    let dep_name = &self.graph[dep];
                    if let (Some(&new_dep), Some(&new_beam)) =
                        (new_index.get(dep_name), new_index.get(name))
                    {
                        new_graph.add_edge(new_dep, new_beam, ());
                    }
                }
            }
        }

        BeamGraph {
            graph: new_graph,
            index: new_index,
        }
    }
}
