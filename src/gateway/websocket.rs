//! WebSocket 实时通信模块
//!
//! 基于 axum + tokio-tungstenite 的 WebSocket 服务，用于编辑器实时通信。
//! 参考 claw-code（ironclaw）的 channels/web WebSocket 实现。
//!
//! 核心能力：
//! - WsServer — WebSocket 服务器，支持多客户端连接
//! - WsSession — 单个 WebSocket 会话，双向消息传递
//! - WsMessage — 消息枚举，JSON 序列化
//! - 心跳保活机制（30s 间隔）

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use crate::error::{Error, Result};

// ── 消息类型 ──────────────────────────────────────────────────────────────

/// WebSocket 消息枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsMessage {
    /// 客户端 → 服务端：任务请求
    TaskRequest {
        task_id: String,
        task: String,
        model: Option<String>,
    },
    /// 服务端 → 客户端：任务进度
    TaskProgress {
        task_id: String,
        step: u32,
        total: Option<u32>,
        message: String,
    },
    /// 服务端 → 客户端：任务结果
    TaskResult {
        task_id: String,
        success: bool,
        result: Option<String>,
        error: Option<String>,
    },
    /// 客户端 → 服务端：编辑请求
    EditRequest {
        request_id: String,
        file_path: String,
        content: String,
        action: EditAction,
    },
    /// 服务端 → 客户端：编辑结果
    EditResult {
        request_id: String,
        success: bool,
        diff: Option<String>,
        error: Option<String>,
    },
    /// 心跳请求
    Ping {
        timestamp: u64,
    },
    /// 心跳响应
    Pong {
        timestamp: u64,
    },
    /// 错误消息
    Error {
        code: String,
        message: String,
    },
}

/// 编辑动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditAction {
    /// 插入内容
    Insert,
    /// 替换内容
    Replace,
    /// 删除内容
    Delete,
}

// ── 会话信息 ──────────────────────────────────────────────────────────────

/// WebSocket 会话信息
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// 会话 ID
    pub session_id: String,
    /// 连接时间
    pub connected_at: u64,
}

// ── 共享状态 ──────────────────────────────────────────────────────────────

/// WebSocket 服务器共享状态
#[derive(Debug, Clone)]
pub struct WsState {
    /// 会话列表
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    /// 广播通道：向所有客户端推送消息
    broadcast_tx: broadcast::Sender<(String, WsMessage)>,
}

impl WsState {
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
        }
    }

    /// 向指定会话发送消息（通过广播）
    pub async fn send_to(&self, session_id: &str, msg: WsMessage) {
        // 广播携带目标 session_id，各 handler 自行过滤
        let _ = self.broadcast_tx.send((session_id.to_string(), msg));
    }

    /// 向所有会话广播消息
    pub async fn broadcast(&self, msg: WsMessage) {
        let _ = self.broadcast_tx.send(("__all__".to_string(), msg));
    }

    /// 获取当前在线会话数
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// 注册新会话
    async fn register_session(&self, session_id: String) {
        let info = SessionInfo {
            session_id: session_id.clone(),
            connected_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        self.sessions.write().await.insert(session_id, info);
        let count = self.sessions.read().await.len();
        tracing::info!(session_count = count, "新 WebSocket 会话已注册");
    }

    /// 注销会话
    async fn unregister_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
        let count = self.sessions.read().await.len();
        tracing::info!(session_count = count, "WebSocket 会话已注销");
    }
}

// ── WebSocket 服务器 ──────────────────────────────────────────────────────

/// WebSocket 服务器配置
#[derive(Debug, Clone)]
pub struct WsServerConfig {
    /// 心跳间隔（秒）
    pub heartbeat_interval_secs: u64,
    /// 最大消息大小（字节）
    pub max_message_size: usize,
}

impl Default for WsServerConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 30,
            max_message_size: 16 * 1024 * 1024, // 16 MB
        }
    }
}

/// WebSocket 服务器
pub struct WsServer {
    config: WsServerConfig,
    state: WsState,
}

impl WsServer {
    /// 创建新的 WebSocket 服务器
    pub fn new(config: WsServerConfig) -> Self {
        Self {
            config,
            state: WsState::new(),
        }
    }

    /// 获取共享状态的克隆（用于外部发送消息）
    pub fn state(&self) -> WsState {
        self.state.clone()
    }

