use crate::config::GovernancePathVariant;
use crate::context::CollaborationModeInstructions;
use crate::context::ContextualUserFragment;
use crate::context::EnvironmentContext;
use crate::context::ModelSwitchInstructions;
use crate::context::PermissionsInstructions;
use crate::context::PersonalitySpecInstructions;
use crate::context::RealtimeEndInstructions;
use crate::context::RealtimeStartInstructions;
use crate::context::RealtimeStartWithInstructions;
use crate::session::PreviousTurnSettings;
use crate::session::turn_context::TurnContext;
use crate::shell::Shell;
use codex_execpolicy::Policy;
use codex_features::Feature;
use codex_protocol::config_types::Personality;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TurnContextItem;

fn build_environment_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
    shell: &Shell,
) -> Option<ResponseItem> {
    if !next.config.include_environment_context {
        return None;
    }

    let prev = previous?;
    let prev_context = EnvironmentContext::from_turn_context_item(prev, shell.name().to_string());
    let next_context = EnvironmentContext::from_turn_context(next, shell);
    if prev_context.equals_except_shell(&next_context) {
        return None;
    }

    Some(ContextualUserFragment::into(
        EnvironmentContext::diff_from_turn_context_item(prev, &next_context),
    ))
}

fn build_permissions_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
    exec_policy: &Policy,
) -> Option<String> {
    if !next.config.include_permissions_instructions {
        return None;
    }

    let prev = previous?;
    if prev.permission_profile() == next.permission_profile()
        && prev.approval_policy == next.approval_policy.value()
    {
        return None;
    }

    Some(
        PermissionsInstructions::from_policy(
            next.sandbox_policy.get(),
            next.approval_policy.value(),
            next.config.approvals_reviewer,
            exec_policy,
            &next.cwd,
            next.features.enabled(Feature::ExecPermissionApprovals),
            next.features.enabled(Feature::RequestPermissionsTool),
        )
        .render(),
    )
}

fn build_collaboration_mode_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
) -> Option<String> {
    let prev = previous?;
    if prev.collaboration_mode.as_ref() != Some(&next.collaboration_mode) {
        // If the next mode has empty developer instructions, this returns None and we emit no
        // update, so prior collaboration instructions remain in the prompt history.
        Some(
            CollaborationModeInstructions::from_collaboration_mode(&next.collaboration_mode)?
                .render(),
        )
    } else {
        None
    }
}

pub(crate) fn build_realtime_update_item(
    previous: Option<&TurnContextItem>,
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
) -> Option<String> {
    match (
        previous.and_then(|item| item.realtime_active),
        next.realtime_active,
    ) {
        (Some(true), false) => Some(RealtimeEndInstructions::new("inactive").render()),
        (Some(false), true) | (None, true) => Some(
            if let Some(instructions) = next
                .config
                .experimental_realtime_start_instructions
                .as_deref()
            {
                RealtimeStartWithInstructions::new(instructions).render()
            } else {
                RealtimeStartInstructions.render()
            },
        ),
        (Some(true), true) | (Some(false), false) => None,
        (None, false) => previous_turn_settings
            .and_then(|settings| settings.realtime_active)
            .filter(|realtime_active| *realtime_active)
            .map(|_| RealtimeEndInstructions::new("inactive").render()),
    }
}

pub(crate) fn build_initial_realtime_item(
    previous: Option<&TurnContextItem>,
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
) -> Option<String> {
    build_realtime_update_item(previous, previous_turn_settings, next)
}

fn build_personality_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
    personality_feature_enabled: bool,
) -> Option<String> {
    if !personality_feature_enabled {
        return None;
    }
    let previous = previous?;
    if next.model_info.slug != previous.model {
        return None;
    }

    if let Some(personality) = next.personality
        && next.personality != previous.personality
    {
        let model_info = &next.model_info;
        let personality_message = personality_message_for(model_info, personality);
        personality_message.map(|message| PersonalitySpecInstructions::new(message).render())
    } else {
        None
    }
}

pub(crate) fn personality_message_for(
    model_info: &ModelInfo,
    personality: Personality,
) -> Option<String> {
    model_info
        .model_messages
        .as_ref()
        .and_then(|spec| spec.get_personality_message(Some(personality)))
        .filter(|message| !message.is_empty())
}

pub(crate) fn build_model_instructions_update_item(
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
) -> Option<String> {
    let previous_turn_settings = previous_turn_settings?;
    if previous_turn_settings.model == next.model_info.slug {
        return None;
    }

    let model_instructions = next.model_info.get_model_instructions(next.personality);
    if model_instructions.is_empty() {
        return None;
    }

    Some(ModelSwitchInstructions::new(model_instructions).render())
}

pub(crate) fn build_developer_update_item(text_sections: Vec<String>) -> Option<ResponseItem> {
    build_text_message("developer", text_sections)
}

pub(crate) fn build_contextual_user_message(text_sections: Vec<String>) -> Option<ResponseItem> {
    build_text_message("user", text_sections)
}

fn build_text_message(role: &str, text_sections: Vec<String>) -> Option<ResponseItem> {
    if text_sections.is_empty() {
        return None;
    }

    let content = text_sections
        .into_iter()
        .map(|text| ContentItem::InputText { text })
        .collect();

    Some(ResponseItem::Message {
        id: None,
        role: role.to_string(),
        content,
        end_turn: None,
        phase: None,
    })
}

pub(crate) struct SettingsUpdateBuildOptions<'a> {
    pub(crate) personality_feature_enabled: bool,
    pub(crate) governance_path_variant: GovernancePathVariant,
    pub(crate) base_instructions: &'a str,
    pub(crate) full_role_sections: &'a [String],
    pub(crate) full_task_sections: &'a [String],
    pub(crate) full_runtime_sections: &'a [String],
}

