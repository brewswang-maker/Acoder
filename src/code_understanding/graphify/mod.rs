//! Graphify 双通道提取器 — 代码知识图谱
//!
//! 双通道设计：
//! - 通道 A：AST 提取（零 Token 消耗，树结构静态分析）
//! - 通道 B：语义提取（LLM 按需，标注 INFERRED/AMBIGUOUS）
//!
//! 三级置信度：EXTRACTED（1.0）/ INFERRED（0.7）/ AMBIGUOUS（0.3）
//!
//! 参考 Graphify 3.2k+ Star：
//! Leiden 算法社区检测 + 反馈回路

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 置信度等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// AST 静态提取，置信度 1.0
    Extracted = 1,
    /// LLM 语义推断，置信度 0.7
    Inferred = 2,
    /// 需要人工确认，置信度 0.3
    Ambiguous = 3,
}

impl Confidence {
    pub fn score(&self) -> f64 {
        match self {
            Self::Extracted => 1.0,
            Self::Inferred => 0.7,
            Self::Ambiguous => 0.3,
        }
    }

    pub fn meets_threshold(&self, min: f64) -> bool {
        self.score() >= min
    }
}

/// Graphify 节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    pub file: String,
    pub span: Option<Span>,
    /// 置信度
    pub confidence: Confidence,
    /// 依赖节点 IDs
    pub dependencies: Vec<String>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// 节点种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Function,
    Struct,
    Enum,
    Trait,
    Module,
    File,
    Import,
    Variable,
    Type,
    Unknown,
}

/// 代码位置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Span {
    pub start_line: u32,
    pub end_line: u32,
}

/// Graphify 图谱
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphifyGraph {
    pub nodes: HashMap<String, GraphNode>,
    pub communities: Vec<Community>,
    /// 提取元数据
    pub extraction_meta: ExtractionMeta,
}

impl Default for GraphifyGraph {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            communities: Vec::new(),
            extraction_meta: ExtractionMeta::default(),
        }
    }
}

/// 社区（Leiden 算法聚类结果）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id: u32,
    pub name: String,
    pub nodes: Vec<String>,
    pub description: String,
}

/// 提取元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionMeta {
    pub files_scanned: usize,
    pub ast_nodes: usize,
    pub semantic_nodes: usize,
    pub ast_tokens_used: usize,
    pub semantic_tokens_used: usize,
    pub extraction_time_ms: u64,
}

impl Default for ExtractionMeta {
    fn default() -> Self {
        Self {
            files_scanned: 0,
            ast_nodes: 0,
            semantic_nodes: 0,
            ast_tokens_used: 0,
            semantic_tokens_used: 0,
            extraction_time_ms: 0,
        }
    }
}

/// Graphify 提取器
pub struct Graphify {
    project_root: PathBuf,
    /// AST 解析器
    ast_parser: crate::code_understanding::parser::Parser,
    /// 图谱（内存中）
    graph: RwLock<GraphifyGraph>,
    /// 最小置信度阈值
    min_confidence: f64,
}

