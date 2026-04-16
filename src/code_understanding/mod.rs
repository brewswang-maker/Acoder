//! 代码理解层
//!
//! 核心能力：
//! - AST 解析：parser 模块
//! - 代码图谱：graph 模块（调用链、依赖图、爆炸半径）
//! - GitNexus 集成：gitnexus 模块（MCP 协议、22 工具、Hooks）
//! - Graphify 集成：graphify 模块（双通道提取、三级置信度、Leiden 社区）
//!
//! 统一知识图谱：整合三个知识图谱工具

pub mod parser;
pub mod graph;
pub mod gitnexus;
pub mod graphify;
pub mod knowledge_graph;

pub use graph::{CodeGraph, BlastRadius, RiskLevel};
pub use gitnexus::{GitNexus, GitNexusConfig, McpTool};
pub use graphify::{Graphify, Confidence, GraphifyGraph, GraphNode, GraphStats, ImpactResult};
pub use knowledge_graph::{KnowledgeGraph, KnowledgeGraphStats};

// Re-export Analyzer for binary compatibility
pub use graph::Analyzer;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── 统一知识图谱 ─────────────────────────────────────────────

/// 统一查询节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryNode {
    pub id: String,
    pub name: String,
    pub file: String,
    pub line_range: Option<String>,
    pub confidence: f64,
    pub source: GraphSource,
    pub relation: Option<String>,
}

/// 图谱来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphSource {
    GitNexus,
    Graphify,
    CodeReviewGraph,
    Unified,
}

/// 统一风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnifiedRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// 变更类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
}

/// 文件风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// 受影响文件详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedFileDetail {
    pub path: String,
    pub risk_level: FileRiskLevel,
    pub change_type: ChangeType,
}

/// 统一影响面分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedImpactResult {
    pub changed_files: Vec<String>,
    pub affected_files: Vec<String>,
    pub affected_details: Vec<AffectedFileDetail>,
    pub call_chain: Vec<String>,
    pub risk_level: UnifiedRiskLevel,
    pub token_estimate: usize,
}
