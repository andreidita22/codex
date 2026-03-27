use serde::Deserialize;
use serde::Serialize;

pub(crate) const SCHEMA: &str = "continuation_bridge_v2";
pub(crate) const LEGACY_SCHEMA: &str = "continuation_bridge_v1";
pub(crate) const PROMPT: &str =
    include_str!("../../templates/continuation_bridge/variants/rich_review/prompt.md");
pub(crate) const OUTPUT_SCHEMA: &str =
    include_str!("../../templates/continuation_bridge/variants/rich_review/schema.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewBridge {
    #[serde(default = "default_schema")]
    pub(crate) schema: String,
    #[serde(default)]
    pub(crate) task: RichReviewTask,
    #[serde(default)]
    pub(crate) repo_identity: RichReviewRepoIdentity,
    #[serde(default)]
    pub(crate) state: RichReviewListSection,
    #[serde(default)]
    pub(crate) blocking_state: RichReviewBlockingState,
    #[serde(default)]
    pub(crate) artifacts: RichReviewArtifacts,
    #[serde(default)]
    pub(crate) active_subagents: Vec<RichReviewSubagent>,
    #[serde(default)]
    pub(crate) key_claims_with_evidence: Vec<RichReviewClaim>,
    #[serde(default)]
    pub(crate) invariants: RichReviewInvariants,
    #[serde(default)]
    pub(crate) epistemics: RichReviewEpistemics,
    #[serde(default)]
    pub(crate) provenance: RichReviewProvenance,
    #[serde(default)]
    pub(crate) working_thesis: RichReviewWorkingThesis,
    #[serde(default)]
    pub(crate) recommended_output_shape: Vec<String>,
    #[serde(default)]
    pub(crate) next: RichReviewNext,
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
pub(crate) struct RichReviewTask {
    #[serde(default)]
    pub(crate) objective: String,
    #[serde(default)]
    pub(crate) current_phase: String,
    #[serde(default)]
    pub(crate) success_condition: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewRepoIdentity {
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
pub(crate) struct RichReviewListSection {
    #[serde(default)]
    pub(crate) completed: Vec<String>,
    #[serde(default)]
    pub(crate) in_progress: Vec<String>,
    #[serde(default)]
    pub(crate) not_started: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewBlockingState {
    #[serde(default)]
    pub(crate) blocking: Vec<RichReviewBlocker>,
    #[serde(default)]
    pub(crate) non_blocking: Vec<String>,
    #[serde(default)]
    pub(crate) optional_followups: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewBlocker {
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
pub(crate) struct RichReviewArtifacts {
    #[serde(default)]
    pub(crate) files_touched: Vec<String>,
    #[serde(default)]
    pub(crate) authoritative_files: Vec<String>,
    #[serde(default)]
    pub(crate) partial_implementations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewSubagent {
    #[serde(default)]
    pub(crate) agent_id: String,
    #[serde(default)]
    pub(crate) thread_id: String,
    #[serde(default)]
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) task: String,
    #[serde(default)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) blocking: bool,
    #[serde(default)]
    pub(crate) last_result_summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewClaim {
    #[serde(default)]
    pub(crate) claim: String,
    #[serde(default)]
    pub(crate) confidence: String,
    #[serde(default)]
    pub(crate) ready_for_output: bool,
    #[serde(default)]
    pub(crate) evidence: Vec<RichReviewEvidence>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewEvidence {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) line: u32,
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) why_it_supports_claim: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewInvariants {
    #[serde(default)]
    pub(crate) must_preserve: Vec<String>,
    #[serde(default)]
    pub(crate) must_not_do: Vec<String>,
    #[serde(default)]
    pub(crate) assumptions_in_force: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewEpistemics {
    #[serde(default)]
    pub(crate) known_uncertainties: Vec<String>,
    #[serde(default)]
    pub(crate) questions_already_resolved: Vec<String>,
    #[serde(default)]
    pub(crate) questions_still_open: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewProvenance {
    #[serde(default)]
    pub(crate) why_current_code_looks_like_this: Vec<String>,
    #[serde(default)]
    pub(crate) rejected_paths: Vec<String>,
    #[serde(default)]
    pub(crate) pending_decisions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewWorkingThesis {
    #[serde(default)]
    pub(crate) current_best_answer: String,
    #[serde(default)]
    pub(crate) main_caveats: Vec<String>,
    #[serde(default)]
    pub(crate) likely_conclusion: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RichReviewNext {
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
