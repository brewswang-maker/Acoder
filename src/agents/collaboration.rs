//! Agent 协作协议

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationProtocol {
    pub session_id: String,
    pub agents: Vec<AgentInfo>,
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub from: String,
    pub to: Option<String>,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub msg_type: MessageType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Request,
    Response,
    Notification,
    Handoff,
    Reject,
}

impl CollaborationProtocol {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            agents: Vec::new(),
            messages: Vec::new(),
        }
    }

    pub fn add_agent(&mut self, id: &str, name: &str, role: &str) {
        self.agents.push(AgentInfo {
            id: id.into(),
            name: name.into(),
            role: role.into(),
            status: AgentStatus::Idle,
        });
    }

    pub fn send(&mut self, from: &str, to: Option<&str>, content: &str, msg_type: MessageType) -> &AgentMessage {
        let msg = AgentMessage {
            id: Uuid::new_v4().to_string(),
            from: from.into(),
            to: to.map(String::from),
            content: content.into(),
            timestamp: chrono::Utc::now(),
            msg_type,
        };
        self.messages.push(msg);
        self.messages.last().unwrap()
    }
}
