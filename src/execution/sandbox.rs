//! Sandbox — 安全沙箱执行

use std::sync::Arc;
use std::path::PathBuf;
use crate::config::Config;
use crate::error::{Error, Result};

pub struct Sandbox {
    config: Arc<Config>,
}

impl Sandbox {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        // 验证 Docker 是否可用
        if config.security.sandbox_enabled {
            let docker_check = tokio::process::Command::new("docker")
                .arg("--version")
                .output()
                .await;

            if docker_check.is_err() {
                tracing::warn!("Docker 不可用，沙箱将使用本地执行模式");
            }
        }

        Ok(Self { config })
    }

    /// 在沙箱中执行命令
    pub async fn execute(&self, command: &str, cwd: &PathBuf) -> Result<String> {
        if !self.config.security.sandbox_enabled {
            return self.execute_native(command, cwd).await;
        }

        match self.config.sandbox.runtime {
            crate::config::SandboxRuntime::Docker => {
                self.execute_docker(command, cwd).await
            }
            crate::config::SandboxRuntime::Native => {
                self.execute_native(command, cwd).await
            }
            crate::config::SandboxRuntime::Wasm => {
                Err(Error::SandboxSecurityBlocked {
                    operation: "wasm".into(),
                    reason: "WASM 沙箱暂未实现".into(),
                })
            }
        }
    }

    async fn execute_docker(&self, command: &str, cwd: &PathBuf) -> Result<String> {
        let image = "rust:1.92-slim";
        let timeout = self.config.sandbox.timeout_secs;

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("run")
            .arg("--rm")
            .arg("--network=none")
            .arg(format!("--memory={}m", self.config.sandbox.memory_mb))
            .arg(format!("--cpus={}", self.config.sandbox.cpu_limit))
            .arg(format!("--pids-limit=100"))
            .arg("-v")
            .arg(format!("{}:/workspace", cwd.display()))
            .arg("-w")
            .arg("/workspace")
            .arg(image)
            .arg("sh")
            .arg("-c")
            .arg(command);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            cmd.output()
        ).await
            .map_err(|_| Error::SandboxTimeout { timeout })?
            .map_err(|e| Error::ExecutionFailed { lang: "docker".into(), reason: e.to_string() })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.to_string())
        } else {
            Err(Error::ExecutionFailed {
                lang: "docker".into(),
                reason: format!("exit {}: {} / {}", output.status, stdout, stderr),
            })
        }
    }

    async fn execute_native(&self, command: &str, cwd: &PathBuf) -> Result<String> {
        // 安全检查
        let blocked = ["rm -rf /", "sudo", "chmod 777", "eval ", "bash -c "];
        for b in blocked {
            if command.contains(b) {
                return Err(Error::SandboxSecurityBlocked {
                    operation: "execute_native".into(),
                    reason: format!("禁止执行: {}", b),
                });
            }
        }

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .output()
            .await
            .map_err(|e| Error::ExecutionFailed { lang: "shell".into(), reason: e.to_string() })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.to_string())
        } else {
            Err(Error::ExecutionFailed {
                lang: "shell".into(),
                reason: format!("exit {}: {}", output.status, stderr),
            })
        }
    }
}
