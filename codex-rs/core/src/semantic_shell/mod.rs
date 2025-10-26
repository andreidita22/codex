use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::Utc;
use regex_lite::Regex;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStderr;
use tokio::process::ChildStdout;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::exec::StdoutStream;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::ExecCommandOutputDeltaEvent;
use crate::protocol::ExecOutputStream;
use crate::protocol::SandboxPolicy;
use crate::sandboxing::ExecEnv;
use crate::spawn::StdioPolicy;
use crate::spawn::spawn_child_async;

/// Result of a shell turn when semantic pause is enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellTurnResult {
    Completed {
        exit: i32,
        duration_ms: u64,
        stdout: String,
        stderr: String,
        aggregated: String,
    },
    Paused {
        run_id: String,
        reason: PauseReason,
        pid: u32,
        idle_ms: u64,
        last_stdout: Vec<String>,
        last_stderr: Vec<String>,
        started_at: String,
    },
    Failed {
        error: String,
    },
}

/// Why the shell paused instead of completing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PauseReason {
    Idle { silent_ms: u64, threshold_ms: u64 },
    Ready { pattern: String },
    Prompt { pattern: String },
}

/// Request parameters for semantic shell execution.
#[derive(Clone)]
pub struct ShellExecRequest {
    pub exec_env: ExecEnv,
    pub sandbox_policy: SandboxPolicy,
    pub stdout_stream: Option<StdoutStream>,
    pub pause_on_idle_ms: Option<u64>,
    pub pause_on_ready_pattern: Option<String>,
    pub pause_on_prompt_pattern: Option<String>,
    pub tail_lines: usize,
}

impl From<&ShellExecRequest> for PausePolicy {
    fn from(req: &ShellExecRequest) -> Self {
        Self {
            pause_on_idle_ms: req.pause_on_idle_ms,
            pause_on_ready_pattern: req.pause_on_ready_pattern.clone(),
            pause_on_prompt_pattern: req.pause_on_prompt_pattern.clone(),
        }
    }
}

/// Placeholder policy updates for resume operations.
#[derive(Debug, Clone, Default)]
pub struct PausePolicy {
    pub pause_on_idle_ms: Option<u64>,
    pub pause_on_ready_pattern: Option<String>,
    pub pause_on_prompt_pattern: Option<String>,
}

/// Summary of a running semantic shell process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecStatus {
    pub pid: u32,
    pub uptime_ms: u64,
    pub last_output_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunListing {
    pub run_id: String,
    pub pid: u32,
    pub uptime_ms: u64,
    pub last_output_ms: u64,
    pub started_at: String,
}

/// Manages long-running shell processes that can be paused/resumed.
#[derive(Default, Clone)]
pub struct SemanticShellManager {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    runs: Mutex<HashMap<String, Arc<ProcessEntry>>>,
}

struct ProcessEntry {
    child: Arc<Mutex<Child>>,
    started_at: Instant,
    started_at_wall: DateTime<Utc>,
    last_output: Arc<Mutex<Instant>>,
    tails: Arc<Mutex<Tails>>,
    policy: Arc<RwLock<PausePolicy>>,
    policy_state: PausePolicyState,
    pause_trigger: PauseTrigger,
    stdout_buf: Arc<Mutex<Vec<u8>>>,
    stderr_buf: Arc<Mutex<Vec<u8>>>,
    aggregated_buf: Arc<Mutex<Vec<u8>>>,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    idle_task: JoinHandle<()>,
}

struct ProcessRegistration {
    child: Arc<Mutex<Child>>,
    started_at: Instant,
    started_at_wall: DateTime<Utc>,
    last_output: Arc<Mutex<Instant>>,
    tails: Arc<Mutex<Tails>>,
    policy: Arc<RwLock<PausePolicy>>,
    policy_state: PausePolicyState,
    pause_trigger: PauseTrigger,
    stdout_buf: Arc<Mutex<Vec<u8>>>,
    stderr_buf: Arc<Mutex<Vec<u8>>>,
    aggregated_buf: Arc<Mutex<Vec<u8>>>,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    idle_task: JoinHandle<()>,
}