pub(crate) struct InitialContextSections {
    constitutional_sections: Vec<String>,
    developer_sections: Vec<String>,
    role_sections: Vec<String>,
    task_sections: Vec<String>,
    runtime_sections: Vec<String>,
    contextual_user_sections: Vec<String>,
}

impl InitialContextSections {
    pub(crate) fn new() -> Self {
        Self {
            constitutional_sections: Vec::with_capacity(1),
            developer_sections: Vec::with_capacity(8),
            role_sections: Vec::with_capacity(3),
            task_sections: Vec::with_capacity(3),
            runtime_sections: Vec::with_capacity(8),
            contextual_user_sections: Vec::with_capacity(2),
        }
    }

    pub(crate) fn push_constitutional_and_developer(&mut self, section: String) {
        self.constitutional_sections.push(section.clone());
        self.developer_sections.push(section);
    }

    pub(crate) fn push_role_and_developer(&mut self, section: String) {
        self.role_sections.push(section.clone());
        self.developer_sections.push(section);
    }

    pub(crate) fn push_task_and_developer(&mut self, section: String) {
        self.task_sections.push(section.clone());
        self.developer_sections.push(section);
    }

    pub(crate) fn push_task_only(&mut self, section: String) {
        self.task_sections.push(section);
    }

    pub(crate) fn push_runtime_and_developer(&mut self, section: String) {
        self.runtime_sections.push(section.clone());
        self.developer_sections.push(section);
    }

    pub(crate) fn push_contextual_user(&mut self, section: String) {
        self.contextual_user_sections.push(section);
    }

    pub(crate) fn maybe_insert_initial_context_prompt_layering_section(
        &mut self,
        path_variant: GovernancePathVariant,
        model: &str,
        session_source: &SessionSource,
        base_instructions: &str,
    ) {
        if let Some(prompt_layering_section) =
            crate::governance::prompt_layers::build_prompt_layering_section(
                crate::governance::prompt_layers::PromptLayeringInput {
                    path_variant,
                    phase: crate::governance::prompt_layers::PromptLayeringPhase::InitialContext,
                    model,
                    session_source,
                    base_instructions,
                    constitutional_sections: &self.constitutional_sections,
                    role_sections: &self.role_sections,
                    task_sections: &self.task_sections,
                    runtime_sections: &self.runtime_sections,
                },
            )
        {
            insert_governance_prompt_layering_section(
                &mut self.developer_sections,
                prompt_layering_section,
            );
        }
    }

    pub(crate) fn into_message_sections(self) -> (Vec<String>, Vec<String>) {
        (self.developer_sections, self.contextual_user_sections)
    }
}

pub(crate) fn insert_governance_prompt_layering_section(
    developer_sections: &mut Vec<String>,
    prompt_layering_section: String,
) {
    let insert_index = if developer_sections
        .first()
        .is_some_and(|section| section.trim_start().starts_with("<model_switch>"))
    {
        1
    } else {
        0
    };
    developer_sections.insert(insert_index, prompt_layering_section);
}

pub(crate) fn build_settings_update_items(
    previous: Option<&TurnContextItem>,
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
    shell: &Shell,
    exec_policy: &Policy,
    options: SettingsUpdateBuildOptions<'_>,
) -> Vec<ResponseItem> {
    // TODO(ccunningham): build_settings_update_items still does not cover every
    // model-visible item emitted by build_initial_context. Persist the remaining
    // inputs or add explicit replay events so fork/resume can diff everything
    // deterministically.
    let contextual_user_message = build_environment_update_item(previous, next, shell);
    let model_switch_item = build_model_instructions_update_item(previous_turn_settings, next);
    let permissions_item = build_permissions_update_item(previous, next, exec_policy);
    let collaboration_mode_item = build_collaboration_mode_update_item(previous, next);
    let realtime_item = build_realtime_update_item(previous, previous_turn_settings, next);
    let personality_item =
        build_personality_update_item(previous, next, options.personality_feature_enabled);

    let mut constitutional_sections = Vec::with_capacity(1);
    let mut developer_update_sections = Vec::with_capacity(6);

    if let Some(model_switch_item) = model_switch_item {
        constitutional_sections.push(model_switch_item.clone());
        developer_update_sections.push(model_switch_item);
    }

    if let Some(permissions_item) = permissions_item {
        developer_update_sections.push(permissions_item);
    }

    if let Some(collaboration_mode_item) = collaboration_mode_item {
        developer_update_sections.push(collaboration_mode_item);
    }

    if let Some(realtime_item) = realtime_item {
        developer_update_sections.push(realtime_item);
    }

    if let Some(personality_item) = personality_item {
        developer_update_sections.push(personality_item);
    }

    if !developer_update_sections.is_empty()
        && let Some(prompt_layering_section) =
            crate::governance::prompt_layers::build_prompt_layering_section(
                crate::governance::prompt_layers::PromptLayeringInput {
                    path_variant: options.governance_path_variant,
                    phase: crate::governance::prompt_layers::PromptLayeringPhase::SettingsUpdate,
                    model: &next.model_info.slug,
                    session_source: &next.session_source,
                    base_instructions: options.base_instructions,
                    constitutional_sections: &constitutional_sections,
                    role_sections: options.full_role_sections,
                    task_sections: options.full_task_sections,
                    runtime_sections: options.full_runtime_sections,
                },
            )
    {
        insert_governance_prompt_layering_section(
            &mut developer_update_sections,
            prompt_layering_section,
        );
    }

    let mut items = Vec::with_capacity(2);
    if let Some(developer_message) = build_developer_update_item(developer_update_sections) {
        items.push(developer_message);
    }
    if let Some(contextual_user_message) = contextual_user_message {
        items.push(contextual_user_message);
    }
    items
}
