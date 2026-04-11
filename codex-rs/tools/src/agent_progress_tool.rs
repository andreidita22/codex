use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

pub fn create_inspect_agent_progress_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "target".to_string(),
            JsonSchema::string(Some(
                "Agent id or canonical task name to inspect (from spawn_agent).".to_string(),
            )),
        ),
        (
            "stalled_after_ms".to_string(),
            JsonSchema::number(Some(
                "Optional stall threshold in milliseconds. If the agent has not emitted meaningful progress since spawn or turn entry within this threshold, the result reports stalled=true."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "inspect_agent_progress".to_string(),
        description: "Inspect normalized live progress for an existing agent. Use this when you need to know whether a sub-agent has entered its turn, started meaningful work, is blocked on approval or user input, or appears stalled."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["target".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(inspect_agent_progress_output_schema()),
    })
}

pub fn create_wait_for_agent_progress_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "target".to_string(),
            JsonSchema::string(Some(
                "Agent id or canonical task name to wait on (from spawn_agent).".to_string(),
            )),
        ),
        (
            "since_seq".to_string(),
            JsonSchema::number(Some(
                "Optional material-progress sequence baseline. The wait completes when the agent's seq becomes greater than this value."
                    .to_string(),
            )),
        ),
        (
            "until_phases".to_string(),
            JsonSchema::array(
                JsonSchema::string(/*description*/ None),
                Some(
                    "Optional set of progress phases that should satisfy the wait immediately when observed."
                        .to_string(),
                ),
            ),
        ),
        (
            "timeout_ms".to_string(),
            JsonSchema::number(Some(
                "Optional timeout in milliseconds. Defaults to 30000, min 10000, max 3600000."
                    .to_string(),
            )),
        ),
        (
            "stalled_after_ms".to_string(),
            JsonSchema::number(Some(
                "Optional stall threshold in milliseconds used when computing the returned stalled field."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "wait_for_agent_progress".to_string(),
        description: "Wait for meaningful progress from an existing agent. Use this after an initial inspect to block until seq advances, a target phase is reached, or the timeout expires."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["target".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(wait_for_agent_progress_output_schema()),
    })
}

fn agent_status_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "string",
                "enum": ["pending_init", "running", "shutdown", "not_found"]
            },
            {
                "type": "object",
                "properties": {
                    "completed": {
                        "type": ["string", "null"]
                    }
                },
                "required": ["completed"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "errored": {
                        "type": "string"
                    }
                },
                "required": ["errored"],
                "additionalProperties": false
            }
        ]
    })
}

fn phase_enum_schema() -> Value {
    json!([
        "pending",
        "reasoning",
        "message_drafting",
        "command",
        "tool_call",
        "waiting_approval",
        "waiting_user_input",
        "completed",
        "errored",
        "interrupted",
        "shutdown"
    ])
}

fn agent_progress_snapshot_properties() -> Value {
    json!({
        "lifecycle_status": {
            "description": "Last known lifecycle status for the agent.",
            "allOf": [agent_status_output_schema()]
        },
        "phase": {
            "type": "string",
            "enum": phase_enum_schema(),
            "description": "Normalized current phase inferred from recent events."
        },
        "blocked_on": {
            "type": ["string", "null"],
            "enum": [
                "exec_approval",
                "patch_approval",
                "permissions_request",
                "user_input_request",
                "elicitation_request",
                null
            ],
            "description": "Blocking condition when the agent is waiting on an external decision."
        },
        "active_work": {
            "type": ["object", "null"],
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["reasoning", "message", "command", "tool"]
                },
                "label": {
                    "type": "string"
                }
            },
            "required": ["kind", "label"],
            "additionalProperties": false,
            "description": "Best-effort description of the work the agent appears to be doing right now."
        },
        "recent_updates": {
            "type": "array",
            "items": {
                "type": "string"
            },
            "description": "Recent summary-safe progress updates in chronological order."
        },
        "latest_visible_message": {
            "type": ["string", "null"],
            "description": "Latest visible assistant message fragment when available."
        },
        "final_message": {
            "type": ["string", "null"],
            "description": "Final assistant message recorded when the turn completed."
        },
        "error_message": {
            "type": ["string", "null"],
            "description": "Latest error text when the agent errored."
        },
        "ever_entered_turn": {
            "type": "boolean",
            "description": "Whether the agent has emitted a TurnStarted event since it was spawned or resumed."
        },
        "ever_reported_progress": {
            "type": "boolean",
            "description": "Whether the agent has emitted any meaningful work or blocker event beyond merely entering the turn."
        },
        "last_progress_age_ms": {
            "type": ["number", "null"],
            "description": "Milliseconds since the last material progress event, when known."
        },
        "seq": {
            "type": "number",
            "description": "Monotonic sequence number that advances only on material progress events."
        },
        "stalled": {
            "type": "boolean",
            "description": "Whether the agent appears stalled under the requested threshold."
        }
    })
}

fn canonical_target_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "thread_id": {
                "type": "string",
                "description": "Thread identifier for the inspected agent."
            },
            "task_name": {
                "type": "string",
                "description": "Canonical task name for the inspected agent when available, otherwise the thread id."
            },
            "nickname": {
                "type": ["string", "null"],
                "description": "User-facing nickname for the inspected agent when available."
            }
        },
        "required": ["thread_id", "task_name", "nickname"],
        "additionalProperties": false
    })
}

