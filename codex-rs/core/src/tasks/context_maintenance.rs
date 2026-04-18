use std::sync::Arc;

use super::SessionTask;
use super::SessionTaskContext;
use crate::codex::TurnContext;
use crate::state::TaskKind;
use codex_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;
use tracing::error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextMaintenanceMode {
    Refresh,
    Prune,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ContextMaintenanceTask {
    pub(crate) mode: ContextMaintenanceMode,
}

impl ContextMaintenanceTask {
    pub(crate) fn new(mode: ContextMaintenanceMode) -> Self {
        Self { mode }
    }
}

impl SessionTask for ContextMaintenanceTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Regular
    }

    fn span_name(&self) -> &'static str {
        match self.mode {
            ContextMaintenanceMode::Refresh => "session_task.context_maintenance.refresh",
            ContextMaintenanceMode::Prune => "session_task.context_maintenance.prune",
        }
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<SessionTaskContext>,
        ctx: Arc<TurnContext>,
        _input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        if cancellation_token.is_cancelled() {
            return None;
        }

        let session = session.clone_session();
        let result = match self.mode {
            ContextMaintenanceMode::Refresh => {
                crate::context_maintenance::run_refresh(session.clone(), ctx.clone()).await
            }
            ContextMaintenanceMode::Prune => {
                crate::context_maintenance::run_prune(session.clone(), ctx.clone()).await
            }
        };

        if let Err(err) = result {
            error!("context maintenance task failed: {err}");
            session
                .send_event(
                    &ctx,
                    codex_protocol::protocol::EventMsg::Error(err.to_error_event(None)),
                )
                .await;
        }
        None
    }
}
