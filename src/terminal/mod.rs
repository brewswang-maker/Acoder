//! # Terminal Backend — 命令执行环境抽象
//!
//! 支持 6 种执行后端：
//! - Local — 本地 shell
//! - Docker — Docker 容器（隔离）
//! - SSH — 远程服务器
//! - Daytona — 云开发环境
//! - Modal — Serverless compute
//! - Singularity — HPC 容器

pub mod local;
pub mod docker;
pub mod ssh;
pub mod multiplexer;

pub use local::LocalTerminal;
pub use docker::DockerTerminal;
pub use ssh::{SshClient, SshConfig, SshAuth, SshResult};
pub use multiplexer::{TerminalMultiplexer, SessionInfo, SessionStatus, SessionId};

use async_trait::async_trait;
use crate::core::traits::{CommandOutput, TerminalBackend};
use crate::error::{Error, Result};

/// 根据配置字符串创建对应的 TerminalBackend
pub fn create_backend(backend_type: &str, config: &BackendConfig) -> Result<Box<dyn TerminalBackend>> {
    match backend_type {
        "local" => Ok(Box::new(LocalTerminal::new())),
        "docker" => Ok(Box::new(DockerTerminal::new(
            config.docker_image.as_deref().unwrap_or("rust:1.75"),
            config.workdir.as_deref(),
        )?)),
        "ssh" => Err(Error::ConfigInvalid {
            key: "terminal.backend".into(),
            reason: "SSH backend 待实现".into(),
        }),
        "daytona" => Err(Error::ConfigInvalid {
            key: "terminal.backend".into(),
            reason: "Daytona backend 待实现".into(),
        }),
        "modal" => Err(Error::ConfigInvalid {
            key: "terminal.backend".into(),
            reason: "Modal backend 待实现".into(),
        }),
        "singularity" => Err(Error::ConfigInvalid {
            key: "terminal.backend".into(),
            reason: "Singularity backend 待实现".into(),
        }),
        _ => Err(Error::ConfigInvalid {
            key: format!("terminal.backend: {}", backend_type),
            reason: "不支持的后端类型".into(),
        }),
    }
}

#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub docker_image: Option<String>,
    pub workdir: Option<String>,
    pub ssh_host: Option<String>,
    pub ssh_user: Option<String>,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            docker_image: Some("rust:1.75".into()),
            workdir: None,
            ssh_host: None,
            ssh_user: None,
        }
    }
}
