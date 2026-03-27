use serde::Deserialize;
use serde::Serialize;

pub(crate) const SCHEMA: &str = "continuation_bridge_baton_v1";
pub(crate) const PROMPT: &str =
    include_str!("../../templates/continuation_bridge/variants/baton/prompt.md");
pub(crate) const OUTPUT_SCHEMA: &str =
    include_str!("../../templates/continuation_bridge/variants/baton/schema.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonBridge {
    #[serde(default = "default_schema")]
    pub(crate) schema: String,
    #[serde(default)]
    pub(crate) task: BatonTask,
    #[serde(default)]
    pub(crate) repo_identity: BatonRepoIdentity,
    #[serde(default)]
    pub(crate) state: BatonState,
    #[serde(default)]
    pub(crate) blocking_state: BatonBlockingState,
    #[serde(default)]
    pub(crate) artifacts: BatonArtifacts,
    #[serde(default)]
    pub(crate) active_subagents: Vec<BatonSubagent>,
    #[serde(default)]
    pub(crate) working_thesis: BatonWorkingThesis,
    #[serde(default)]
    pub(crate) next: BatonNext,
}

impl Default for BatonBridge {
    fn default() -> Self {
        Self {
            schema: default_schema(),
            task: BatonTask::default(),
            repo_identity: BatonRepoIdentity::default(),
            state: BatonState::default(),
            blocking_state: BatonBlockingState::default(),
            artifacts: BatonArtifacts::default(),
            active_subagents: Vec::new(),
            working_thesis: BatonWorkingThesis::default(),
            next: BatonNext::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonTask {
    #[serde(default)]
    pub(crate) objective: String,
    #[serde(default)]
    pub(crate) current_phase: String,
    #[serde(default)]
    pub(crate) success_condition: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonRepoIdentity {
    #[serde(default)]
    pub(crate) repo_root: String,
    #[serde(default)]
    pub(crate) branch: String,
    #[serde(default)]
    pub(crate) head_commit: String,
    #[serde(default)]
    pub(crate) worktree_dirty: bool,
    #[serde(default)]
    pub(crate) dirty_files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonState {
    #[serde(default)]
    pub(crate) completed: Vec<String>,
    #[serde(default)]
    pub(crate) in_progress: Vec<String>,
    #[serde(default)]
    pub(crate) remaining: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonBlockingState {
    #[serde(default)]
    pub(crate) blocking: Vec<BatonBlocker>,
    #[serde(default)]
    pub(crate) human_actions_pending: Vec<BatonHumanAction>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonBlocker {
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) reason: String,
    #[serde(default)]
    pub(crate) owner: String,
    #[serde(default)]
    pub(crate) unblocks: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonHumanAction {
    #[serde(default)]
    pub(crate) action: String,
    #[serde(default)]
    pub(crate) context: String,
    #[serde(default)]
    pub(crate) blocking: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonArtifacts {
    #[serde(default)]
    pub(crate) authoritative_files: Vec<String>,
    #[serde(default)]
    pub(crate) partial_implementations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonSubagent {
    #[serde(default)]
    pub(crate) agent_id: String,
    #[serde(default)]
    pub(crate) thread_id: String,
    #[serde(default)]
    pub(crate) nickname: String,
    #[serde(default)]
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) blocking: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonWorkingThesis {
    #[serde(default)]
    pub(crate) current_best_answer: String,
    #[serde(default)]
    pub(crate) main_caveats: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatonNext {
    #[serde(default)]
    pub(crate) immediate_next_action: String,
    #[serde(default)]
    pub(crate) fallback_if_blocked: String,
    #[serde(default)]
    pub(crate) validation_step: String,
}

pub(crate) fn default_schema() -> String {
    SCHEMA.to_string()
}