#[derive(Debug)]
struct Tails {
    stdout: VecDeque<String>,
    stderr: VecDeque<String>,
    cap: usize,
}

impl Tails {
    fn new(cap: usize) -> Self {
        Self {
            stdout: VecDeque::new(),
            stderr: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    fn push_stdout(&mut self, line: String) {
        if self.stdout.len() == self.cap {
            self.stdout.pop_front();
        }
        self.stdout.push_back(line);
    }

    fn push_stderr(&mut self, line: String) {
        if self.stderr.len() == self.cap {
            self.stderr.pop_front();
        }
        self.stderr.push_back(line);
    }

    fn snapshot(&self) -> (Vec<String>, Vec<String>) {
        (
            self.stdout.iter().cloned().collect(),
            self.stderr.iter().cloned().collect(),
        )
    }
}

#[derive(Clone)]
struct CompiledPattern {
    regex: Regex,
    pattern: String,
}

#[derive(Clone)]
struct PausePolicyState {
    idle_ms: Arc<RwLock<Option<u64>>>,
    ready: Arc<RwLock<Option<CompiledPattern>>>,
    prompt: Arc<RwLock<Option<CompiledPattern>>>,
}

impl PausePolicyState {
    fn new(policy: &PausePolicy) -> Result<Self> {
        Ok(Self {
            idle_ms: Arc::new(RwLock::new(policy.pause_on_idle_ms)),
            ready: Arc::new(RwLock::new(compile_pattern_owned(
                policy.pause_on_ready_pattern.clone(),
            )?)),
            prompt: Arc::new(RwLock::new(compile_pattern_owned(
                policy.pause_on_prompt_pattern.clone(),
            )?)),
        })
    }

    async fn update(&self, policy: &PausePolicy) -> Result<()> {
        {
            let mut idle = self.idle_ms.write().await;
            *idle = policy.pause_on_idle_ms;
        }
        {
            let mut ready = self.ready.write().await;
            *ready = compile_pattern_owned(policy.pause_on_ready_pattern.clone())?;
        }
        {
            let mut prompt = self.prompt.write().await;
            *prompt = compile_pattern_owned(policy.pause_on_prompt_pattern.clone())?;
        }
        Ok(())
    }

    async fn idle_threshold(&self) -> Option<u64> {
        *self.idle_ms.read().await
    }

    async fn ready_pattern(&self) -> Option<CompiledPattern> {
        self.ready.read().await.clone()
    }

    async fn prompt_pattern(&self) -> Option<CompiledPattern> {
        self.prompt.read().await.clone()
    }

    async fn match_line(&self, line: &str) -> Option<PauseReason> {
        if let Some(pattern) = self.ready_pattern().await {
            if pattern.regex.is_match(line) {
                return Some(PauseReason::Ready {
                    pattern: pattern.pattern.clone(),
                });
            }
        }
        if let Some(pattern) = self.prompt_pattern().await {
            if pattern.regex.is_match(line) {
                return Some(PauseReason::Prompt {
                    pattern: pattern.pattern.clone(),
                });
            }
        }
        None
    }
}

#[derive(Clone)]
struct PauseTrigger {
    sender: Arc<Mutex<Option<oneshot::Sender<PauseReason>>>>,
}

impl PauseTrigger {
    fn new() -> (Self, oneshot::Receiver<PauseReason>) {
        let (tx, rx) = oneshot::channel();
        (
            Self {
                sender: Arc::new(Mutex::new(Some(tx))),
            },
            rx,
        )
    }

    async fn arm(&self) -> oneshot::Receiver<PauseReason> {
        let (tx, rx) = oneshot::channel();
        let mut sender = self.sender.lock().await;
        *sender = Some(tx);
        rx
    }

    async fn trigger(&self, reason: PauseReason) {
        if let Some(tx) = self.sender.lock().await.take() {
            let _ = tx.send(reason);
        }
    }
}

impl SemanticShellManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn exec_with_semantic_pause(&self, req: ShellExecRequest) -> Result<ShellTurnResult> {
        let mut child = spawn_from_exec_env(&req.exec_env, &req.sandbox_policy)
            .await
            .with_context(|| "failed to spawn semantic shell child".to_string())?;
        let pid = child.id().unwrap_or(0);

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("child missing stdout pipe"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("child missing stderr pipe"))?;

        let child = Arc::new(Mutex::new(child));
        let last_output = Arc::new(Mutex::new(Instant::now()));
        let tails = Arc::new(Mutex::new(Tails::new(req.tail_lines)));
        let stdout_buf = Arc::new(Mutex::new(Vec::new()));
        let stderr_buf = Arc::new(Mutex::new(Vec::new()));
        let aggregated_buf = Arc::new(Mutex::new(Vec::new()));
        let stdout_stream = req.stdout_stream.clone();
        let (pause_trigger, pause_rx) = PauseTrigger::new();

        let policy_value = PausePolicy::from(&req);
        let policy_state = PausePolicyState::new(&policy_value)?;
        let policy = Arc::new(RwLock::new(policy_value));

        let stdout_task = Self::spawn_stdout_reader(
            stdout,
            last_output.clone(),
            tails.clone(),
            pause_trigger.clone(),
            policy_state.clone(),
            stdout_buf.clone(),
            aggregated_buf.clone(),
            stdout_stream.clone(),
        );
        let stderr_task = Self::spawn_stderr_reader(
            stderr,
            tails.clone(),
            stderr_buf.clone(),
            aggregated_buf.clone(),
            stdout_stream.clone(),
        );
        let idle_task = Self::spawn_idle_monitor(
            last_output.clone(),
            pause_trigger.clone(),
            policy_state.clone(),
        );

        let started_at = Instant::now();
        let started_at_wall = Utc::now();
        let child_for_wait = child.clone();
        let completion = async move {
            let mut child_guard = child_for_wait.lock().await;
            child_guard.wait().await
        };
        tokio::pin!(completion);

        tokio::pin!(pause_rx);

        tokio::select! {
            status = &mut completion => {
                stdout_task.abort();
                stderr_task.abort();
                idle_task.abort();
                let status = status?;
                return completed_from_buffers(
                    status,
                    started_at,
                    &stdout_buf,
                    &stderr_buf,
                    &aggregated_buf,
                )
                .await;
            }
            reason = &mut pause_rx => {
                match reason {
                    Ok(reason) => {
                        let run_id = self
                            .register_process(ProcessRegistration {
                                child: child.clone(),
                                started_at,
                                started_at_wall,
                                last_output: last_output.clone(),
                                tails: tails.clone(),
                                policy: policy.clone(),
                                policy_state: policy_state.clone(),
                                pause_trigger: pause_trigger.clone(),
                                stdout_buf: stdout_buf.clone(),
                                stderr_buf: stderr_buf.clone(),
                                aggregated_buf: aggregated_buf.clone(),
                                stdout_task,
                                stderr_task,
                                idle_task,
                            })
                            .await;
                        let (out_tail, err_tail) = tails.lock().await.snapshot();
                        let idle_ms = last_output.lock().await.elapsed().as_millis() as u64;
                        Ok(ShellTurnResult::Paused {
                            run_id,
                            reason,
                            pid,
                            idle_ms,
                            last_stdout: out_tail,
                            last_stderr: err_tail,
                            started_at: started_at_wall.to_rfc3339(),
                        })
                    }
                    Err(_) => {
                        stdout_task.abort();
                        stderr_task.abort();
                        idle_task.abort();
                        let mut child_guard = child.lock().await;
                        let status = child_guard.wait().await?;
                        return completed_from_buffers(
                            status,
                            started_at,
                            &stdout_buf,
                            &stderr_buf,
                            &aggregated_buf,
                        )
                        .await;
                    }
                }
            }
        }
    }

    pub async fn resume(
        &self,
        run_id: &str,
        policy: Option<PausePolicy>,
    ) -> Result<ShellTurnResult> {
        let entry = self.get_entry(run_id).await?;
        if let Some(new_policy) = policy {
            entry.policy_state.update(&new_policy).await?;
            let mut stored = entry.policy.write().await;
            *stored = new_policy;
        }

        let completion = wait_for_exit(entry.child.clone());
        tokio::pin!(completion);
        let pause_rx = entry.pause_trigger.arm().await;
        tokio::pin!(pause_rx);
        tokio::select! {
            status = &mut completion => {
                let status = status?;
                let result = entry.completed_result(status).await;
                self.finish_run(run_id).await;
                result
            }
            reason = &mut pause_rx => {
                match reason {
                    Ok(reason) => {
                        let (out_tail, err_tail) = entry.snapshot_tails().await;
                        let idle_ms = entry.last_output.lock().await.elapsed().as_millis() as u64;
                        let pid = entry.child.lock().await.id().unwrap_or(0) as u32;
                        Ok(ShellTurnResult::Paused {
                            run_id: run_id.to_string(),
                            reason,
                            pid,
                            idle_ms,
                            last_stdout: out_tail,
                            last_stderr: err_tail,
                            started_at: entry.started_at_wall.to_rfc3339(),
                        })
                    }
                    Err(_) => {
                        let status = completion.await?;
                        let result = entry.completed_result(status).await;
                        self.finish_run(run_id).await;
                        result
                    }
                }
            }
        }
    }

    pub async fn interrupt(&self, run_id: &str, graceful_ms: u64) -> Result<()> {
        let entry = self.get_entry(run_id).await?;

        #[cfg(unix)]
        {
            if let Some(pid) = entry.child.lock().await.id() {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGINT);
                }
            }
        }

