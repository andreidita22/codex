//! Semantic owner for agent progress observation and reduction.

mod contract;
mod progress;
mod wait;

pub use contract::AGENT_ACTIVE_WORK_KIND_NAMES;
pub use contract::AGENT_BLOCK_REASON_NAMES;
pub use contract::AGENT_PROGRESS_PHASE_NAMES;
pub use contract::INSPECT_AGENT_PROGRESS_TOOL_NAME;
pub use contract::WAIT_FOR_AGENT_PROGRESS_MATCH_REASON_NAMES;
pub use contract::WAIT_FOR_AGENT_PROGRESS_TOOL_NAME;
pub use progress::AgentActiveWork;
pub use progress::AgentActiveWorkKind;
pub use progress::AgentBlockReason;
pub use progress::AgentProgressPhase;
pub use progress::AgentProgressSnapshot;
pub use progress::ProgressRegistry;
pub use wait::WaitForAgentProgressMatchReason;
pub use wait::WaitObservationMoment;
pub use wait::classify_wait_observation;
