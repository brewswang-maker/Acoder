//! 记忆系统 — 三级记忆架构
//!
//! Working Memory → Session Memory → Long-term Memory
//!
//! Working: 当前任务上下文，在 Context 层维护
//! Session: 会话期间的事件和产物，存储在 SQLite
//! Long-term: 跨会话的持久化知识，存储在 SQLite + 向量

pub mod working;
pub mod session;
pub mod longterm;
pub mod atomic;
pub mod embedding;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};

pub use working::WorkingMemory;
pub use session::SessionMemory;
pub use longterm::LongTermMemory;
pub use embedding::{EmbeddingProvider, MockEmbeddingProvider, cosine_similarity};

/// 记忆项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
    pub access_count: u32,
    pub importance: f64,
    pub embedding_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Working,
    Session,
    LongTerm,
    Atomic,
}

impl MemoryItem {
    pub fn new(content: String, memory_type: MemoryType) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content,
            memory_type,
            tags: Vec::new(),
            created_at: now,
            accessed_at: now,
            access_count: 0,
            importance: 0.5,
            embedding_id: None,
        }
    }
}

/// 记忆管理器
pub struct MemoryManager {
    session: SessionMemory,
    longterm: LongTermMemory,
}

impl MemoryManager {
    pub async fn new(data_dir: PathBuf) -> anyhow::Result<Self> {
        let session = SessionMemory::new(data_dir.join("sessions.db")).await?;
        let longterm = LongTermMemory::new(data_dir.join("memory.db")).await?;
        Ok(Self { session, longterm })
    }

    pub async fn store(&self, item: MemoryItem) -> anyhow::Result<()> {
        match item.memory_type {
            MemoryType::Session => self.session.store(&item).await,
            MemoryType::LongTerm => self.longterm.store(&item).await,
            _ => Ok(()),
        }
    }

    pub async fn retrieve(&self, query: &str, memory_type: MemoryType, limit: usize) -> anyhow::Result<Vec<MemoryItem>> {
        match memory_type {
            MemoryType::Session => self.session.retrieve(query, limit).await,
            MemoryType::LongTerm => self.longterm.retrieve(query, limit).await,
            _ => Ok(Vec::new()),
        }
    }
}
