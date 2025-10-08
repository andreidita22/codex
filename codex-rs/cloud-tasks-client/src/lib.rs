mod api;

pub use api::ApplyOutcome;
pub use api::ApplyStatus;
pub use api::AttemptStatus;
pub use api::CloudBackend;
pub use api::CloudTaskError;
pub use api::CreateTaskReq;
pub use api::CreatedTask;
pub use api::DiffSummary;
pub use api::Result;
pub use api::TaskFeed;
pub use api::TaskId;
pub use api::TaskKind;
pub use api::TaskListPage;
pub use api::TaskListRequest;
pub use api::TaskListSort;
pub use api::TaskStatus;
pub use api::TaskSummary;
pub use api::TaskText;
pub use api::TurnAttempt;
pub use api::TurnHistoryEntry;

#[cfg(feature = "mock")]
mod mock;

#[cfg(feature = "online")]
mod http;

#[cfg(feature = "mock")]
pub use mock::MockClient;

#[cfg(feature = "online")]
pub use http::HttpClient;

// Reusable apply engine now lives in the shared crate `codex-git-apply`.
