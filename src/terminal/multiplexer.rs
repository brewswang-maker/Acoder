//! 终端多路复用器 — 管理多个远程终端会话
//!
//! Phase 4 远程控制核心：
//! - 多 SSH 会话并行管理
//! - 会话生命周期管理
//! - 会话间隔离与安全

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use super::ssh::{SshClient, SshConfig, SshResult};

/// 会话 ID 类型
pub type SessionId = String;

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub status: SessionStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_active_at: chrono::DateTime<chrono::Utc>,
    pub commands_executed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
pub enum SessionStatus {
    Connecting,
    Connected,
    Idle,
    Error,
    Closed,
}

/// 终端多路复用器
pub struct TerminalMultiplexer {
    sessions: Arc<RwLock<HashMap<SessionId, ManagedSession>>>,
}

struct ManagedSession {
    client: SshClient,
    info: SessionInfo,
}

impl TerminalMultiplexer {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建新的 SSH 会话
    pub async fn create_session(&self, name: &str, config: SshConfig) -> Result<SessionId> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let client = SshClient::with_config(config.clone());
        let host = config.host.clone();
        let port = config.port;
        let user = config.user.clone();

        let info = SessionInfo {
            id: id.clone(),
            name: name.to_string(),
            host: host.clone(),
            port,
            user: user.clone(),
            status: SessionStatus::Connecting,
            created_at: now,
            last_active_at: now,
            commands_executed: 0,
        };

        {
            let mut sessions = self.sessions.write();
            sessions.insert(id.clone(), ManagedSession { client, info });
        }

        // 尝试连接
        let sessions = self.sessions.read();
        if let Some(session) = sessions.get(&id) {
            match session.client.connect().await {
                Ok(()) => {
                    drop(sessions);
                    self.update_status(&id, SessionStatus::Connected).await;
                    tracing::info!("会话创建成功: {} ({})", name, id);
                }
                Err(e) => {
                    drop(sessions);
                    self.update_status(&id, SessionStatus::Error).await;
                    return Err(Error::ExternalToolError {
                        tool: "ssh".into(),
                        reason: format!("会话 {} 连接失败: {}", id, e),
                    });
                }
            }
        }

        Ok(id)
    }

    /// 在指定会话中执行命令
    pub async fn execute(&self, session_id: &str, command: &str) -> Result<SshResult> {
        let sessions = self.sessions.read();
        let session = sessions.get(session_id)
            .ok_or_else(|| Error::SessionNotFound { session_id: session_id.into() })?;

        if session.info.status == SessionStatus::Closed {
            return Err(Error::SessionNotFound { session_id: session_id.into() });
        }

        let result = session.client.execute(command).await?;
        drop(sessions);

        // 更新活跃时间
        {
            let mut sessions = self.sessions.write();
            if let Some(session) = sessions.get_mut(session_id) {
                session.info.last_active_at = chrono::Utc::now();
                session.info.commands_executed += 1;
            }
        }

        Ok(result)
    }

    /// 列出所有会话
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions.read().values().map(|s| s.info.clone()).collect()
    }

    /// 获取会话信息
    pub fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        self.sessions.read().get(session_id).map(|s| s.info.clone())
    }

    /// 关闭会话
    pub async fn close_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write();
        if sessions.remove(session_id).is_some() {
            tracing::info!("会话已关闭: {}", session_id);
            Ok(())
        } else {
            Err(Error::SessionNotFound { session_id: session_id.into() })
        }
    }

    /// 关闭所有会话
    pub async fn close_all(&self) {
        let count = {
            let mut sessions = self.sessions.write();
            let count = sessions.len();
            sessions.clear();
            count
        };
        tracing::info!("已关闭 {} 个会话", count);
    }

    /// 活跃会话数量
    pub fn active_count(&self) -> usize {
        self.sessions.read().values()
            .filter(|s| s.info.status == SessionStatus::Connected)
            .count()
    }

    /// 在所有连接的会话中并行执行命令
    pub async fn execute_all(&self, command: &str) -> HashMap<SessionId, Result<SshResult>> {
        let sessions: Vec<(SessionId, SshClient)> = {
            let sessions = self.sessions.read();
            sessions.iter()
                .filter(|(_, s)| s.info.status == SessionStatus::Connected)
                .map(|(id, s)| (id.clone(), SshClient::with_config(s.client.config().clone())))
                .collect()
        };

        let mut results = HashMap::new();
        let mut join_set = tokio::task::JoinSet::new();

        for (id, client) in sessions {
            let cmd = command.to_string();
            join_set.spawn(async move {
                let result = client.execute(&cmd).await;
                (id, result)
            });
        }

        while let Some(out) = join_set.join_next().await {
            match out {
                Ok((id, result)) => { results.insert(id, result); }
                Err(e) => { tracing::error!("会话任务 panic: {}", e); }
            }
        }

        results
    }

    /// 更新会话状态
    async fn update_status(&self, session_id: &str, status: SessionStatus) {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(session_id) {
            session.info.status = status;
        }
    }
}

impl Default for TerminalMultiplexer {
    fn default() -> Self { Self::new() }
}
