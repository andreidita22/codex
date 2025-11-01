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
#[cfg(windows)]
use windows_sys::Win32::System::Console::CTRL_BREAK_EVENT;
#[cfg(windows)]
use windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent;
#[cfg(windows)]
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

use crate::exec::StdoutStream;
use crate::protocol::BackgroundEventEvent;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::ExecCommandOutputDeltaEvent;
use crate::protocol::ExecOutputStream;
use crate::protocol::SandboxPolicy;
use crate::sandboxing::ExecEnv;
use crate::spawn::StdioPolicy;
use crate::spawn::spawn_child_async;

const STDOUT_CAPTURE_LIMIT_BYTES: usize = 512 * 1024;
const STDERR_CAPTURE_LIMIT_BYTES: usize = 512 * 1024;
const AGGREGATED_CAPTURE_LIMIT_BYTES: usize = 1024 * 1024;
const AUTO_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(500);

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
    stdout_buf: Arc<Mutex<CaptureBuffer>>,
    stderr_buf: Arc<Mutex<CaptureBuffer>>,
    aggregated_buf: Arc<Mutex<CaptureBuffer>>,
    stdout_stream: Option<StdoutStream>,
    completed_status: Arc<Mutex<Option<std::process::ExitStatus>>>,
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
    stdout_buf: Arc<Mutex<CaptureBuffer>>,
    stderr_buf: Arc<Mutex<CaptureBuffer>>,
    aggregated_buf: Arc<Mutex<CaptureBuffer>>,
    stdout_stream: Option<StdoutStream>,
    completed_status: Arc<Mutex<Option<std::process::ExitStatus>>>,
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
        let pid = child
            .id()
            .ok_or_else(|| anyhow!("spawned process has no PID"))?;

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
        let stdout_buf = Arc::new(Mutex::new(CaptureBuffer::new(STDOUT_CAPTURE_LIMIT_BYTES)));
        let stderr_buf = Arc::new(Mutex::new(CaptureBuffer::new(STDERR_CAPTURE_LIMIT_BYTES)));
        let aggregated_buf = Arc::new(Mutex::new(CaptureBuffer::new(
            AGGREGATED_CAPTURE_LIMIT_BYTES,
        )));
        let stdout_stream = req.stdout_stream.clone();
        let completed_status = Arc::new(Mutex::new(None));
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
                                stdout_stream: stdout_stream.clone(),
                                completed_status: completed_status.clone(),
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

        if let Some(status) = entry.completed_status().await {
            let result = entry.completed_result(status).await;
            self.finish_run(run_id).await;
            return result;
        }

        let completion = wait_for_exit(entry.child.clone());
        tokio::pin!(completion);
        let pause_rx = entry.pause_trigger.arm().await;
        tokio::pin!(pause_rx);
        tokio::select! {
            status = &mut completion => {
                let status = status?;
                entry.mark_completed(status).await;
                let result = entry.completed_result(status).await;
                self.finish_run(run_id).await;
                result
            }
            reason = &mut pause_rx => {
                match reason {
                    Ok(reason) => {
                        let (out_tail, err_tail) = entry.snapshot_tails().await;
                        let idle_ms = entry.last_output.lock().await.elapsed().as_millis() as u64;
                        let pid = child_pid(&entry.child).await?;
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
                        entry.mark_completed(status).await;
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
            match child_pid(&entry.child).await {
                Ok(pid) => {
                    if let Err(err) = send_ctrl_break(pid) {
                        tracing::warn!(
                            pid,
                            error = %err,
                            "failed to send CTRL_BREAK_EVENT; falling back to force kill if needed"
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "semantic shell interrupt requested for exited process: {err:#}"
                    );
                }
            }
        }

        let wait = tokio::time::timeout(
            Duration::from_millis(graceful_ms),
            wait_for_exit(entry.child.clone()),
        )
        .await;
        match wait {
            Ok(status) => {
                let status = status?;
                entry.mark_completed(status).await;
            }
            Err(_) => {
                entry.child.lock().await.start_kill()?;
                let status = entry.child.lock().await.wait().await?;
                entry.mark_completed(status).await;
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
        let status = entry.child.lock().await.wait().await?;
        entry.mark_completed(status).await;
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
        let pid = child_pid(&entry.child).await?;
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
            let pid = child_pid(&entry.child).await?;
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
            stdout_stream: registration.stdout_stream,
            completed_status: registration.completed_status,
            stdout_task: registration.stdout_task,
            stderr_task: registration.stderr_task,
            idle_task: registration.idle_task,
        });
        self.inner
            .runs
            .lock()
            .await
            .insert(run_id.clone(), Arc::clone(&entry));
        self.spawn_exit_monitor(run_id.clone(), Arc::clone(&self.inner), entry);
        run_id
    }

    fn spawn_exit_monitor(&self, run_id: String, inner: Arc<Inner>, entry: Arc<ProcessEntry>) {
        tokio::spawn(async move {
            loop {
                if entry.completed_status().await.is_some() {
                    break;
                }
                tokio::time::sleep(AUTO_EXIT_POLL_INTERVAL).await;
                let status_opt = {
                    let mut child = entry.child.lock().await;
                    match child.try_wait() {
                        Ok(opt) => opt,
                        Err(err) => {
                            tracing::debug!(
                                error = %err,
                                run_id,
                                "semantic shell try_wait failed while monitoring exit"
                            );
                            None
                        }
                    }
                };
                if let Some(status) = status_opt {
                    if entry.mark_completed(status).await {
                        let still_registered = {
                            let runs = inner.runs.lock().await;
                            runs.contains_key(&run_id)
                        };
                        if still_registered {
                            entry.emit_completion_event(&run_id, status).await;
                        }
                    }
                    break;
                }
            }
        });
    }

    fn spawn_stdout_reader(
        stdout: ChildStdout,
        last_output: Arc<Mutex<Instant>>,
        tails: Arc<Mutex<Tails>>,
        pause_trigger: PauseTrigger,
        policy_state: PausePolicyState,
        stdout_buf: Arc<Mutex<CaptureBuffer>>,
        aggregated_buf: Arc<Mutex<CaptureBuffer>>,
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
        stderr_buf: Arc<Mutex<CaptureBuffer>>,
        aggregated_buf: Arc<Mutex<CaptureBuffer>>,
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

    async fn append_line(buf: &Arc<Mutex<CaptureBuffer>>, line: &str) {
        buf.lock().await.append_line(line);
    }
}

struct CaptureBuffer {
    data: Vec<u8>,
    truncated: bool,
    cap: usize,
}

impl CaptureBuffer {
    fn new(cap: usize) -> Self {
        Self {
            data: Vec::new(),
            truncated: false,
            cap,
        }
    }

    fn append_line(&mut self, line: &str) {
        self.data.extend_from_slice(line.as_bytes());
        self.data.push(b'\n');
        if self.data.len() > self.cap {
            let overflow = self.data.len().saturating_sub(self.cap);
            self.data.drain(0..overflow);
            self.truncated = true;
        }
    }

    fn snapshot(&self) -> String {
        let text = String::from_utf8_lossy(&self.data).to_string();
        if self.truncated {
            format!(
                "[semantic shell captured last {} bytes]\n{}",
                self.cap, text
            )
        } else {
            text
        }
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

    async fn mark_completed(&self, status: std::process::ExitStatus) -> bool {
        let mut guard = self.completed_status.lock().await;
        if guard.is_none() {
            *guard = Some(status);
            true
        } else {
            false
        }
    }

    async fn completed_status(&self) -> Option<std::process::ExitStatus> {
        *self.completed_status.lock().await
    }

    async fn emit_completion_event(&self, run_id: &str, status: std::process::ExitStatus) {
        let Some(stream) = &self.stdout_stream else {
            return;
        };
        let exit_code = status.code().unwrap_or(-1);
        let elapsed_secs = self.started_at.elapsed().as_secs();
        let message = format!(
            "Semantic shell run {run_id} exited with code {exit_code} after {elapsed_secs}s. Run `codex shell resume {run_id}` to collect final output."
        );
        let event = Event {
            id: stream.sub_id.clone(),
            msg: EventMsg::BackgroundEvent(BackgroundEventEvent { message }),
        };
        let _ = stream.tx_event.send(event).await;
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

async fn buffer_to_string(buf: &Arc<Mutex<CaptureBuffer>>) -> String {
    buf.lock().await.snapshot()
}

async fn completed_from_buffers(
    status: std::process::ExitStatus,
    started_at: Instant,
    stdout_buf: &Arc<Mutex<CaptureBuffer>>,
    stderr_buf: &Arc<Mutex<CaptureBuffer>>,
    aggregated_buf: &Arc<Mutex<CaptureBuffer>>,
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

async fn child_pid(child: &Arc<Mutex<Child>>) -> Result<u32> {
    child
        .lock()
        .await
        .id()
        .map(|pid| pid as u32)
        .ok_or_else(|| anyhow!("child process has already exited"))
}

#[cfg(windows)]
fn send_ctrl_break(pid: u32) -> Result<()> {
    unsafe {
        if SetConsoleCtrlHandler(None, 1) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let result = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid);
        let signal_err = std::io::Error::last_os_error();
        let _ = SetConsoleCtrlHandler(None, 0);
        if result == 0 {
            return Err(signal_err.into());
        }
    }
    Ok(())
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
