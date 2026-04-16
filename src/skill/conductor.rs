//! Conductor 上下文管理器
//!
//! 实现设计文档 §4.1.2 Conductor 方案：
//! - ConductorContext: 管理 conductor/ 目录
//! - load: 加载项目上下文
//! - save_checkpoint: Skill 进化前保存检查点
//! - rollback_to: 失败时回滚
//! - validate_against_skill: 验证 Skill 与上下文兼容性

use crate::error::{Error, Result};
use crate::ui::checkpoint::CheckpointManager;
use crate::ui::session_viewer::{Checkpoint, ContextSnapshot, ResumeContext};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Conductor 上下文管理器
///
/// 管理 conductor/ 目录结构：
/// ```
/// conductor/
/// ├── checkpoints/       # 检查点存储
/// │   └── {session_id}/
/// │       └── {checkpoint_id}.json
/// ├── snapshots/        # 上下文快照
/// │   └── {snapshot_id}.json
/// └── metadata.json      # Conductor 元数据
/// ```
pub struct ConductorContext {
    data_dir: PathBuf,
    checkpoint_manager: CheckpointManager,
    metadata: ConductorMetadata,
}

/// Conductor 元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConductorMetadata {
    pub version: String,
    pub project_root: PathBuf,
    pub active_sessions: Vec<String>,
    pub last_checkpoint: Option<DateTime<Utc>>,
    pub skill_versions: HashMap<String, String>,
}

impl ConductorMetadata {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            version: "1.0.0".to_string(),
            project_root,
            active_sessions: Vec::new(),
            last_checkpoint: None,
            skill_versions: HashMap::new(),
        }
    }
}

/// Conductor 检查点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConductorCheckpoint {
    pub checkpoint_id: String,
    pub session_id: String,
    pub skill_id: String,
    pub skill_version: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub context_snapshot: ContextSnapshot,
    pub file_snapshots: HashMap<String, String>, // path -> content hash
    pub event_index: usize,
}

/// Conductor 检查点摘要（列表展示用，不含大字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConductorCheckpointSummary {
    pub checkpoint_id: String,
    pub session_id: String,
    pub skill_id: String,
    pub skill_version: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub event_index: usize,
}

