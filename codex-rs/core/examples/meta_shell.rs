use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

use async_channel::unbounded;
use codex_core::exec::MetaShellConfig;
use codex_core::exec::SandboxType;
use codex_core::exec::ShellMeta;
use codex_core::exec::StdoutStream;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::SandboxPolicy;
use codex_core::sandboxing::ExecEnv;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let script = env::args()
        .nth(1)
        .unwrap_or_else(|| "scripts/mock_long_running.sh".into());
    let command = vec!["/bin/bash".into(), script];

    let (tx, rx) = unbounded::<Event>();
    let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    let env = ExecEnv {
        command: command.clone(),
        cwd: cwd.clone(),
        env: HashMap::new(),
        timeout_ms: Some(90_000),
        sandbox: SandboxType::None,
        with_escalated_permissions: None,
        justification: None,
        arg0: None,
    };

    let stream = StdoutStream {
        sub_id: "demo_sub".into(),
        call_id: "demo_call".into(),
        tx_event: tx,
        meta: Some(ShellMeta {
            meta_config: Some(MetaShellConfig {
                silence_threshold: Duration::from_secs(30),
                heartbeat_interval: Duration::from_secs(5),
                command_label: command.join(" "),
                cwd_display: cwd.display().to_string(),
                reporter: None,
                interrupt_on_silence: true,
            }),
        }),
    };

    let rx_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event.msg {
                EventMsg::BackgroundEvent(ev) => println!("[background] {}", ev.message),
                EventMsg::ExecCommandOutputDelta(delta) => {
                    let text = String::from_utf8_lossy(&delta.chunk);
                    println!(
                        "[delta {:?}] {} bytes | {}",
                        delta.stream,
                        delta.chunk.len(),
                        text.trim_end()
                    )
                }
                EventMsg::ExecCommandEnd(ev) => println!(
                    "[exec end] exit={} duration={:?}",
                    ev.exit_code, ev.duration
                ),
                other => println!("[event] {:?}", other),
            }
        }
    });

    let output =
        codex_core::sandboxing::execute_env(&env, &SandboxPolicy::DangerFullAccess, Some(stream))
            .await?;
    println!(
        "Command finished: exit={} timed_out={} ",
        output.exit_code, output.timed_out
    );

    rx_task.abort();
    let _ = rx_task.await;
    Ok(())
}
