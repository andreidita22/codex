use serde::Deserialize;
use serde::Serialize;

pub(crate) const SCHEMA: &str = "continuation_bridge_v2";
pub(crate) const LEGACY_SCHEMA: &str = "continuation_bridge_v1";
pub(crate) const PROMPT: &str =
    include_str!("../../templates/continuation_bridge/variants/rich_review/prompt.md");
pub(crate) const OUTPUT_SCHEMA: &str =
    include_str!("../../templates/continuation_bridge/variants/rich_review/schema.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewBridge {
    #[serde(default = "default_schema")]
    pub schema: String,
    #[serde(default)]
    pub task: RichReviewTask,
    #[serde(default)]
    pub repo_identity: RichReviewRepoIdentity,
    #[serde(default)]
    pub state: RichReviewListSection,
    #[serde(default)]
    pub blocking_state: RichReviewBlockingState,
    #[serde(default)]
    pub artifacts: RichReviewArtifacts,
    #[serde(default)]
    pub active_subagents: Vec<RichReviewSubagent>,
    #[serde(default)]
    pub key_claims_with_evidence: Vec<RichReviewClaim>,
    #[serde(default)]
    pub invariants: RichReviewInvariants,
    #[serde(default)]
    pub epistemics: RichReviewEpistemics,
    #[serde(default)]
    pub provenance: RichReviewProvenance,
    #[serde(default)]
    pub working_thesis: RichReviewWorkingThesis,
    #[serde(default)]
    pub recommended_output_shape: Vec<String>,
    #[serde(default)]
    pub next: RichReviewNext,
}

impl Default for RichReviewBridge {
    fn default() -> Self {
        Self {
            schema: default_schema(),
            task: RichReviewTask::default(),
            repo_identity: RichReviewRepoIdentity::default(),
            state: RichReviewListSection::default(),
            blocking_state: RichReviewBlockingState::default(),
            artifacts: RichReviewArtifacts::default(),
            active_subagents: Vec::new(),
            key_claims_with_evidence: Vec::new(),
            invariants: RichReviewInvariants::default(),
            epistemics: RichReviewEpistemics::default(),
            provenance: RichReviewProvenance::default(),
            working_thesis: RichReviewWorkingThesis::default(),
            recommended_output_shape: Vec::new(),
            next: RichReviewNext::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewTask {
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub current_phase: String,
    #[serde(default)]
    pub success_condition: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewRepoIdentity {
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
pub struct RichReviewListSection {
    #[serde(default)]
    pub completed: Vec<String>,
    #[serde(default)]
    pub in_progress: Vec<String>,
    #[serde(default)]
    pub not_started: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewBlockingState {
    #[serde(default)]
    pub blocking: Vec<RichReviewBlocker>,
    #[serde(default)]
    pub non_blocking: Vec<String>,
    #[serde(default)]
    pub optional_followups: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewBlocker {
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
pub struct RichReviewArtifacts {
    #[serde(default)]
    pub files_touched: Vec<String>,
    #[serde(default)]
    pub authoritative_files: Vec<String>,
    #[serde(default)]
    pub partial_implementations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewSubagent {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub blocking: bool,
    #[serde(default)]
    pub last_result_summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewClaim {
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub ready_for_output: bool,
    #[serde(default)]
    pub evidence: Vec<RichReviewEvidence>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewEvidence {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub why_it_supports_claim: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewInvariants {
    #[serde(default)]
    pub must_preserve: Vec<String>,
    #[serde(default)]
    pub must_not_do: Vec<String>,
    #[serde(default)]
    pub assumptions_in_force: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewEpistemics {
    #[serde(default)]
    pub known_uncertainties: Vec<String>,
    #[serde(default)]
    pub questions_already_resolved: Vec<String>,
    #[serde(default)]
    pub questions_still_open: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewProvenance {
    #[serde(default)]
    pub why_current_code_looks_like_this: Vec<String>,
    #[serde(default)]
    pub rejected_paths: Vec<String>,
    #[serde(default)]
    pub pending_decisions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewWorkingThesis {
    #[serde(default)]
    pub current_best_answer: String,
    #[serde(default)]
    pub main_caveats: Vec<String>,
    #[serde(default)]
    pub likely_conclusion: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichReviewNext {
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
