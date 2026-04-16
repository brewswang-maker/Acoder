//! 统一知识图谱 — 整合 GitNexus / Graphify / CodeReviewGraph

use std::collections::HashMap;
use std::path::PathBuf;

use crate::code_understanding::graphify::{Graphify, GraphStats, ImpactResult, ImpactEntry, Confidence};
use crate::code_understanding::gitnexus::{GitNexus, GitNexusConfig};
use crate::code_understanding::{
    QueryNode, GraphSource, UnifiedRiskLevel, UnifiedImpactResult,
    AffectedFileDetail, ChangeType, FileRiskLevel,
};
use crate::error::Result;

/// 统一知识图谱
pub struct KnowledgeGraph {
    graphify: Graphify,
    gitnexus: GitNexus,
    project_root: PathBuf,
}

impl KnowledgeGraph {
    /// 新建知识图谱
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            graphify: Graphify::new(&project_root),
            gitnexus: GitNexus::new(GitNexusConfig::default()),
            project_root,
        }
    }

    /// 构建索引
    pub async fn build_index(&self) -> Result<()> {
        self.graphify.extract().await
            .map_err(|e| crate::Error::ContextLoadFailed(format!("{:?}", e)))?;
        self.gitnexus.analyze().await
            .map_err(|e| crate::Error::ContextLoadFailed(format!("{:?}", e)))?;
        Ok(())
    }

    /// 统一查询
    pub async fn query(&self, keyword: &str, min_confidence: f64) -> Vec<QueryNode> {
        let mut results: Vec<QueryNode> = Vec::new();

        let nodes = self.graphify.query(keyword, min_confidence).await;
        for node in nodes {
            let line_range = node.span.as_ref().map(|s| format!("{}:{}", s.start_line, s.end_line));
            results.push(QueryNode {
                id: node.id.clone(),
                name: node.name.clone(),
                file: node.file.clone(),
                line_range,
                confidence: node.confidence.score(),
                source: GraphSource::Graphify,
                relation: None,
            });
        }

        let gn_results = self.gitnexus.query_symbols(keyword).await;
        results.extend(gn_results);

        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// 统一影响面分析
    pub async fn trace_impact(&self, changed_files: &[String]) -> UnifiedImpactResult {
        let mut affected_files: Vec<String> = Vec::new();
        let mut call_chain: Vec<String> = Vec::new();
        let mut risk_level = UnifiedRiskLevel::Low;

        for file in changed_files {
            let result = self.graphify.trace_impact(file).await;
            affected_files.extend(result.affected_nodes);
            for entry in result.call_chain {
                call_chain.push(format!("{} → {}", entry.from, entry.to));
            }
        }

        let count = affected_files.len();
        if count > 10 { risk_level = UnifiedRiskLevel::High; }
        else if count > 3 { risk_level = UnifiedRiskLevel::Medium; }

        let unique_affected: Vec<String> = {
            let mut seen = HashMap::new();
            for n in affected_files {
                seen.entry(n.clone()).or_insert(n);
            }
            seen.into_values().collect()
        };

        let affected_details: Vec<AffectedFileDetail> = unique_affected.iter()
            .map(|path| AffectedFileDetail {
                path: path.clone(),
                risk_level: FileRiskLevel::Medium,
                change_type: ChangeType::Modified,
            })
            .collect();

        UnifiedImpactResult {
            changed_files: changed_files.to_vec(),
            affected_files: unique_affected.clone(),
            affected_details,
            call_chain,
            risk_level,
            token_estimate: unique_affected.len() * 200 + changed_files.len() * 50,
        }
    }

    /// Pre-ToolUse Hook
    pub fn pre_tool_use_context(&self, tool_name: &str, args: &str) -> String {
        self.gitnexus.pre_tool_use_context(tool_name, args)
    }

    /// Post-ToolUse Hook
    pub async fn post_tool_use_update(&self, tool_name: &str, result: &str) -> Result<()> {
        self.graphify.incorporate_feedback(tool_name, result, &[]).await;
        self.gitnexus.post_tool_use_update(tool_name, result).await
    }

    /// 统计信息
    pub async fn stats(&self) -> KnowledgeGraphStats {
        let gf_stats = self.graphify.stats().await;
        let gn_stats = self.gitnexus.stats();
        KnowledgeGraphStats {
            graphify_nodes: gf_stats.total_nodes,
            gitnexus_files: gn_stats.file_count,
        }
    }

    /// 将任务描述注入图谱上下文，返回可追加到 system prompt 的内容片段
    ///
    /// 策略：
    /// 1. 从任务描述中提取关键词（2-3个核心词）
    /// 2. 查询图谱获取相关符号、依赖关系
    /// 3. 构造简洁的上下文摘要（< 800 tokens）
    pub async fn inject_prompt_context(&self, task: &str) -> Option<String> {
        // 提取关键词
        let keywords = extract_keywords(task);
        if keywords.is_empty() {
            return None;
        }

        // 并行查询多个关键词（取前2个）
        let mut all_nodes: Vec<QueryNode> = Vec::new();
        for kw in keywords.iter().take(2) {
            let results = self.query(kw, 0.3).await;
            all_nodes.extend(results);
        }

        // 去重 + 按文件分组
        let mut by_file: HashMap<String, Vec<&QueryNode>> = HashMap::new();
        let mut seen_ids = HashMap::new();
        for node in &all_nodes {
            if seen_ids.contains_key(&node.id) { continue; }
            seen_ids.insert(node.id.clone(), ());
            by_file.entry(node.file.clone())
                .or_default()
                .push(node);
        }

        if by_file.is_empty() {
            return None;
        }

        // 生成上下文片段（按文件组织，限制总量）
        let mut ctx = String::from("## 代码上下文\n\n");
        let mut total_lines = 0;
        let max_lines = 40;

        for (file, nodes) in by_file.iter().take(5) {
            if total_lines >= max_lines { break; }
            ctx.push_str(&format!("### {}\n", file));
            for node in nodes.iter().take(6) {
                let line_info = node.line_range.as_ref()
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();
                ctx.push_str(&format!(
                    "- `{}`{} [conf:{:.0}%] {}\n",
                    node.name,
                    line_info,
                    node.confidence * 100.0,
                    node.source.name()
                ));
            }
            total_lines += nodes.len();
        }

        // 影响面分析（如果任务涉及修改）
        if task.to_lowercase().contains("修改") || task.to_lowercase().contains("change")
            || task.to_lowercase().contains("edit") || task.to_lowercase().contains("重构")
        {
            let relevant_files: Vec<String> = by_file.keys().cloned().take(3).collect();
            if !relevant_files.is_empty() {
                let impact = self.trace_impact(&relevant_files).await;
                if !impact.affected_files.is_empty() {
                    ctx.push_str(&format!(
                        "\n### 影响面分析\n变更文件: {:?}\n影响范围: {} 个文件\n风险等级: {:?}\n",
                        impact.changed_files,
                        impact.affected_files.len(),
                        impact.risk_level
                    ));
                    if !impact.call_chain.is_empty() {
                        ctx.push_str("调用链:\n");
                        for chain in impact.call_chain.iter().take(5) {
                            ctx.push_str(&format!("  {}\n", chain));
                        }
                    }
                }
            }
        }

        Some(ctx)
    }

    /// 获取项目统计摘要
    pub fn summary(&self) -> String {
        format!(
            "项目根: {}",
            self.project_root.display()
        )
    }
}

