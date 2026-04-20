//! Semantic owner for agent progress observation and reduction.

mod progress;

pub use progress::AgentActiveWork;
pub use progress::AgentActiveWorkKind;
pub use progress::AgentBlockReason;
pub use progress::AgentProgressPhase;
pub use progress::AgentProgressSnapshot;
pub use progress::ProgressRegistry;