        #[cfg(windows)]
        {
            let _ = entry.child.lock().await.start_kill();
        }

        let wait = tokio::time::timeout(
            Duration::from_millis(graceful_ms),
            wait_for_exit(entry.child.clone()),
        )
        .await;
        match wait {
            Ok(status) => {
                let _ = status?;
            }
            Err(_) => {
                entry.child.lock().await.start_kill()?;
                let _ = entry.child.lock().await.wait().await?;
            }
        }
        self.finish_run(run_id).await;
        Ok(())
    }

    pub async fn kill(&self, run_id: &str) -> Result<()> {
        let entry = self
            .inner
            .runs
            .lock()
            .await
            .remove(run_id)
            .ok_or_else(|| anyhow!("unknown run_id"))?;
        let _ = entry.child.lock().await.start_kill();
        let _ = entry.child.lock().await.wait().await?;
        entry.abort_tasks();
        Ok(())
    }

    pub async fn status(&self, run_id: &str) -> Result<ExecStatus> {
        let entry = self
            .inner
            .runs
            .lock()
            .await
            .get(run_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown run_id"))?;
        let pid = entry.child.lock().await.id().unwrap_or(0) as u32;
        let uptime_ms = entry.started_at.elapsed().as_millis() as u64;
        let last_output_ms = entry.last_output.lock().await.elapsed().as_millis() as u64;
        Ok(ExecStatus {
            pid,
            uptime_ms,
            last_output_ms,
        })
    }

    pub async fn list(&self) -> Result<Vec<RunListing>> {
        let runs = self.inner.runs.lock().await;
        let snapshots: Vec<(String, Arc<ProcessEntry>)> = runs
            .iter()
            .map(|(run_id, entry)| (run_id.clone(), Arc::clone(entry)))
            .collect();
        drop(runs);

        let mut listings = Vec::with_capacity(snapshots.len());
        for (run_id, entry) in snapshots {
            let pid = entry.child.lock().await.id().unwrap_or(0) as u32;
            let uptime_ms = entry.started_at.elapsed().as_millis() as u64;
            let last_output_ms = entry.last_output.lock().await.elapsed().as_millis() as u64;
            listings.push(RunListing {
                run_id,
                pid,
                uptime_ms,
                last_output_ms,
                started_at: entry.started_at_wall.to_rfc3339(),
            });
        }
        Ok(listings)
    }

    async fn get_entry(&self, run_id: &str) -> Result<Arc<ProcessEntry>> {
        self.inner
            .runs
            .lock()
            .await
            .get(run_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown run_id"))
    }

    async fn finish_run(&self, run_id: &str) {
        if let Some(entry) = self.inner.runs.lock().await.remove(run_id) {
            entry.abort_tasks();
        }
    }

    async fn register_process(&self, registration: ProcessRegistration) -> String {
        let run_id = format!("r-{}", Uuid::new_v4().simple());
        let entry = Arc::new(ProcessEntry {
            child: registration.child,
            started_at: registration.started_at,
            started_at_wall: registration.started_at_wall,
            last_output: registration.last_output,
            tails: registration.tails,
            policy: registration.policy,
            policy_state: registration.policy_state,
            pause_trigger: registration.pause_trigger,
            stdout_buf: registration.stdout_buf,
            stderr_buf: registration.stderr_buf,
            aggregated_buf: registration.aggregated_buf,
            stdout_task: registration.stdout_task,
            stderr_task: registration.stderr_task,
            idle_task: registration.idle_task,
        });
        self.inner.runs.lock().await.insert(run_id.clone(), entry);
        run_id
    }

    fn spawn_stdout_reader(
        stdout: ChildStdout,
        last_output: Arc<Mutex<Instant>>,
        tails: Arc<Mutex<Tails>>,
        pause_trigger: PauseTrigger,
        policy_state: PausePolicyState,
        stdout_buf: Arc<Mutex<Vec<u8>>>,
        aggregated_buf: Arc<Mutex<Vec<u8>>>,
        stdout_stream: Option<StdoutStream>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                *last_output.lock().await = Instant::now();
                tails.lock().await.push_stdout(line.clone());
                Self::append_line(&stdout_buf, &line).await;
                Self::append_line(&aggregated_buf, &line).await;
                if let Some(stream) = &stdout_stream {
                    let mut chunk = line.clone();
                    chunk.push('\n');
                    send_stream_chunk(stream, chunk.as_bytes(), false).await;
                }
                if let Some(reason) = policy_state.match_line(&line).await {
                    pause_trigger.trigger(reason).await;
                }
            }
        })
    }

    fn spawn_stderr_reader(
        stderr: ChildStderr,
        tails: Arc<Mutex<Tails>>,
        stderr_buf: Arc<Mutex<Vec<u8>>>,
        aggregated_buf: Arc<Mutex<Vec<u8>>>,
        stdout_stream: Option<StdoutStream>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tails.lock().await.push_stderr(line.clone());
                Self::append_line(&stderr_buf, &line).await;
                Self::append_line(&aggregated_buf, &line).await;
                if let Some(stream) = &stdout_stream {
                    let mut chunk = line.clone();
                    chunk.push('\n');
                    send_stream_chunk(stream, chunk.as_bytes(), true).await;
                }
            }
        })
    }

    fn spawn_idle_monitor(
        last_output: Arc<Mutex<Instant>>,
        pause_trigger: PauseTrigger,
        policy_state: PausePolicyState,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut last_reported: Option<Instant> = None;
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let Some(ms) = policy_state.idle_threshold().await else {
                    last_reported = None;
                    continue;
                };
                let threshold = Duration::from_millis(ms);
                let observed = *last_output.lock().await;
                if let Some(prev) = last_reported {
                    if observed > prev {
                        last_reported = None;
                    }
                }
                let elapsed = observed.elapsed();
                if elapsed >= threshold && last_reported.is_none() {
                    pause_trigger
                        .trigger(PauseReason::Idle {
                            silent_ms: elapsed.as_millis() as u64,
                            threshold_ms: ms,
                        })
                        .await;
                    last_reported = Some(observed);
                }
            }
        })
    }

    async fn append_line(buf: &Arc<Mutex<Vec<u8>>>, line: &str) {
        let mut guard = buf.lock().await;
        guard.extend_from_slice(line.as_bytes());
        guard.push(b'\n');
    }
}