/// 知识图谱统计
#[derive(Debug, Default)]
pub struct KnowledgeGraphStats {
    pub graphify_nodes: usize,
    pub gitnexus_files: usize,
}

// ── 辅助函数 ─────────────────────────────────────────────────

/// 从任务描述中提取关键词（简单分词 + 停用词过滤）
fn extract_keywords(task: &str) -> Vec<String> {
    let stop_words = [
        "的", "了", "在", "是", "我", "你", "他", "它", "们", "这", "那",
        "一个", "什么", "怎么", "如何", "为什么", "可以", "需要", "应该",
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "can", "to", "of", "in", "for", "on", "with",
        "at", "by", "from", "as", "into", "through", "during", "before", "after",
        "and", "or", "but", "if", "then", "else", "when", "where", "while",
        "实现", "开发", "创建", "添加", "修改", "删除", "修复", "优化",
        "请", "帮我", "给我", "一下", "一个", "写", "做", "完成",
    ];

    let words: Vec<String> = task.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '#')
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_lowercase())
        .filter(|s| !stop_words.contains(&s.as_str()))
        .collect();

    // 取出现最多的词
    let mut freq: HashMap<String, usize> = HashMap::new();
    for w in &words {
        *freq.entry(w.clone()).or_insert(0) += 1;
    }

    let mut sorted: Vec<(String, usize)> = freq.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    sorted.into_iter().take(3).map(|(w, _)| w).collect()
}

impl GraphSource {
    pub fn name(&self) -> &'static str {
        match self {
            GraphSource::GitNexus => "GitNexus",
            GraphSource::Graphify => "Graphify",
            GraphSource::CodeReviewGraph => "CodeReviewGraph",
            GraphSource::Unified => "Unified",
        }
    }
}