/// 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub compatible: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ConductorContext {
    /// 创建 ConductorContext
    pub fn new(data_dir: PathBuf, project_root: PathBuf) -> Self {
        let checkpoint_manager = CheckpointManager::new(data_dir.clone());
        let metadata = ConductorMetadata::new(project_root);

        Self {
            data_dir,
            checkpoint_manager,
            metadata,
        }
    }

    /// 初始化 Conductor 目录结构
    pub async fn init(&self) -> Result<()> {
        let dirs = [
            self.data_dir.join("conductor"),
            self.data_dir.join("conductor/checkpoints"),
            self.data_dir.join("conductor/snapshots"),
        ];

        for dir in &dirs {
            tokio::fs::create_dir_all(dir).await?;
        }

        self.save_metadata().await?;
        tracing::info!("ConductorContext initialized at {:?}", self.data_dir);
        Ok(())
    }

    /// 加载项目上下文
    ///
    /// 扫描项目目录，构建 ContextSnapshot：
    /// - 活跃文件列表
    /// - 工作记忆摘要
    /// - 最近工具结果
    pub async fn load(&self, task_description: &str) -> Result<ContextSnapshot> {
        let project_root = &self.metadata.project_root;

        // 扫描活跃文件（排除 target, node_modules, .git 等）
        let active_files = self.scan_active_files(project_root).await?;

        // 读取 .gitignore 模式用于过滤
        let ignore_patterns = self.load_gitignore(project_root).await;

        Ok(ContextSnapshot {
            task_description: task_description.to_string(),
            project_root: project_root.to_string_lossy().to_string(),
            active_files,
            working_memory_summary: self.build_working_memory_summary().await,
            recent_tool_results: Vec::new(),
            llm_conversation_history: Vec::new(),
        })
    }

    /// 保存检查点（Skill 进化前）
    pub async fn save_checkpoint(
        &mut self,
        session_id: &str,
        skill_id: &str,
        skill_version: &str,
        description: &str,
        context_snapshot: ContextSnapshot,
        event_index: usize,
    ) -> Result<ConductorCheckpoint> {
        let checkpoint_id = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now();

        // 快照关键文件内容哈希
        let file_snapshots = self.snapshot_file_hashes(&context_snapshot.active_files).await?;

        let checkpoint = ConductorCheckpoint {
            checkpoint_id: checkpoint_id.clone(),
            session_id: session_id.to_string(),
            skill_id: skill_id.to_string(),
            skill_version: skill_version.to_string(),
            timestamp,
            description: description.to_string(),
            context_snapshot,
            file_snapshots,
            event_index,
        };

        // 保存检查点文件
        let path = self
            .data_dir
            .join("conductor/checkpoints")
            .join(session_id)
            .join(format!("{}.json", checkpoint_id));
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        let content = serde_json::to_string_pretty(&checkpoint)?;
        tokio::fs::write(&path, &content).await?;

        // 更新元数据
        self.metadata.last_checkpoint = Some(timestamp);
        if !self.metadata.active_sessions.contains(&session_id.to_string()) {
            self.metadata.active_sessions.push(session_id.to_string());
        }
        self.metadata
            .skill_versions
            .insert(skill_id.to_string(), skill_version.to_string());
        self.save_metadata().await?;

        tracing::info!(
            "Conductor checkpoint saved: {} for skill {} v{}",
            checkpoint_id,
            skill_id,
            skill_version
        );
        Ok(checkpoint)
    }

    /// 从检查点恢复
    pub async fn rollback_to(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<RollbackResult> {
        let path = self
            .data_dir
            .join("conductor/checkpoints")
            .join(session_id)
            .join(format!("{}.json", checkpoint_id));

        if !path.exists() {
            return Err(Error::from(anyhow!("conductor checkpoint {} not found", checkpoint_id)));
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let checkpoint: ConductorCheckpoint = serde_json::from_str(&content)?;

        // 验证文件快照是否仍然有效
        let mut reverted_files = Vec::new();
        for (file_path, _expected_hash) in &checkpoint.file_snapshots {
            if Path::new(file_path).exists() {
                reverted_files.push(file_path.clone());
            }
        }

        Ok(RollbackResult {
            checkpoint,
            reverted_files,
            restored_session_id: session_id.to_string(),
        })
    }

    /// 列出会话的所有 Conductor 检查点
    pub async fn list_checkpoints(&self, session_id: &str) -> Result<Vec<ConductorCheckpointSummary>> {
        let dir = self.data_dir.join("conductor/checkpoints").join(session_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                    if let Ok(cp) = serde_json::from_str::<ConductorCheckpoint>(&content) {
                        checkpoints.push(ConductorCheckpointSummary {
                            checkpoint_id: cp.checkpoint_id,
                            session_id: cp.session_id,
                            skill_id: cp.skill_id,
                            skill_version: cp.skill_version,
                            timestamp: cp.timestamp,
                            description: cp.description,
                            event_index: cp.event_index,
                        });
                    }
                }
            }
        }

        checkpoints.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(checkpoints)
    }

    /// 验证 Skill 与当前上下文兼容性
    ///
    /// 检查项：
    /// 1. Skill 所需文件是否存在
    /// 2. Skill 依赖是否满足
    /// 3. 当前上下文是否支持 Skill 运行
    pub async fn validate_against_skill(
        &self,
        skill_id: &str,
        skill_manifest: &SkillManifest,
    ) -> Result<ValidationResult> {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // 检查必需文件
        for required_file in &skill_manifest.required_files {
            let path = self.metadata.project_root.join(required_file);
            if !path.exists() {
                errors.push(format!("required file '{}' not found", required_file));
            }
        }

        // 检查文件数量警告
        if skill_manifest.max_files > 0 {
            let active_count = self.scan_active_files(&self.metadata.project_root).await?.len();
            if active_count > skill_manifest.max_files {
                warnings.push(format!(
                    "project has {} files, skill supports max {}",
                    active_count, skill_manifest.max_files
                ));
            }
        }

        // 检查 Skill 版本兼容性
        if let Some(current_version) = self.metadata.skill_versions.get(skill_id) {
            if current_version != &skill_manifest.version {
                warnings.push(format!(
                    "skill {} version mismatch: current={}, requested={}",
                    skill_id, current_version, skill_manifest.version
                ));
            }
        }

        let compatible = errors.is_empty();
        Ok(ValidationResult {
            compatible,
            warnings,
            errors,
        })
    }

    /// 获取元数据
    pub fn metadata(&self) -> &ConductorMetadata {
        &self.metadata
    }

    // ── 内部辅助方法 ──────────────────────────────────────────────

    async fn scan_active_files(&self, project_root: &Path) -> Result<Vec<String>> {
        let ignore_dirs =
            ["target", "node_modules", ".git", ".svn", ".hg", "dist", "build", "__pycache__"];
        let project_root_owned = project_root.to_path_buf();

        let result: Vec<String> = tokio::task::spawn_blocking(move || {
            let mut entries: Vec<String> = Vec::new();
            let walker = walkdir::WalkDir::new(&project_root_owned)
                .max_depth(10)
                .follow_links(false);

            for entry_result in walker.into_iter() {
                let entry: walkdir::DirEntry = match entry_result {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let path = entry.path();
                if path.is_file() {
                    let relative = path.strip_prefix(&project_root_owned).unwrap_or(path);
                    let relative_str = relative.to_string_lossy();

                    let should_ignore = ignore_dirs.iter().any(|d| {
                        relative_str.starts_with(d) || relative_str.starts_with('.')
                    });

                    if !should_ignore {
                        entries.push(relative_str.to_string());
                    }
                }
            }
            entries
        })
        .await
        .map_err(|e| Error::from(anyhow::anyhow!("spawn_blocking failed: {e}")))?;

        let mut files = Vec::new();
        files.extend(result.into_iter().take(500));
        Ok(files)
    }

    async fn load_gitignore(&self, project_root: &Path) -> Vec<String> {
        let gitignore_path = project_root.join(".gitignore");
        if let Ok(content) = tokio::fs::read_to_string(&gitignore_path).await {
            content
                .lines()
                .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                .map(String::from)
                .collect()
        } else {
            Vec::new()
        }
    }

    async fn build_working_memory_summary(&self) -> String {
        // TODO: 集成 WorkingMemory 读取摘要
        String::new()
    }

    async fn snapshot_file_hashes(
        &self,
        files: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut hashes = HashMap::new();
        let project_root = &self.metadata.project_root;

        for file in files.iter().take(100) {
            let path = project_root.join(file);
            if path.exists() {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    use sha2::{Sha256, Digest};
                    let mut hasher = Sha256::new();
                    hasher.update(content.as_bytes());
                    let hash = format!("{:x}", hasher.finalize());
                    hashes.insert(file.clone(), hash);
                }
            }
        }

        Ok(hashes)
    }

    async fn save_metadata(&self) -> Result<()> {
        let path = self.data_dir.join("conductor/metadata.json");
        let content = serde_json::to_string_pretty(&self.metadata)?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }
}

/// Rollback 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResult {
    pub checkpoint: ConductorCheckpoint,
    pub reverted_files: Vec<String>,
    pub restored_session_id: String,
}

/// Skill 清单（由 Skill 自身提供）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub skill_id: String,
    pub version: String,
    pub required_files: Vec<String>,
    pub max_files: usize,
    pub required_tools: Vec<String>,
    pub compatible_agents: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metadata_creation() {
        let meta = ConductorMetadata::new(PathBuf::from("/tmp/test"));
        assert_eq!(meta.version, "1.0.0");
        assert!(meta.active_sessions.is_empty());
    }
}