fn inspect_agent_progress_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "canonical_target": canonical_target_output_schema(),
            "lifecycle_status": agent_progress_snapshot_properties()["lifecycle_status"].clone(),
            "phase": agent_progress_snapshot_properties()["phase"].clone(),
            "blocked_on": agent_progress_snapshot_properties()["blocked_on"].clone(),
            "active_work": agent_progress_snapshot_properties()["active_work"].clone(),
            "recent_updates": agent_progress_snapshot_properties()["recent_updates"].clone(),
            "latest_visible_message": agent_progress_snapshot_properties()["latest_visible_message"].clone(),
            "final_message": agent_progress_snapshot_properties()["final_message"].clone(),
            "error_message": agent_progress_snapshot_properties()["error_message"].clone(),
            "ever_entered_turn": agent_progress_snapshot_properties()["ever_entered_turn"].clone(),
            "ever_reported_progress": agent_progress_snapshot_properties()["ever_reported_progress"].clone(),
            "last_progress_age_ms": agent_progress_snapshot_properties()["last_progress_age_ms"].clone(),
            "seq": agent_progress_snapshot_properties()["seq"].clone(),
            "stalled": agent_progress_snapshot_properties()["stalled"].clone()
        },
        "required": [
            "canonical_target",
            "lifecycle_status",
            "phase",
            "blocked_on",
            "active_work",
            "recent_updates",
            "latest_visible_message",
            "final_message",
            "error_message",
            "ever_entered_turn",
            "ever_reported_progress",
            "last_progress_age_ms",
            "seq",
            "stalled"
        ],
        "additionalProperties": false
    })
}

fn wait_for_agent_progress_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "message": {
                "type": "string"
            },
            "timed_out": {
                "type": "boolean"
            },
            "match_reason": {
                "type": "string",
                "enum": ["already_satisfied", "seq_advanced", "phase_matched", "timed_out"]
            },
            "canonical_target": canonical_target_output_schema(),
            "lifecycle_status": agent_progress_snapshot_properties()["lifecycle_status"].clone(),
            "phase": agent_progress_snapshot_properties()["phase"].clone(),
            "blocked_on": agent_progress_snapshot_properties()["blocked_on"].clone(),
            "active_work": agent_progress_snapshot_properties()["active_work"].clone(),
            "recent_updates": agent_progress_snapshot_properties()["recent_updates"].clone(),
            "latest_visible_message": agent_progress_snapshot_properties()["latest_visible_message"].clone(),
            "final_message": agent_progress_snapshot_properties()["final_message"].clone(),
            "error_message": agent_progress_snapshot_properties()["error_message"].clone(),
            "ever_entered_turn": agent_progress_snapshot_properties()["ever_entered_turn"].clone(),
            "ever_reported_progress": agent_progress_snapshot_properties()["ever_reported_progress"].clone(),
            "last_progress_age_ms": agent_progress_snapshot_properties()["last_progress_age_ms"].clone(),
            "seq": agent_progress_snapshot_properties()["seq"].clone(),
            "stalled": agent_progress_snapshot_properties()["stalled"].clone()
        },
        "required": [
            "message",
            "timed_out",
            "match_reason",
            "canonical_target",
            "lifecycle_status",
            "phase",
            "blocked_on",
            "active_work",
            "recent_updates",
            "latest_visible_message",
            "final_message",
            "error_message",
            "ever_entered_turn",
            "ever_reported_progress",
            "last_progress_age_ms",
            "seq",
            "stalled"
        ],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn inspect_agent_progress_tool_requires_target_and_exposes_snapshot_shape() {
        let ToolSpec::Function(ResponsesApiTool {
            parameters,
            output_schema,
            ..
        }) = create_inspect_agent_progress_tool()
        else {
            panic!("inspect_agent_progress should be a function tool");
        };
        let properties = parameters
            .properties
            .expect("inspect_agent_progress should use object params");
        assert!(properties.contains_key("target"));
        assert!(properties.contains_key("stalled_after_ms"));
        assert_eq!(parameters.required, Some(vec!["target".to_string()]));
        let output_schema = output_schema.expect("inspect_agent_progress output schema");
        assert_eq!(
            output_schema["properties"]["canonical_target"]["required"],
            json!(["thread_id", "task_name", "nickname"])
        );
        assert_eq!(
            output_schema["properties"]["phase"]["enum"],
            phase_enum_schema()
        );
        assert_eq!(
            output_schema["properties"]["ever_reported_progress"]["type"],
            json!("boolean")
        );
    }

    #[test]
    fn wait_for_agent_progress_tool_requires_target_and_exposes_match_shape() {
        let ToolSpec::Function(ResponsesApiTool {
            parameters,
            output_schema,
            ..
        }) = create_wait_for_agent_progress_tool()
        else {
            panic!("wait_for_agent_progress should be a function tool");
        };
        let properties = parameters
            .properties
            .expect("wait_for_agent_progress should use object params");
        assert!(properties.contains_key("target"));
        assert!(properties.contains_key("since_seq"));
        assert!(properties.contains_key("until_phases"));
        assert!(properties.contains_key("timeout_ms"));
        assert!(properties.contains_key("stalled_after_ms"));
        assert_eq!(parameters.required, Some(vec!["target".to_string()]));
        let output_schema = output_schema.expect("wait_for_agent_progress output schema");
        assert_eq!(
            output_schema["properties"]["match_reason"]["enum"],
            json!([
                "already_satisfied",
                "seq_advanced",
                "phase_matched",
                "timed_out"
            ])
        );
        assert_eq!(
            output_schema["properties"]["canonical_target"]["required"],
            json!(["thread_id", "task_name", "nickname"])
        );
    }
}
