use crate::packet::ACTIVE_CONTEXT_PACKET_SCHEMA;
use crate::packet::ACTIVE_CONTEXT_PACKET_TAG;
use crate::packet::ActiveContextPacket;
use crate::packet::PacketBudget;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct RenderedPacket<'a> {
    packet_schema: &'static str,
    packet_id: &'a str,
    resolution: &'a crate::packet::BrokerResolutionSummary,
    active_schema: &'a Option<crate::packet::ResolvedSchemaRef>,
    active_operator: &'a Option<crate::packet::ResolvedOperatorRef>,
    active_track: &'a Option<String>,
    tool_bindings: Vec<&'a str>,
    clarify: crate::adjudicator::ClarifyDisposition,
    lexical_events: &'a [crate::packet::LexicalEvolutionEvent],
    witness: &'a crate::adjudicator::BrokerWitness,
}

pub fn render_active_context_packet(packet: &ActiveContextPacket, budget: &PacketBudget) -> String {
    let payload = RenderedPacket {
        packet_schema: ACTIVE_CONTEXT_PACKET_SCHEMA,
        packet_id: &packet.packet_id.0,
        resolution: &packet.resolution,
        active_schema: &packet.active_schema,
        active_operator: &packet.active_operator,
        active_track: &packet.active_track,
        tool_bindings: packet
            .tool_bindings
            .iter()
            .take(budget.max_tool_bindings)
            .map(String::as_str)
            .collect(),
        clarify: packet.clarify,
        lexical_events: &packet.lexical_events,
        witness: &packet.witness,
    };
    let payload_json = match serde_json::to_string(&payload) {
        Ok(payload_json) => payload_json,
        Err(err) => panic!("active context packet payload should serialize: {err}"),
    };
    let rendered = format!(
        "<{ACTIVE_CONTEXT_PACKET_TAG} schema=\"{ACTIVE_CONTEXT_PACKET_SCHEMA}\">{payload_json}</{ACTIVE_CONTEXT_PACKET_TAG}>"
    );
    if rendered.len() <= budget.max_total_packet_chars {
        rendered
    } else {
        format!(
            "<{ACTIVE_CONTEXT_PACKET_TAG} schema=\"{ACTIVE_CONTEXT_PACKET_SCHEMA}\">{{\"packet_id\":\"{}\",\"resolution\":{{\"decision\":\"packet_truncated\"}}}}</{ACTIVE_CONTEXT_PACKET_TAG}>",
            packet.packet_id.0
        )
    }
}

pub fn render_active_context_packet_item(
    packet: &ActiveContextPacket,
    budget: &PacketBudget,
) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: render_active_context_packet(packet, budget),
        }],
        end_turn: None,
        phase: None,
    }
}

pub fn is_active_context_packet_text(text: &str) -> bool {
    text.contains(&format!("<{ACTIVE_CONTEXT_PACKET_TAG}"))
        && text.contains(&format!("</{ACTIVE_CONTEXT_PACKET_TAG}>"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adjudicator::BrokerWitness;
    use crate::adjudicator::ClarifyDisposition;
    use crate::packet::BrokerResolutionSummary;
    use crate::packet::ConfidenceDecision;
    use crate::packet::PacketId;
    use pretty_assertions::assert_eq;

    #[test]
    fn renders_tagged_developer_packet() {
        let packet = ActiveContextPacket {
            packet_schema: ACTIVE_CONTEXT_PACKET_SCHEMA,
            packet_id: PacketId("v0:test".to_string()),
            resolution: BrokerResolutionSummary {
                decision: ConfidenceDecision::Resolved,
                selected_schema_id: None,
                selected_operator_id: None,
            },
            active_schema: None,
            active_operator: None,
            active_track: None,
            tool_bindings: vec!["shell".to_string()],
            clarify: ClarifyDisposition::None,
            lexical_events: Vec::new(),
            witness: BrokerWitness {
                adjudicator: "deterministic_v0".to_string(),
                candidate_count: 1,
                matched_terms: vec!["review".to_string()],
            },
        };
        let rendered = render_active_context_packet(&packet, &PacketBudget::default());
        assert!(is_active_context_packet_text(&rendered));
        assert!(rendered.starts_with(&format!(
            "<{ACTIVE_CONTEXT_PACKET_TAG} schema=\"{ACTIVE_CONTEXT_PACKET_SCHEMA}\">"
        )));
        assert!(rendered.ends_with(&format!("</{ACTIVE_CONTEXT_PACKET_TAG}>")));

        let item = render_active_context_packet_item(&packet, &PacketBudget::default());
        let ResponseItem::Message { role, content, .. } = item else {
            panic!("expected developer message");
        };
        assert_eq!(role, "developer");
        assert_eq!(content.len(), 1);
    }

    #[test]
    fn truncates_when_budget_is_tiny() {
        let packet = ActiveContextPacket {
            packet_schema: ACTIVE_CONTEXT_PACKET_SCHEMA,
            packet_id: PacketId("v0:test".to_string()),
            resolution: BrokerResolutionSummary {
                decision: ConfidenceDecision::Resolved,
                selected_schema_id: None,
                selected_operator_id: None,
            },
            active_schema: None,
            active_operator: None,
            active_track: None,
            tool_bindings: vec!["shell".to_string(); 10],
            clarify: ClarifyDisposition::None,
            lexical_events: Vec::new(),
            witness: BrokerWitness {
                adjudicator: "deterministic_v0".to_string(),
                candidate_count: 1,
                matched_terms: vec!["review".to_string()],
            },
        };
        let rendered = render_active_context_packet(
            &packet,
            &PacketBudget {
                max_tool_bindings: 1,
                max_total_packet_chars: 40,
            },
        );
        assert!(rendered.contains("packet_truncated"));
    }
}
