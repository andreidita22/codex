use crate::codex::Session;
use crate::codex::TurnContext;
use crate::config::CURRENT_THREAD_MODEL_SELECTOR;
use crate::config::ContextMaintenanceReasoningEffort;
use codex_otel::SessionTelemetry;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ReasoningEffort;
use tracing::warn;

#[derive(Clone)]
pub(crate) struct ContextMaintenanceRequestContext {
    pub(crate) model_info: ModelInfo,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
}

fn requested_model_slug(turn_context: &TurnContext) -> Option<String> {
    match turn_context.config.context_maintenance_model.as_deref() {
        Some(selector) if selector.eq_ignore_ascii_case(CURRENT_THREAD_MODEL_SELECTOR) => None,
        Some(selector) => Some(selector.to_string()),
        None => turn_context.config.continuation_bridge_model.clone(),
    }
}

fn requested_reasoning_setting(turn_context: &TurnContext) -> ContextMaintenanceReasoningEffort {
    turn_context
        .config
        .context_maintenance_reasoning_effort
        .unwrap_or_else(|| {
            turn_context
                .config
                .continuation_bridge_reasoning_effort
                .map(|effort| match effort {
                    ReasoningEffort::None => ContextMaintenanceReasoningEffort::None,
                    ReasoningEffort::Minimal => ContextMaintenanceReasoningEffort::Minimal,
                    ReasoningEffort::Low => ContextMaintenanceReasoningEffort::Low,
                    ReasoningEffort::Medium => ContextMaintenanceReasoningEffort::Medium,
                    ReasoningEffort::High => ContextMaintenanceReasoningEffort::High,
                    ReasoningEffort::XHigh => ContextMaintenanceReasoningEffort::XHigh,
                })
                .unwrap_or(ContextMaintenanceReasoningEffort::CurrentThread)
        })
}

fn resolved_reasoning_effort(
    model_info: &ModelInfo,
    fallback_effort: Option<ReasoningEffort>,
    requested: ContextMaintenanceReasoningEffort,
) -> Option<ReasoningEffort> {
    let requested_effort = requested.resolve(fallback_effort);
    let supported_reasoning_levels = model_info
        .supported_reasoning_levels
        .iter()
        .map(|preset| preset.effort)
        .collect::<Vec<_>>();
    if let Some(requested_effort) = requested_effort
        && supported_reasoning_levels.contains(&requested_effort)
    {
        return Some(requested_effort);
    }

    if requested != ContextMaintenanceReasoningEffort::CurrentThread {
        warn!(
            "context maintenance reasoning effort `{requested:?}` unsupported for model `{}`; falling back to model-compatible reasoning",
            model_info.slug
        );
    }

    fallback_effort
        .filter(|effort| supported_reasoning_levels.contains(effort))
        .or_else(|| {
            supported_reasoning_levels
                .get(supported_reasoning_levels.len().saturating_sub(1) / 2)
                .copied()
        })
        .or(model_info.default_reasoning_level)
}

pub(crate) async fn resolve_context_maintenance_request_context(
    sess: &Session,
    turn_context: &TurnContext,
) -> ContextMaintenanceRequestContext {
    let requested_reasoning = requested_reasoning_setting(turn_context);
    let resident_context = || ContextMaintenanceRequestContext {
        model_info: turn_context.model_info.clone(),
        session_telemetry: turn_context.session_telemetry.clone(),
        reasoning_effort: resolved_reasoning_effort(
            &turn_context.model_info,
            turn_context.reasoning_effort,
            requested_reasoning,
        ),
    };

    let Some(requested_model) = requested_model_slug(turn_context) else {
        return resident_context();
    };

    if requested_model == turn_context.model_info.slug {
        return resident_context();
    }

    let maintenance_turn = turn_context
        .with_model(requested_model.clone(), &sess.services.models_manager)
        .await;
    if maintenance_turn.model_info.slug != requested_model {
        warn!(
            "context maintenance model `{requested_model}` unavailable; falling back to resident model `{}`",
            turn_context.model_info.slug
        );
        return resident_context();
    }

    let reasoning_effort = resolved_reasoning_effort(
        &maintenance_turn.model_info,
        maintenance_turn.reasoning_effort,
        requested_reasoning,
    );
    ContextMaintenanceRequestContext {
        model_info: maintenance_turn.model_info,
        session_telemetry: maintenance_turn.session_telemetry,
        reasoning_effort,
    }
}