impl ProcessEntry {
    fn abort_tasks(&self) {
        self.stdout_task.abort();
        self.stderr_task.abort();
        self.idle_task.abort();
    }

    async fn snapshot_tails(&self) -> (Vec<String>, Vec<String>) {
        self.tails.lock().await.snapshot()
    }

    async fn completed_result(&self, status: std::process::ExitStatus) -> Result<ShellTurnResult> {
        let (stdout_text, stderr_text, aggregated_text) = self.collected_output().await;
        Ok(ShellTurnResult::Completed {
            exit: status.code().unwrap_or(-1),
            duration_ms: self.started_at.elapsed().as_millis() as u64,
            stdout: stdout_text,
            stderr: stderr_text,
            aggregated: aggregated_text,
        })
    }

    async fn collected_output(&self) -> (String, String, String) {
        let stdout = buffer_to_string(&self.stdout_buf).await;
        let stderr = buffer_to_string(&self.stderr_buf).await;
        let aggregated = buffer_to_string(&self.aggregated_buf).await;
        (stdout, stderr, aggregated)
    }
}

fn compile_pattern_owned(pattern: Option<String>) -> Result<Option<CompiledPattern>> {
    match pattern {
        Some(pat) => {
            let regex = Regex::new(&pat).map_err(|err| anyhow!("invalid regex '{pat}': {err}"))?;
            Ok(Some(CompiledPattern {
                regex,
                pattern: pat,
            }))
        }
        None => Ok(None),
    }
}