impl Graphify {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let pr = project_root.into();
        Self {
            project_root: pr.clone(),
            ast_parser: crate::code_understanding::parser::Parser::new(&pr),
            graph: RwLock::new(GraphifyGraph::default()),
            min_confidence: 0.3,
        }
    }

    /// 双通道并行提取
    ///
    /// 通道 A：AST 静态解析（零 Token，置信度 1.0）
    /// 通道 B：LLM 语义分析（按需，置信度 0.7）
    pub async fn extract(&self) -> Result<GraphifyGraph, GraphifyError> {
        let start = std::time::Instant::now();

        // 1. 扫描源文件
        let files = self.scan_source_files().await?;
        let mut ast_nodes = Vec::new();
        let mut total_tokens_ast = 0usize;

        // 2. 通道 A：AST 提取（并行）
        for file in &files {
            if let Ok(parse_result) = self.ast_parser.parse_file(file).await {
                for func in &parse_result.functions {
                    total_tokens_ast += 10; // 估算
                    ast_nodes.push(GraphNode {
                        id: format!("{}:{}", file.display(), func.name),
                        kind: NodeKind::Function,
                        name: func.name.clone(),
                        file: file.display().to_string(),
                        span: Some(Span {
                            start_line: func.span.start_line as u32,
                            end_line: func.span.end_line as u32,
                        }),
                        confidence: Confidence::Extracted,
                        dependencies: Vec::new(),
                        metadata: HashMap::new(),
                    });
                }
                for st in &parse_result.structs {
                    ast_nodes.push(GraphNode {
                        id: format!("{}:{}", file.display(), st.name),
                        kind: NodeKind::Struct,
                        name: st.name.clone(),
                        file: file.display().to_string(),
                        span: Some(Span {
                            start_line: st.span.start_line as u32,
                            end_line: st.span.end_line as u32,
                        }),
                        confidence: Confidence::Extracted,
                        dependencies: parse_result.imports.iter().map(|i| i.path.clone()).collect(),
                        metadata: HashMap::new(),
                    });
                }
                for en in &parse_result.enums {
                    ast_nodes.push(GraphNode {
                        id: format!("{}:{}", file.display(), en.name),
                        kind: NodeKind::Enum,
                        name: en.name.clone(),
                        file: file.display().to_string(),
                        span: Some(Span {
                            start_line: en.span.start_line as u32,
                            end_line: en.span.end_line as u32,
                        }),
                        confidence: Confidence::Extracted,
                        dependencies: Vec::new(),
                        metadata: HashMap::new(),
                    });
                }
            }
        }

        // 3. 通道 B：语义提取（当前简化，标记为 Inferred）
        // TODO: 调用 LLM 进行语义分析

        // 4. 构建图谱
        let mut nodes = HashMap::new();
        for node in ast_nodes {
            nodes.insert(node.id.clone(), node);
        }

        // 5. Leiden 社区检测（简化版）
        let communities = self.detect_communities(&nodes);

        let duration_ms = start.elapsed().as_millis() as u64;
        let total_nodes = nodes.len();

        let graph = GraphifyGraph {
            nodes,
            communities,
            extraction_meta: ExtractionMeta {
                files_scanned: files.len(),
                ast_nodes: total_nodes,
                semantic_nodes: 0,
                ast_tokens_used: total_tokens_ast,
                semantic_tokens_used: 0,
                extraction_time_ms: duration_ms,
            },
        };

        *self.graph.write().await = graph.clone();
        Ok(graph)
    }

    /// 简化版 Leiden 社区检测
    fn detect_communities(&self, nodes: &HashMap<String, GraphNode>) -> Vec<Community> {
        // 简化：按文件分组作为社区
        let mut file_groups: HashMap<String, Vec<String>> = HashMap::new();
        for (id, node) in nodes {
            file_groups.entry(node.file.clone()).or_default().push(id.clone());
        }

        let mut communities = Vec::new();
        let mut community_id = 0u32;

        for (file, node_ids) in file_groups {
            let kind = nodes.get(&node_ids[0]).map(|n| format!("{:?}", n.kind)).unwrap_or_default();
            communities.push(Community {
                id: community_id,
                name: format!("{} ({})", file, kind),
                nodes: node_ids,
                description: format!("文件 {} 中的代码实体", file),
            });
            community_id += 1;
        }

        communities
    }

    /// 查询图谱
    ///
    /// 返回与查询相关且满足置信度阈值的节点
    pub async fn query(&self, keyword: &str, min_confidence: f64) -> Vec<GraphNode> {
        let graph = self.graph.read().await;
        let keyword_lower = keyword.to_lowercase();

        graph.nodes.values()
            .filter(|node| {
                node.confidence.meets_threshold(min_confidence)
                    && (node.name.to_lowercase().contains(&keyword_lower)
                        || node.file.to_lowercase().contains(&keyword_lower))
            })
            .cloned()
            .collect()
    }

    /// 影响面分析
    pub async fn trace_impact(&self, target: &str) -> ImpactResult {
        let graph = self.graph.read().await;
        let mut affected = HashSet::new();
        let mut call_chain = Vec::new();

        // 找出目标节点
        for (id, node) in &graph.nodes {
            if id.contains(target) || node.name.contains(target) {
                // 找出所有依赖此节点的节点（被调用者）
                for (other_id, other) in &graph.nodes {
                    if other.dependencies.iter().any(|dep| id.contains(dep)) {
                        affected.insert(other_id.clone());
                        call_chain.push(ImpactEntry {
                            from: other_id.clone(),
                            to: id.clone(),
                            relation: "calls".to_string(),
                        });
                    }
                }
            }
        }

        ImpactResult {
            target: target.to_string(),
            affected_nodes: affected.into_iter().collect(),
            call_chain,
        }
    }

    /// 反馈回路：Q&A 结果写入图谱
    ///
    /// 用户问答自动沉淀为图谱节点，下次查询时可用
    pub async fn incorporate_feedback(&self, query: &str, result: &str, nodes: &[String]) {
        let mut graph = self.graph.write().await;

        for node_id in nodes {
            if let Some(node) = graph.nodes.get_mut(node_id) {
                // 更新元数据，标记为 Inferred（来自反馈）
                node.confidence = Confidence::Inferred;
                node.metadata.insert(
                    format!("feedback_{}", chrono::Utc::now().timestamp()),
                    format!("Q: {} | A: {}", query, result),
                );
            }
        }
    }

    /// 获取图谱统计
    pub async fn stats(&self) -> GraphStats {
        let graph = self.graph.read().await;
        GraphStats {
            total_nodes: graph.nodes.len(),
            by_kind: {
                let mut m = HashMap::new();
                for node in graph.nodes.values() {
                    *m.entry(format!("{:?}", node.kind)).or_insert(0) += 1;
                }
                m
            },
            by_confidence: {
                let mut m = HashMap::new();
                for node in graph.nodes.values() {
                    *m.entry(format!("{}", node.confidence)).or_insert(0) += 1;
                }
                m
            },
            communities: graph.communities.len(),
        }
    }

    /// 扫描源文件（同步递归，避免 async 递归）
    async fn scan_source_files(&self) -> Result<Vec<PathBuf>, GraphifyError> {
        let mut files = Vec::new();
        Self::walk_dir(&self.project_root, &mut files, 0);
        Ok(files)
    }

    /// 同步递归扫描（std::fs）
    fn walk_dir(dir: &PathBuf, files: &mut Vec<PathBuf>, depth: usize) {
        if depth > 6 { return; }
        use std::fs;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name == "target" || name == "node_modules" { continue; }
                }
                if path.is_dir() {
                    Self::walk_dir(&path, files, depth + 1);
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext, "rs"|"ts"|"js"|"py"|"go"|"java") {
                        files.push(path);
                    }
                }
            }
        }
    }
}

/// 影响面分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactResult {
    pub target: String,
    pub affected_nodes: Vec<String>,
    pub call_chain: Vec<ImpactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactEntry {
    pub from: String,
    pub to: String,
    pub relation: String,
}

/// 图谱统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub by_kind: HashMap<String, usize>,
    pub by_confidence: HashMap<String, usize>,
    pub communities: usize,
}

/// Graphify 错误
#[derive(Debug)]
pub enum GraphifyError {
    IoError(String),
    ParseError(String),
    LlmError(String),
}
