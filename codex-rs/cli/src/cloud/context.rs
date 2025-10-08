use std::sync::Arc;

use anyhow::Context as AnyhowContext;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codex_backend_client::Client as BackendClient;
use codex_backend_openapi_models::models::TaskListItem;
use codex_cloud_tasks::util::extract_chatgpt_account_id;
use codex_cloud_tasks::util::normalize_base_url;
use codex_cloud_tasks::util::set_user_agent_suffix;
use codex_cloud_tasks_client::CloudBackend;
use codex_cloud_tasks_client::CloudTaskError;
use codex_cloud_tasks_client::CreateTaskReq;
use codex_cloud_tasks_client::HttpClient;
use codex_cloud_tasks_client::MockClient;
use codex_cloud_tasks_client::TaskId;
use codex_cloud_tasks_client::TaskSummary;
use codex_cloud_tasks_client::TaskText;
use codex_cloud_tasks_client::TurnAttempt;
use codex_common::CliConfigOverrides;
use codex_login::AuthManager;

use crate::cloud::types::TaskData;
use crate::cloud::types::VariantData;

const UA_SUFFIX: &str = "codex_cloud_headless";

pub struct CloudContext {
    backend: Arc<dyn CloudBackend>,
    http_extras: Option<HttpExtras>,
    is_mock: bool,
}

struct HttpExtras {
    backend: BackendClient,
}

pub async fn build_context(overrides: CliConfigOverrides) -> Result<CloudContext> {
    CloudContext::new(overrides).await
}

impl CloudContext {
    async fn new(config_overrides: CliConfigOverrides) -> Result<Self> {
        set_user_agent_suffix(UA_SUFFIX);
        let use_mock = matches!(
            std::env::var("CODEX_CLOUD_TASKS_MODE").ok().as_deref(),
            Some("mock") | Some("MOCK")
        );
        if use_mock {
            return Ok(Self {
                backend: Arc::new(MockClient),
                http_extras: None,
                is_mock: true,
            });
        }

        let override_pairs = config_overrides.parse_overrides().map_err(|e| anyhow!(e))?;
        let config = codex_core::config::Config::load_with_cli_overrides(
            override_pairs,
            codex_core::config::ConfigOverrides::default(),
        )
        .await
        .with_context(|| "Failed to load configuration with CLI overrides")?;

        let raw_base_url = match std::env::var("CODEX_CLOUD_TASKS_BASE_URL") {
            Ok(val) if !val.trim().is_empty() => val,
            _ => config.chatgpt_base_url.clone(),
        };
        let base_url = normalize_base_url(&raw_base_url);
        let ua = codex_core::default_client::get_codex_user_agent();

        let mut client = HttpClient::new(base_url.clone())?.with_user_agent(ua.clone());
        let mut backend_client = BackendClient::new(base_url)?.with_user_agent(ua);

        let codex_home = config.codex_home.clone();
        let auth_manager = AuthManager::new(codex_home, false);
        let auth = auth_manager
            .auth()
            .ok_or_else(|| anyhow!("Not signed in. Run 'codex login' to sign in with ChatGPT."))?;
        let token = auth
            .get_token()
            .await
            .context("Failed to load ChatGPT session token")?;
        if token.is_empty() {
            bail!("Not signed in. Run 'codex login' to sign in with ChatGPT.");
        }
        client = client.with_bearer_token(token.clone());
        backend_client = backend_client.with_bearer_token(token.clone());

        if let Some(account_id) = auth
            .get_account_id()
            .or_else(|| extract_chatgpt_account_id(&token))
        {
            client = client.with_chatgpt_account_id(account_id.clone());
            backend_client = backend_client.with_chatgpt_account_id(account_id);
        }

        Ok(Self {
            backend: Arc::new(client),
            http_extras: Some(HttpExtras {
                backend: backend_client,
            }),
            is_mock: false,
        })
    }

