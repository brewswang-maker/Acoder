//! OutcomeSignal — 任务执行后的反馈信号
//!
//! 每个任务执行完后记录 OutcomeSignal，用于后续学习。

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 任务类型（用于模型选择的历史统计分组）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// 代码补全 / 重构
    CodeCompletion,
    /// Bug 定位与修复
    BugFix,
    /// 写测试用例
    WriteTests,
    /// 项目初始化
    ProjectInit,
    /// 代码审查
    CodeReview,
    /// 添加功能
    AddFeature,
    /// 文档生成
    Docs,
    /// 技术调研
    Research,
    /// 其他通用任务
    General,
}

impl TaskType {
    /// 根据任务描述推断类型
    pub fn from_task_description(task: &str) -> Self {
        let t = task.to_lowercase();
        if t.contains("bug") || t.contains("fix") || t.contains("修复") {
            TaskType::BugFix
        } else if t.contains("test") || t.contains("测试") {
            TaskType::WriteTests
        } else if t.contains("init") || t.contains("初始化") || t.contains("新建项目") {
            TaskType::ProjectInit
        } else if t.contains("review") || t.contains("审查") || t.contains("看代码") {
            TaskType::CodeReview
        } else if t.contains("feature") || t.contains("功能") || t.contains("添加") {
            TaskType::AddFeature
        } else if t.contains("doc") || t.contains("文档") || t.contains("注释") {
            TaskType::Docs
        } else if t.contains("research") || t.contains("调研") || t.contains("分析") || t.contains("技术栈") {
            TaskType::Research
        } else if t.contains("refactor") || t.contains("重构") || t.contains("优化") {
            TaskType::CodeCompletion
        } else {
            TaskType::General
        }
    }
}

impl Default for TaskType {
    fn default() -> Self { TaskType::General }
}

/// 单次任务执行后的完整反馈信号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeSignal {
    /// 任务类型
    pub task_type: TaskType,
    /// 任务描述（原始）
    pub task_description: String,
    /// 最终选择的模型
    pub model: String,
    /// 执行是否成功
    pub success: bool,
    /// 失败原因（如果有）
    pub failure_reason: Option<String>,
    /// Token 用量
    pub input_tokens: usize,
    pub output_tokens: usize,
    /// 耗时（毫秒）
    pub latency_ms: u64,
    /// 工具调用次数
    pub tool_calls: usize,
    /// 重试次数
    pub retries: u32,
    /// 任务复杂度（低/中/高）
    pub complexity: ComplexityLevel,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComplexityLevel {
    Low,
    Medium,
    High,
}

impl Default for ComplexityLevel {
    fn default() -> Self { ComplexityLevel::Medium }
}

impl ComplexityLevel {
    /// 根据 token 用量 + 工具调用次数推断复杂度
    pub fn from_stats(tokens: usize, tools: usize) -> Self {
        if tokens > 8000 || tools > 10 {
            ComplexityLevel::High
        } else if tokens > 3000 || tools > 4 {
            ComplexityLevel::Medium
        } else {
            ComplexityLevel::Low
        }
    }
}

/// Outcome 记录器 — 将信号存入 SQLite 供后续学习
pub struct OutcomeRecorder {
    db_path: std::path::PathBuf,
}

impl OutcomeRecorder {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        Self { db_path }
    }

    /// 初始化数据库表
    pub fn init(&self) -> Result<(), crate::error::Error> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS outcomes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_type TEXT NOT NULL,
                task_description TEXT NOT NULL,
                model TEXT NOT NULL,
                success INTEGER NOT NULL,
                failure_reason TEXT,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                latency_ms INTEGER NOT NULL,
                tool_calls INTEGER NOT NULL,
                retries INTEGER NOT NULL,
                complexity TEXT NOT NULL,
                timestamp TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_type ON outcomes(task_type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_model ON outcomes(model)",
            [],
        )?;
        Ok(())
    }

    /// 记录一次任务结果
    pub async fn record(&self, signal: &OutcomeSignal) -> Result<(), crate::error::Error> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO outcomes (task_type, task_description, model, success, failure_reason,
                input_tokens, output_tokens, latency_ms, tool_calls, retries, complexity, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                serde_json::to_string(&signal.task_type).unwrap(),
                signal.task_description,
                signal.model,
                signal.success as i32,
                signal.failure_reason,
                signal.input_tokens,
                signal.output_tokens,
                signal.latency_ms,
                signal.tool_calls,
                signal.retries,
                serde_json::to_string(&signal.complexity).unwrap(),
                signal.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 查询某任务类型在某时间段内的历史成功率
    pub async fn success_rate(
        &self,
        task_type: TaskType,
        limit: usize,
    ) -> Result<f64, crate::error::Error> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let type_str = serde_json::to_string(&task_type).unwrap();
        let total: f64 = conn.query_row(
            "SELECT COUNT(*) FROM outcomes WHERE task_type = ?1 ORDER BY id DESC LIMIT ?2",
            rusqlite::params![type_str, limit],
            |row| row.get::<_, i32>(0),
        )? as f64;

        if total == 0.0 {
            return Ok(0.0);
        }

        let successes: f64 = conn.query_row(
            "SELECT COUNT(*) FROM outcomes WHERE task_type = ?1 AND success = 1 ORDER BY id DESC LIMIT ?2",
            rusqlite::params![type_str, limit],
            |row| row.get::<_, i32>(0),
        )? as f64;

        Ok(successes / total)
    }

    /// 获取某任务类型的模型历史表现
    pub async fn model_stats(
        &self,
        task_type: TaskType,
        model: &str,
    ) -> Result<ModelStats, crate::error::Error> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let type_str = serde_json::to_string(&task_type).unwrap();

        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM outcomes WHERE task_type = ?1 AND model = ?2",
            rusqlite::params![type_str, model],
            |row| row.get(0),
        )?;

        if count == 0 {
            return Ok(ModelStats::default());
        }

        let successes: i32 = conn.query_row(
            "SELECT COUNT(*) FROM outcomes WHERE task_type = ?1 AND model = ?2 AND success = 1",
            rusqlite::params![type_str, model],
            |row| row.get(0),
        )?;

        let avg_latency: f64 = conn.query_row(
            "SELECT AVG(latency_ms) FROM outcomes WHERE task_type = ?1 AND model = ?2",
            rusqlite::params![type_str, model],
            |row| row.get::<_, f64>(0),
        ).unwrap_or(0.0);

        let avg_tokens: f64 = conn.query_row(
            "SELECT AVG(input_tokens + output_tokens) FROM outcomes WHERE task_type = ?1 AND model = ?2",
            rusqlite::params![type_str, model],
            |row| row.get::<_, f64>(0),
        ).unwrap_or(0.0);

        Ok(ModelStats {
            attempts: count,
            successes,
            success_rate: successes as f64 / count as f64,
            avg_latency_ms: avg_latency,
            avg_tokens,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelStats {
    pub attempts: i32,
    pub successes: i32,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub avg_tokens: f64,
}
