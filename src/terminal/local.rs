use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::core::traits::{CommandOutput, TerminalBackend};
use crate::error::{Error, Result};

pub struct LocalTerminal;

impl LocalTerminal {
    pub fn new() -> Self { Self }
}

impl Default for LocalTerminal {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl TerminalBackend for LocalTerminal {
    async fn execute(&self, command: &str, timeout_secs: Option<u64>, workdir: Option<&str>) -> Result<CommandOutput> {
        let start = std::time::Instant::now();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn()
            .map_err(|e| Error::ExecutionFailed { lang: "bash".into(), reason: e.to_string() })?;

        let output = if let Some(secs) = timeout_secs {
            timeout(Duration::from_secs(secs), child.wait_with_output())
                .await
                .map_err(|_| Error::TaskTimeout(format!("命令执行超时 {}s", secs)))?
        } else {
            child.wait_with_output().await
        }.map_err(|e| Error::ExecutionFailed { lang: "bash".into(), reason: e.to_string() })?;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            elapsed_ms,
        })
    }

    async fn read_file(&self, path: &str) -> Result<String> {
        tokio::fs::read_to_string(path).await
            .map_err(|e| Error::FileNotFound { path: path.into() }.into())
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> bool {
        tokio::fs::metadata(path).await.is_ok()
    }

    fn backend_type(&self) -> &str { "local" }
}
