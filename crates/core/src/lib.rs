//! Aurora Core - Core types and traits for the Aurora build system.

mod beam;
mod beamfile;
mod condition;
mod error;
mod hook;
pub mod interpolation;
mod variable;

pub use beam::{Beam, Command, RunBlock};
pub use beamfile::Beamfile;
pub use condition::Condition;
pub use error::{AuroraError, Result};
pub use hook::Hook;
pub use interpolation::{InterpolationContext, interpolate};
pub use variable::Variable;