    /// 启动 WebSocket 监听
    pub async fn run(self, addr: SocketAddr) -> Result<()> {
        let state = self.state;
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .with_state(state);

        let listener = TcpListener::bind(addr).await.map_err(|e| {
            Error::GatewayRequestFailed(format!("WebSocket 绑定地址 {} 失败: {}", addr, e))
        })?;

        tracing::info!("WebSocket 服务启动于 ws://{}", addr);
        axum::serve(listener, app)
            .await
            .map_err(|e| Error::GatewayRequestFailed(format!("WebSocket 服务错误: {}", e)))?;

        Ok(())
    }
}

// ── WebSocket 升级处理 ────────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// ── 单个 WebSocket 连接处理 ───────────────────────────────────────────────

/// 处理单个 WebSocket 连接（WsSession）
async fn handle_socket(socket: WebSocket, state: WsState) {
    let session_id = Uuid::new_v4().to_string();
    state.register_session(session_id.clone()).await;

    let (mut ws_sink, mut ws_stream) = socket.split();
    let (local_tx, mut local_rx) = mpsc::channel::<WsMessage>(256);

    tracing::info!(session_id = %session_id, "WebSocket 会话已建立");

    // 心跳定时器
    let heartbeat_interval = tokio::time::Duration::from_secs(30);

    loop {
        tokio::select! {
            // 接收来自客户端的消息
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsMessage>(&text) {
                            Ok(ws_msg) => {
                                tracing::debug!(session_id = %session_id, msg_type = ?std::mem::discriminant(&ws_msg), "收到客户端消息");
                                handle_incoming_message(&session_id, ws_msg).await;
                            }
                            Err(e) => {
                                tracing::warn!(session_id = %session_id, "消息解析失败: {}", e);
                                let _ = ws_sink.send(Message::Text(
                                    serde_json::to_string(&WsMessage::Error {
                                        code: "PARSE_ERROR".into(),
                                        message: format!("JSON 解析失败: {}", e),
                                    }).unwrap_or_default().into(),
                                )).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_sink.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!(session_id = %session_id, "客户端断开连接");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!(session_id = %session_id, "WebSocket 错误: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            // 接收来自本地的发送请求
            Some(local_msg) = local_rx.recv() => {
                if let Ok(text) = serde_json::to_string(&local_msg) {
                    if ws_sink.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }

            // 心跳
            _ = tokio::time::sleep(heartbeat_interval) => {
                let ping = WsMessage::Ping {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                };
                if let Ok(text) = serde_json::to_string(&ping) {
                    if ws_sink.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    state.unregister_session(&session_id).await;
    tracing::info!(session_id = %session_id, "WebSocket 会话已关闭");
}

// ── 消息处理 ──────────────────────────────────────────────────────────────

/// 处理来自客户端的消息
async fn handle_incoming_message(session_id: &str, msg: WsMessage) {
    match msg {
        WsMessage::Ping { timestamp } => {
            tracing::trace!(session_id = %session_id, timestamp = timestamp, "收到 Ping");
            // Pong 由调用方通过 state.send_to 发送
        }
        WsMessage::Pong { timestamp } => {
            tracing::trace!(session_id = %session_id, timestamp = timestamp, "收到 Pong");
        }
        WsMessage::TaskRequest { task_id, task, model } => {
            tracing::info!(
                session_id = %session_id,
                task_id = %task_id,
                task = %task,
                model = ?model,
                "收到任务请求"
            );
            // TODO: 将任务提交到执行引擎
        }
        WsMessage::EditRequest { request_id, file_path, content, action } => {
            tracing::info!(
                session_id = %session_id,
                request_id = %request_id,
                file_path = %file_path,
                action = ?action,
                "收到编辑请求"
            );
            // TODO: 执行编辑操作
        }
        WsMessage::Error { code, message } => {
            tracing::warn!(
                session_id = %session_id,
                code = %code,
                message = %message,
                "收到客户端错误"
            );
        }
        _ => {
            tracing::debug!(session_id = %session_id, "忽略非请求类型消息");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_message_serialization() {
        let msg = WsMessage::Ping { timestamp: 12345 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"Ping\""));

        let parsed: WsMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, WsMessage::Ping { timestamp: 12345 }));
    }

    #[test]
    fn test_task_request_roundtrip() {
        let msg = WsMessage::TaskRequest {
            task_id: "task-001".into(),
            task: "写一个排序函数".into(),
            model: Some("gpt-4o".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: WsMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, WsMessage::TaskRequest { .. }));
    }
}