async fn wait_for_exit(child: Arc<Mutex<Child>>) -> Result<std::process::ExitStatus> {
    let mut guard = child.lock().await;
    guard
        .wait()
        .await
        .map_err(|err| anyhow!("failed to wait for child process: {err}"))
}

async fn spawn_from_exec_env(env: &ExecEnv, policy: &SandboxPolicy) -> Result<Child> {
    let (program, args) = env
        .command
        .split_first()
        .ok_or_else(|| anyhow!("exec env missing command"))?;
    spawn_child_async(
        PathBuf::from(program),
        args.to_vec(),
        env.arg0.as_deref(),
        env.cwd.clone(),
        policy,
        StdioPolicy::RedirectForShellTool,
        env.env.clone(),
    )
    .await
    .map_err(|err| anyhow!("failed to spawn child process: {err}"))
}

async fn buffer_to_string(buf: &Arc<Mutex<Vec<u8>>>) -> String {
    let data = buf.lock().await.clone();
    String::from_utf8_lossy(&data).to_string()
}

async fn completed_from_buffers(
    status: std::process::ExitStatus,
    started_at: Instant,
    stdout_buf: &Arc<Mutex<Vec<u8>>>,
    stderr_buf: &Arc<Mutex<Vec<u8>>>,
    aggregated_buf: &Arc<Mutex<Vec<u8>>>,
) -> Result<ShellTurnResult> {
    let stdout_text = buffer_to_string(stdout_buf).await;
    let stderr_text = buffer_to_string(stderr_buf).await;
    let aggregated_text = buffer_to_string(aggregated_buf).await;
    Ok(build_completed(
        status.code().unwrap_or(-1),
        started_at.elapsed(),
        stdout_text,
        stderr_text,
        aggregated_text,
    ))
}

