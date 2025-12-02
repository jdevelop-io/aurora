//! Task scheduling for beam execution.

use aurora_core::Result;

use crate::dag::DependencyGraph;

/// Schedules beam execution based on dependencies.
pub struct Scheduler {
    /// The dependency graph.
    graph: DependencyGraph,

    /// Maximum number of parallel beams.
    max_parallelism: usize,
}

/// Represents a planned execution order.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Beams grouped by execution level.
    /// Beams within the same level can be executed in parallel.
    pub levels: Vec<ExecutionLevel>,
}

/// A group of beams that can be executed in parallel.
#[derive(Debug, Clone)]
pub struct ExecutionLevel {
    /// Beam names in this level.
    pub beams: Vec<String>,
}

impl Scheduler {
    /// Creates a new scheduler from a dependency graph.
    pub fn new(graph: DependencyGraph) -> Self {
        Self {
            graph,
            max_parallelism: num_cpus::get(),
        }
    }

    /// Sets the maximum parallelism.
    pub fn with_max_parallelism(mut self, max: usize) -> Self {
        self.max_parallelism = max.max(1);
        self
    }

    /// Creates an execution plan for a target beam.
    pub fn execution_plan(&self, target: &str) -> Result<ExecutionPlan> {
        let levels = self.graph.parallel_levels(target)?;

        let execution_levels = levels
            .into_iter()
            .map(|beams| {
                // Split large levels based on max_parallelism
                ExecutionLevel { beams }
            })
            .collect();

        Ok(ExecutionPlan {
            levels: execution_levels,
        })
    }

    /// Returns the maximum parallelism setting.
    pub fn max_parallelism(&self) -> usize {
        self.max_parallelism
    }
}

impl ExecutionPlan {
    /// Returns the total number of beams in the plan.
    pub fn total_beams(&self) -> usize {
        self.levels.iter().map(|l| l.beams.len()).sum()
    }

    /// Returns all beam names in execution order.
    pub fn all_beams(&self) -> Vec<&str> {
        self.levels
            .iter()
            .flat_map(|l| l.beams.iter().map(|s| s.as_str()))
            .collect()
    }
}

impl ExecutionLevel {
    /// Returns true if this level has multiple beams (can parallelize).
    pub fn is_parallel(&self) -> bool {
        self.beams.len() > 1
    }
}
