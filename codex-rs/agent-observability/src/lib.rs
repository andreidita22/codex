//! Semantic owner for agent progress observation and reduction.

mod progress;
mod wait;

pub use progress::AgentActiveWork;
pub use progress::AgentActiveWorkKind;
pub use progress::AgentBlockReason;
pub use progress::AgentProgressPhase;
pub use progress::AgentProgressSnapshot;
pub use progress::ProgressRegistry;
pub use wait::WaitForAgentProgressMatchReason;
pub use wait::WaitObservationMoment;
pub use wait::classify_wait_observation;
