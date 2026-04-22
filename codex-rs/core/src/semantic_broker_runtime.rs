use crate::session::turn_context::TurnContext;
use crate::tools::ToolRouter;
use codex_features::Feature;
use codex_protocol::models::ResponseItem;
use codex_semantic_broker::BrokerInput;
use codex_semantic_broker::DeterministicAdjudicator;
use codex_semantic_broker::PacketBudget;
use codex_semantic_broker::adjudicate_candidates;
use codex_semantic_broker::build_active_context_packet;
use codex_semantic_broker::build_candidate_set;
use codex_semantic_broker::builtin_registry;
use codex_semantic_broker::render_active_context_packet_item;
use tracing::warn;

pub(crate) fn append_semantic_broker_prompt_overlay(
    mut prompt_input: Vec<ResponseItem>,
    router: &ToolRouter,
    turn_context: &TurnContext,
) -> Vec<ResponseItem> {
    if !turn_context.features.enabled(Feature::SemanticBroker) {
        return prompt_input;
    }

    let registry = match builtin_registry() {
        Ok(registry) => registry,
        Err(err) => {
            warn!("semantic broker disabled for this prompt build: {err}");
            return prompt_input;
        }
    };

    let broker_input = BrokerInput {
        current_turn_text: current_turn_text(&prompt_input),
        visible_tool_names: visible_tool_names_for_prompt(router, turn_context),
        session_source: Some(turn_context.session_source.clone()),
        active_track: None,
    };
    let candidates = build_candidate_set(&broker_input, &registry);
    let resolution = adjudicate_candidates(
        &broker_input,
        &candidates,
        &DeterministicAdjudicator::default(),
    );
    let packet = build_active_context_packet(&broker_input, &resolution);
    prompt_input.push(render_active_context_packet_item(
        &packet,
        &PacketBudget::default(),
    ));
    prompt_input
}

fn current_turn_text(prompt_input: &[ResponseItem]) -> Option<String> {
    prompt_input.iter().rev().find_map(|item| match item {
        ResponseItem::Message { role, content, .. } if role == "user" => {
            codex_protocol::models::content_items_to_text(content)
        }
        _ => None,
    })
}

fn visible_tool_names_for_prompt(router: &ToolRouter, turn_context: &TurnContext) -> Vec<String> {
    let deferred_dynamic_tools = turn_context
        .dynamic_tools
        .iter()
        .filter(|tool| tool.defer_loading)
        .map(|tool| tool.name.as_str())
        .collect::<std::collections::HashSet<_>>();

    let mut tool_names = router
        .model_visible_specs()
        .into_iter()
        .filter(|spec| !deferred_dynamic_tools.contains(spec.name()))
        .map(|spec| spec.name().to_string())
        .collect::<Vec<_>>();
    tool_names.sort();
    tool_names
}
