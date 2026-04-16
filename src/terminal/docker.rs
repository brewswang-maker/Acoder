use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::core::traits::{CommandOutput, TerminalBackend};
use crate::error::{Error, Result};

pub struct DockerTerminal {
    image: String,
    workdir: Option<String>,
}

impl DockerTerminal {
    pub fn new(image: &str, workdir: Option<&str>) -> Result<Self> {
        Ok(Self {
            image: image.into(),
            workdir: workdir.map(String::from),
        })
    }

    async fn docker_available() -> bool {
        Command::new("docker")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[async_trait]
impl TerminalBackend for DockerTerminal {
    async fn execute(&self, command: &str, timeout_secs: Option<u64>, _workdir: Option<&str>) -> Result<CommandOutput> {
        if !Self::docker_available().await {
            return Err(Error::ExecutionFailed {
                lang: "docker".into(),
                reason: "Docker 不可用，请确保 Docker 已安装并运行".into(),
            });
        }

        let start = std::time::Instant::now();
        let mut args = vec![
            "run", "--rm", "-i",
            "--network", "none",      // 网络隔离，安全
            "--memory", "512m",      // 内存限制
            "--pids-limit", "64",    // 进程数限制
        ];

        // 添加工作目录
        if let Some(ref wd) = self.workdir {
            args.push("-w");
            args.push(wd);
        } else {
            args.push("-w");
            args.push("/workspace");
        }

        args.push(&self.image);
        args.extend(["sh", "-c", command]);

        let mut cmd = Command::new("docker");
        cmd.args(&args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn()
            .map_err(|e| Error::ExecutionFailed {
                lang: "docker".into(),
                reason: format!("Docker 启动失败: {}", e),
            })?;

        let output = if let Some(secs) = timeout_secs {
            timeout(Duration::from_secs(secs), child.wait_with_output())
                .await
                .map_err(|_| Error::SandboxTimeout { timeout: secs })?
        } else {
            child.wait_with_output().await
        }.map_err(|e| Error::ExecutionFailed {
            lang: "docker".into(),
            reason: e.to_string(),
        })?;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            elapsed_ms,
        })
    }

    async fn read_file(&self, path: &str) -> Result<String> {
        let mut cmd = Command::new("docker");
        cmd.args(["cp", &format!("{}:/workspace/{}", self.image, path), "-"])
            .stdout(Stdio::piped());
        let child = cmd.spawn()
            .map_err(|e| Error::ExecutionFailed { lang: "docker".into(), reason: e.to_string() })?;
        let out = child.wait_with_output().await
            .map_err(|e| Error::ExecutionFailed { lang: "docker".into(), reason: e.to_string() })?;
        String::from_utf8(out.stdout)
            .map_err(|e| Error::ExecutionFailed { lang: "docker".into(), reason: e.to_string() })
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let mut cmd = Command::new("docker");
        cmd.args(["cp", "-", &format!("{}:/workspace/{}", self.image, path)]);
        let mut child = cmd.spawn()
            .map_err(|e| Error::ExecutionFailed { lang: "docker".into(), reason: e.to_string() })?;
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(content.as_bytes()).await?;
            stdin.shutdown().await?;
        }
        child.wait().await
            .map_err(|e| Error::ExecutionFailed { lang: "docker".into(), reason: e.to_string() })?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> bool {
        Command::new("docker")
            .args(["exec", &self.image, "test", "-e", path])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn backend_type(&self) -> &str { "docker" }
}