    pub fn is_mock(&self) -> bool {
        self.is_mock
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskSummary>> {
        self.backend.list_tasks(None).await.map_err(map_cloud_error)
    }

    pub async fn get_task_text(&self, task_id: &str) -> Result<TaskText> {
        let task_id = TaskId(task_id.to_string());
        self.backend
            .get_task_text(task_id)
            .await
            .map_err(map_cloud_error)
    }

    pub async fn get_task_diff(&self, task_id: &str) -> Result<Option<String>> {
        let task_id = TaskId(task_id.to_string());
        self.backend
            .get_task_diff(task_id)
            .await
            .map_err(map_cloud_error)
    }

    pub async fn list_sibling_attempts(
        &self,
        task_id: &str,
        turn_id: &str,
    ) -> Result<Vec<TurnAttempt>> {
        let task_id = TaskId(task_id.to_string());
        self.backend
            .list_sibling_attempts(task_id, turn_id.to_string())
            .await
            .map_err(map_cloud_error)
    }

    async fn fetch_task_list_item(&self, task_id: &str) -> Result<Option<TaskListItem>> {
        let Some(extras) = &self.http_extras else {
            return Ok(None);
        };

        let (_parsed, body, _ct) = extras
            .backend
            .get_task_details_with_body(task_id)
            .await
            .context("Failed to fetch task details")?;
        let value: serde_json::Value =
            serde_json::from_str(&body).context("Failed to parse task details JSON")?;
        if let Some(task) = value.get("task") {
            let item: TaskListItem = serde_json::from_value(task.clone())
                .context("Failed to deserialize task metadata")?;
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    pub async fn find_task_summary(&self, task_id: &str) -> Result<Option<TaskSummary>> {
        let tasks = self.list_tasks().await?;
        Ok(tasks.into_iter().find(|t| t.id.0 == task_id))
    }

    pub async fn load_task_data(&self, task_id: &str) -> Result<TaskData> {
        let summary = self.find_task_summary(task_id).await?;
        let list_item = self.fetch_task_list_item(task_id).await?;
        let text = self
            .get_task_text(task_id)
            .await
            .context("Failed to fetch task conversation")?;
        let base_diff = self
            .get_task_diff(task_id)
            .await
            .context("Failed to fetch task diff")?;

        let mut variants = Vec::new();
        variants.push(VariantData {
            index: 1,
            is_base: true,
            attempt_placement: text.attempt_placement,
            status: text.attempt_status,
            diff: base_diff,
            messages: text.messages.clone(),
            prompt: text.prompt.clone(),
            turn_id: text.turn_id.clone(),
        });

        let mut siblings: Vec<VariantData> = Vec::new();
        if let Some(turn_id) = text.turn_id.clone() {
            let attempts = self
                .list_sibling_attempts(task_id, &turn_id)
                .await
                .context("Failed to load sibling attempts")?;
            let seen_turn: Option<String> = text.turn_id.clone();
            for attempt in attempts {
                if seen_turn
                    .as_ref()
                    .map(|base| base == &attempt.turn_id)
                    .unwrap_or(false)
                {
                    continue;
                }
                siblings.push(VariantData {
                    index: 0,
                    is_base: false,
                    attempt_placement: attempt.attempt_placement,
                    status: attempt.status,
                    diff: attempt.diff,
                    messages: attempt.messages,
                    prompt: None,
                    turn_id: Some(attempt.turn_id),
                });
            }
        }

        siblings.sort_by(|a, b| {
            crate::cloud::helpers::compare_attempts(
                a.attempt_placement,
                b.attempt_placement,
                a.turn_id.as_deref(),
                b.turn_id.as_deref(),
            )
        });
        for (i, variant) in siblings.into_iter().enumerate() {
            let mut variant = variant;
            variant.index = i + 2;
            variants.push(variant);
        }

        let attempt_hint = summary
            .as_ref()
            .and_then(|s| s.attempt_total)
            .or_else(|| Some(text.sibling_turn_ids.len().saturating_add(1)))
            .filter(|v| *v != 0);

        Ok(TaskData {
            task_id: task_id.to_string(),
            summary,
            list_item,
            variants,
            attempt_total_hint: attempt_hint,
        })
    }

    pub async fn create_task(&self, request: CreateTaskReq) -> Result<TaskId> {
        let created = self
            .backend
            .create_task(request)
            .await
            .map_err(map_cloud_error)?;
        Ok(created.id)
    }
}

fn map_cloud_error(err: CloudTaskError) -> anyhow::Error {
    anyhow!(err)
}
