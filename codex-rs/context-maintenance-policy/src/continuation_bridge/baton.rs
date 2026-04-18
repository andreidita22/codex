use serde::Deserialize;
use serde::Serialize;

pub(crate) const SCHEMA: &str = "continuation_bridge_baton_v1";
pub(crate) const PROMPT: &str =
    include_str!("../../templates/continuation_bridge/variants/baton/prompt.md");
pub(crate) const OUTPUT_SCHEMA: &str =
    include_str!("../../templates/continuation_bridge/variants/baton/schema.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonBridge {
    #[serde(default = "default_schema")]
    pub schema: String,
    #[serde(default)]
    pub task: BatonTask,
    #[serde(default)]
    pub repo_identity: BatonRepoIdentity,
    #[serde(default)]
    pub state: BatonState,
    #[serde(default)]
    pub blocking_state: BatonBlockingState,
    #[serde(default)]
    pub artifacts: BatonArtifacts,
    #[serde(default)]
    pub active_subagents: Vec<BatonSubagent>,
    #[serde(default)]
    pub working_thesis: BatonWorkingThesis,
    #[serde(default)]
    pub next: BatonNext,
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
pub struct BatonTask {
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub current_phase: String,
    #[serde(default)]
    pub success_condition: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonRepoIdentity {
    #[serde(default)]
    pub repo_root: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub head_commit: String,
    #[serde(default)]
    pub worktree_dirty: bool,
    #[serde(default)]
    pub dirty_files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonState {
    #[serde(default)]
    pub completed: Vec<String>,
    #[serde(default)]
    pub in_progress: Vec<String>,
    #[serde(default)]
    pub remaining: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonBlockingState {
    #[serde(default)]
    pub blocking: Vec<BatonBlocker>,
    #[serde(default)]
    pub human_actions_pending: Vec<BatonHumanAction>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonBlocker {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub unblocks: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonHumanAction {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub blocking: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonArtifacts {
    #[serde(default)]
    pub authoritative_files: Vec<String>,
    #[serde(default)]
    pub partial_implementations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonSubagent {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub blocking: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonWorkingThesis {
    #[serde(default)]
    pub current_best_answer: String,
    #[serde(default)]
    pub main_caveats: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatonNext {
    #[serde(default)]
    pub immediate_next_action: String,
    #[serde(default)]
    pub fallback_if_blocked: String,
    #[serde(default)]
    pub validation_step: String,
}

pub(crate) fn default_schema() -> String {
    SCHEMA.to_string()
}
