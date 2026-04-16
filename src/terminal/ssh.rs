//! SSH 远程终端 — 远程服务器连接和命令执行
//!
//! Phase 4 远程控制基础能力：
//! - 使用系统 ssh/scp 命令（不引入 ssh2 crate）
//! - 超时控制 + 安全检查
//! - 支持命令执行、文件上传/下载

use std::path::PathBuf;
use std::time::Duration;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// SSH 连接配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// 主机地址
    pub host: String,
    /// 端口
    pub port: u16,
    /// 用户名
    pub user: String,
    /// 认证方式
    pub auth: SshAuth,
    /// 连接超时（秒）
    pub connect_timeout: u64,
    /// 命令执行超时（秒）
    pub command_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshAuth {
    /// 密钥文件路径
    KeyFile(String),
    /// 密码（不推荐，仅开发用）
    Password(String),
    /// SSH Agent
    Agent,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 22,
            user: "root".into(),
            auth: SshAuth::Agent,
            connect_timeout: 10,
            command_timeout: 30,
        }
    }
}

/// SSH 命令执行结果
#[derive(Debug, Clone)]
pub struct SshResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

/// SSH 客户端 — 通过系统 ssh/scp 命令执行远程操作
pub struct SshClient {
    config: SshConfig,
}

impl SshClient {
    pub fn new(host: impl Into<String>, port: u16, user: impl Into<String>) -> Self {
        Self {
            config: SshConfig {
                host: host.into(),
                port,
                user: user.into(),
                ..Default::default()
            },
        }
    }

    pub fn with_config(config: SshConfig) -> Self {
        Self { config }
    }

    /// 构建 ssh 基础参数
    fn ssh_base_args(&self) -> Vec<String> {
        let mut args = vec![
            "-o".into(), "StrictHostKeyChecking=accept-new".into(),
            "-o".into(), "ConnectTimeout=10".into(),
            "-p".into(), self.config.port.to_string(),
            format!("{}@{}", self.config.user, self.config.host),
        ];
        match &self.config.auth {
            SshAuth::KeyFile(path) => {
                args.insert(0, "-i".into());
                args.insert(1, path.clone());
            }
            SshAuth::Agent => {}
            SshAuth::Password(_) => {} // 密码通过 SSH_ASKPASS 处理
        }
        args
    }

    /// 测试连接
    pub async fn connect(&self) -> Result<()> {
        let args = {
            let mut base = self.ssh_base_args();
            base.push("exit".into());
            base
        };

        let output = tokio::time::timeout(
            Duration::from_secs(self.config.connect_timeout),
            tokio::process::Command::new("ssh").args(&args).output(),
        ).await
            .map_err(|_| Error::TaskTimeout("SSH 连接超时".into()))?
            .map_err(|e| Error::ExternalToolError { tool: "ssh".into(), reason: e.to_string() })?;

        if output.status.success() {
            tracing::info!("SSH 连接成功: {}@{}:{}", self.config.user, self.config.host, self.config.port);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(Error::ExternalToolError {
                tool: "ssh".into(),
                reason: format!("连接失败: {}", stderr),
            })
        }
    }

    /// 远程执行命令
    pub async fn execute(&self, command: &str) -> Result<SshResult> {
        let start = std::time::Instant::now();

        // 安全检查
        let blocked = ["rm -rf /", "mkfs", "dd if=", "> /dev/sd"];
        for b in &blocked {
            if command.contains(b) {
                return Err(Error::SandboxSecurityBlocked {
                    operation: "ssh_execute".into(),
                    reason: format!("危险命令被拦截: {}", b),
                });
            }
        }

        let mut args = self.ssh_base_args();
        args.push(command.into());

        let output = tokio::time::timeout(
            Duration::from_secs(self.config.command_timeout),
            tokio::process::Command::new("ssh").args(&args).output(),
        ).await
            .map_err(|_| Error::TaskTimeout(format!("SSH 命令执行超时 ({}s)", self.config.command_timeout)))?
            .map_err(|e| Error::ExternalToolError { tool: "ssh".into(), reason: e.to_string() })?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        tracing::debug!("SSH 命令完成 ({}ms): exit={}", duration_ms, exit_code);

        Ok(SshResult { stdout, stderr, exit_code, duration_ms })
    }

    /// 上传文件（scp）
    pub async fn upload(&self, local_path: &str, remote_path: &str) -> Result<()> {
        let remote = format!("{}@{}:{}", self.config.user, self.config.host, remote_path);
        let args = match &self.config.auth {
            SshAuth::KeyFile(path) => vec!["-i".into(), path.clone(), local_path.into(), remote],
            _ => vec![local_path.into(), remote],
        };

        let output = tokio::time::timeout(
            Duration::from_secs(120),
            tokio::process::Command::new("scp").args(&args).output(),
        ).await
            .map_err(|_| Error::TaskTimeout("SCP 上传超时".into()))?
            .map_err(|e| Error::ExternalToolError { tool: "scp".into(), reason: e.to_string() })?;

        if output.status.success() {
            tracing::info!("SCP 上传成功: {} → {}", local_path, remote_path);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(Error::ExternalToolError { tool: "scp".into(), reason: format!("上传失败: {}", stderr) })
        }
    }

    /// 下载文件（scp）
    pub async fn download(&self, remote_path: &str, local_path: &str) -> Result<()> {
        let remote = format!("{}@{}:{}", self.config.user, self.config.host, remote_path);
        let args = match &self.config.auth {
            SshAuth::KeyFile(path) => vec!["-i".into(), path.clone(), remote, local_path.into()],
            _ => vec![remote, local_path.into()],
        };

        let output = tokio::time::timeout(
            Duration::from_secs(120),
            tokio::process::Command::new("scp").args(&args).output(),
        ).await
            .map_err(|_| Error::TaskTimeout("SCP 下载超时".into()))?
            .map_err(|e| Error::ExternalToolError { tool: "scp".into(), reason: e.to_string() })?;

        if output.status.success() {
            tracing::info!("SCP 下载成功: {} → {}", remote_path, local_path);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(Error::ExternalToolError { tool: "scp".into(), reason: format!("下载失败: {}", stderr) })
        }
    }

    pub fn config(&self) -> &SshConfig {
        &self.config
    }
}