fn build_completed(
    exit: i32,
    duration: Duration,
    stdout: String,
    stderr: String,
    aggregated: String,
) -> ShellTurnResult {
    ShellTurnResult::Completed {
        exit,
        duration_ms: duration.as_millis() as u64,
        stdout,
        stderr,
        aggregated,
    }
}

async fn send_stream_chunk(stream: &StdoutStream, chunk: &[u8], is_stderr: bool) {
    let event = Event {
        id: stream.sub_id.clone(),
        msg: EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
            call_id: stream.call_id.clone(),
            stream: if is_stderr {
                ExecOutputStream::Stderr
            } else {
                ExecOutputStream::Stdout
            },
            chunk: chunk.to_vec(),
        }),
    };
    let _ = stream.tx_event.send(event).await;
}

pub fn format_pause_reason(reason: &PauseReason) -> String {
    match reason {
        PauseReason::Idle {
            silent_ms,
            threshold_ms,
        } => format!("idle for {}ms (threshold {}ms)", silent_ms, threshold_ms),
        PauseReason::Ready { pattern } => {
            format!("ready pattern matched: `{pattern}`")
        }
        PauseReason::Prompt { pattern } => {
            format!("prompt detected: `{pattern}`")
        }
    }
}

pub fn format_recent_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        return "<no output yet>".to_string();
    }
    lines.join("\n")
}
