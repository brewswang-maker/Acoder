//! UI 模块 — 会话可视化、检查点管理与 Diff 审查
//!
//! 核心组件：
//! - `session_viewer`: 会话时序图、检查点 Resume、时间线回溯
//! - `checkpoint`: 检查点管理（保存/恢复/列表/自动保存）
//! - `diff_viewer`: Diff 可视化审查器

pub mod checkpoint;
pub mod diff_viewer;
pub mod session_viewer;

// Re-export core types
pub use session_viewer::{
    SessionVisualizer, SessionTimeline, AgentTrack, ExecutionSpan,
    SpanAction, SpanStatus, ToolCallRef, TokenUsage, Decision, DecisionType,
    Checkpoint as SessionCheckpoint, ContextSnapshot, ConversationTurn, AgentState, MemoryState,
    TimelineStatus, ResumeContext,
};
pub use checkpoint::{CheckpointManager, CheckpointSummary};
pub use diff_viewer::{
    ChangedFile, DiffHunk, DiffLine, DiffViewer, FileStats, HunkStatus, LineDecision,
    ReviewCommand, ReviewProgress, DiffReviewSession, ReviewStatus,
};
