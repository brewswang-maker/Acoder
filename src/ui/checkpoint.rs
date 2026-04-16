//! 检查点管理模块
//!
//! 功能：
//! - save: 保存当前状态到检查点
//! - list: 列出所有检查点
//! - restore: 从检查点恢复
//! - delete: 删除检查点
//! - auto_save: 自动保存（每N个工具调用后）

use crate::error::Result;
use crate::memory::{MemoryManager, MemoryItem, MemoryType};
use crate::ui::session_viewer::{
    AgentState, Checkpoint, ContextSnapshot, MemoryState, ResumeContext, SessionVisualizer,
};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

/// 检查点管理器
pub struct CheckpointManager {
    data_dir: PathBuf,
    session_visualizer: SessionVisualizer,
    /// 自动保存计数器
    auto_save_counter: usize,
    /// 自动保存间隔（工具调用数）
    auto_save_interval: usize,
}

impl CheckpointManager {
    /// 创建 CheckpointManager
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir: data_dir.clone(),
            session_visualizer: SessionVisualizer::new(data_dir),
            auto_save_counter: 0,
            auto_save_interval: 10, // 默认每10次工具调用自动保存
        }
    }

    /// 设置自动保存间隔
    pub fn with_auto_save_interval(mut self, interval: usize) -> Self {
        self.auto_save_interval = interval;
        self
    }

    /// 保存检查点
    ///
    /// 保存内容：
    /// - 上下文快照（任务描述、项目根目录、活跃文件、工作记忆摘要、工具结果、LLM对话历史）
    /// - Agent 状态
    /// - 记忆状态
    pub async fn save(
        &self,
        session_id: &str,
        description: &str,
        context_snapshot: ContextSnapshot,
        agent_state: HashMap<String, AgentState>,
        memory_state: MemoryState,
        event_index: usize,
    ) -> Result<Checkpoint> {
        self.session_visualizer
            .save_checkpoint(
                session_id,
                description,
                context_snapshot,
                agent_state,
                memory_state,
                event_index,
                false,
            )
            .await
    }

    /// 自动保存检查点（每 N 次工具调用）
    ///
    /// 返回 Some(checkpoint) 如果触发了自动保存，否则返回 None
    pub async fn auto_save(
        &mut self,
        session_id: &str,
        context_snapshot: ContextSnapshot,
        agent_state: HashMap<String, AgentState>,
        memory_state: MemoryState,
        event_index: usize,
    ) -> Result<Option<Checkpoint>> {
        self.auto_save_counter += 1;

        if self.auto_save_counter >= self.auto_save_interval {
            self.auto_save_counter = 0;
            let checkpoint = self
                .session_visualizer
                .save_checkpoint(
                    session_id,
                    &format!("[Auto-save] Tool call #{}", event_index),
                    context_snapshot,
                    agent_state,
                    memory_state,
                    event_index,
                    true,
                )
                .await?;
            Ok(Some(checkpoint))
        } else {
            Ok(None)
        }
    }

    /// 列出所有检查点
    pub async fn list(&self, session_id: &str) -> Result<Vec<CheckpointSummary>> {
        let checkpoints = self
            .session_visualizer
            .generate_timeline(session_id)
            .await?
            .checkpoints;

        Ok(checkpoints
            .into_iter()
            .map(|cp| CheckpointSummary {
                checkpoint_id: cp.checkpoint_id,
                session_id: cp.session_id,
                timestamp: cp.timestamp,
                description: cp.description,
                is_auto_save: cp.is_auto_save,
                event_index: cp.event_index,
            })
            .collect())
    }

    /// 从检查点恢复
    pub async fn restore(&self, session_id: &str, checkpoint_id: &str) -> Result<ResumeContext> {
        self.session_visualizer
            .resume_from_checkpoint(session_id, checkpoint_id)
            .await
    }

    /// 删除检查点
    pub async fn delete(&self, session_id: &str, checkpoint_id: &str) -> Result<()> {
        let path = self
            .data_dir
            .join("checkpoints")
            .join(session_id)
            .join(format!("{}.json", checkpoint_id));

        if !path.exists() {
            return Err(anyhow!("checkpoint {} not found", checkpoint_id).into());
        }

        fs::remove_file(&path).await?;
        tracing::info!("deleted checkpoint {} for session {}", checkpoint_id, session_id);
        Ok(())
    }

    /// 删除会话的所有检查点
    pub async fn delete_all(&self, session_id: &str) -> Result<usize> {
        let dir = self.data_dir.join("checkpoints").join(session_id);
        if !dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        let mut entries = fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                fs::remove_file(entry.path()).await?;
                count += 1;
            }
        }

        tracing::info!("deleted {} checkpoints for session {}", count, session_id);
        Ok(count)
    }

    /// 获取检查点详情
    pub async fn get(&self, session_id: &str, checkpoint_id: &str) -> Result<Checkpoint> {
        let path = self
            .data_dir
            .join("checkpoints")
            .join(session_id)
            .join(format!("{}.json", checkpoint_id));

        let content = fs::read_to_string(&path).await?;
        let checkpoint: Checkpoint = serde_json::from_str(&content)?;
        Ok(checkpoint)
    }

    /// 构建空的上下文快照
    pub fn empty_context_snapshot(task_description: &str, project_root: &str) -> ContextSnapshot {
        ContextSnapshot {
            task_description: task_description.to_string(),
            project_root: project_root.to_string(),
            active_files: Vec::new(),
            working_memory_summary: String::new(),
            recent_tool_results: Vec::new(),
            llm_conversation_history: Vec::new(),
        }
    }

    /// 构建空的 Agent 状态
    pub fn empty_agent_state(agent_id: &str, agent_name: &str) -> HashMap<String, AgentState> {
        let mut map = HashMap::new();
        map.insert(
            agent_id.to_string(),
            AgentState {
                agent_id: agent_id.to_string(),
                agent_name: agent_name.to_string(),
                current_task: String::new(),
                progress: 0.0,
                status: "idle".to_string(),
            },
        );
        map
    }

    /// 构建空的记忆状态
    pub fn empty_memory_state() -> MemoryState {
        MemoryState {
            working_memory_size: 0,
            session_memory_entries: 0,
            longterm_hints: Vec::new(),
        }
    }
}

/// 检查点摘要（用于列表展示）
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckpointSummary {
    pub checkpoint_id: String,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub is_auto_save: bool,
    pub event_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_snapshot() {
        let snap = CheckpointManager::empty_context_snapshot("test task", "/tmp");
        assert_eq!(snap.task_description, "test task");
        assert_eq!(snap.project_root, "/tmp");
    }
}
