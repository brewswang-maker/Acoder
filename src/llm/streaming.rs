//! 流式输出 — SSE / WebSocket 实时响应
//!
//! 支持：
//! - Server-Sent Events (SSE)：HTTP 长连接推送
//! - WebSocket：双向实时通信
//! - 终端流式输出：逐字符显示

use std::pin::Pin;
use std::sync::Arc;
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// 流式事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// 文本增量
    TextDelta { content: String },
    /// 思考过程增量（CoT 推理）
    ThinkingDelta { content: String },
    /// 工具调用开始
    ToolCallStart { tool: String, call_id: String },
    /// 工具调用参数增量
    ToolCallDelta { call_id: String, arguments: String },
    /// 工具调用结束
    ToolCallEnd { call_id: String, result: String },
    /// 完成
    Done { reason: FinishReason },
    /// 错误
    Error { message: String },
}

/// 完成原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolCall,
    MaxTokens,
    Cancelled,
    Error,
}

/// 流式输出管理器
pub struct StreamManager {
    sender: broadcast::Sender<StreamEvent>,
}

impl StreamManager {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    /// 订阅流式事件
    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.sender.subscribe()
    }

    /// 发送文本增量
    pub fn send_text(&self, content: &str) {
        let _ = self.sender.send(StreamEvent::TextDelta { content: content.to_string() });
    }

    /// 发送思考增量
    pub fn send_thinking(&self, content: &str) {
        let _ = self.sender.send(StreamEvent::ThinkingDelta { content: content.to_string() });
    }

    /// 发送完成事件
    pub fn send_done(&self, reason: FinishReason) {
        let _ = self.sender.send(StreamEvent::Done { reason });
    }

    /// 发送错误事件
    pub fn send_error(&self, message: &str) {
        let _ = self.sender.send(StreamEvent::Error { message: message.to_string() });
    }

    /// 将流式事件转换为 SSE 格式
    pub fn to_sse(event: &StreamEvent) -> String {
        let json = serde_json::to_string(event).unwrap_or_default();
        format!("data: {}\n\n", json)
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}
