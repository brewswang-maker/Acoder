//! 多 Agent 协作规划器
//!
//! Three-Explorer 并行探索策略 + Commander 统一调度：
//! - Architecture Explorer：理解代码结构和技术栈
//! - Change Explorer：分析需要改动的范围
//! - Risk Explorer：识别潜在问题和危险操作
//!
//! 参考 agency-orchestrator 的 147 角色 + L2 规划决策

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use crate::llm::{Client as LlmClient, Message, LlmRequest};
use crate::error::Result;

/// Explorer 角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum ExplorerRole {
    /// 架构视角：理解代码结构
    Architecture,
    /// 变更视角：分析改动范围
    Change,
    /// 风险视角：识别潜在问题
    Risk,
}

/// Explorer 分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerResult {
    pub role: ExplorerRole,
    pub findings: Vec<Finding>,
    pub confidence: f64,
    pub duration_ms: u64,
}

/// 发现项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub category: String,
    pub description: String,
    pub severity: Severity,
    pub location: Option<String>,
    pub suggestion: Option<String>,
}

/// 严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// 多 Agent 规划器
pub struct MultiAgentPlanner {
    llm: Arc<LlmClient>,
}

impl MultiAgentPlanner {
    pub fn new(llm: Arc<LlmClient>) -> Self {
        Self { llm }
    }

    /// 并行执行三个 Explorer（真并发）
    pub async fn explore(&self, task: &str, context: &str) -> Result<Vec<ExplorerResult>> {
        let mut set = JoinSet::new();

        // 启动三个并行任务
        for role in [ExplorerRole::Architecture, ExplorerRole::Change, ExplorerRole::Risk] {
            let llm = self.llm.clone();
            let task = task.to_string();
            let context = context.to_string();

            set.spawn(async move {
                let start = std::time::Instant::now();
                let result = Self::analyze_role(llm.as_ref(), role, &task, &context).await;
                let duration_ms = start.elapsed().as_millis() as u64;

                match result {
                    Ok((findings, confidence)) => ExplorerResult {
                        role,
                        findings,
                        confidence,
                        duration_ms,
                    },
                    Err(e) => ExplorerResult {
                        role,
                        findings: vec![Finding {
                            category: "分析失败".to_string(),
                            description: e.to_string(),
                            severity: Severity::Warning,
                            location: None,
                            suggestion: None,
                        }],
                        confidence: 0.0,
                        duration_ms,
                    },
                }
            });
        }

        // 收集结果
        let mut results = Vec::new();
        while let Some(res) = set.join_next().await {
            if let Ok(r) = res {
                results.push(r);
            }
        }

        Ok(results)
    }

    /// 单个角色的分析（调用 LLM）
    async fn analyze_role(
        llm: &LlmClient,
        role: ExplorerRole,
        task: &str,
        context: &str,
    ) -> Result<(Vec<Finding>, f64)> {
        let prompt = Self::build_prompt(role, task, context);

        let request = LlmRequest {
            model: "auto".into(),
            messages: vec![Message::user(&prompt)],
            temperature: Some(0.2),
            max_tokens: Some(1200),
            stream: false,
            tools: None,
        };

        let response = llm.complete(request).await?;
        let content = response.content;

        // 解析 LLM 返回的 JSON 格式发现项
        let (findings, confidence) = Self::parse_findings(&content, role);

        Ok((findings, confidence))
    }

    fn build_prompt(role: ExplorerRole, task: &str, context: &str) -> String {
        match role {
            ExplorerRole::Architecture => format!(
                r#"你是架构分析师。请从架构视角分析以下任务。

任务: {task}

项目上下文:
{context}

请分析：
1. 涉及哪些模块/组件（列出具体文件路径）
2. 技术栈依赖（框架、库版本）
3. 接口变化（API/数据结构）
4. 数据流影响（上下游依赖）

返回 JSON 数组格式：
[
  {{"category": "模块", "description": "...", "severity": "info|warning|error", "location": "src/...", "suggestion": "..."}}
]

最后给出置信度（0-1）：
Confidence: 0.XX"#
            ),
            ExplorerRole::Change => format!(
                r#"你是变更分析师。请从变更视角分析以下任务。

任务: {task}

项目上下文:
{context}

请分析：
1. 需要修改哪些文件（具体路径）
2. 改动范围有多大（行数估计）
3. 是否有依赖文件需要同步修改
4. 需要的测试覆盖

返回 JSON 数组格式：
[
  {{"category": "文件", "description": "...", "severity": "info|warning|error", "location": "path/to/file", "suggestion": "..."}}
]

最后给出置信度（0-1）：
Confidence: 0.XX"#
            ),
            ExplorerRole::Risk => format!(
                r#"你是风险分析师。请从风险视角分析以下任务。

任务: {task}

项目上下文:
{context}

请分析：
1. 潜在的危险操作（删除/迁移/权限变更）
2. 可能导致破坏的功能（向后兼容性）
3. 安全风险（注入、泄露、越权）
4. 回滚难度评估

返回 JSON 数组格式：
[
  {{"category": "风险", "description": "...", "severity": "info|warning|error", "location": "...", "suggestion": "..."}}
]

最后给出置信度（0-1）：
Confidence: 0.XX"#
            ),
        }
    }

    /// 解析 LLM 返回内容，提取发现项
    fn parse_findings(content: &str, role: ExplorerRole) -> (Vec<Finding>, f64) {
        let mut findings = Vec::new();
        let mut confidence = 0.7; // 默认置信度

        // 尝试解析 JSON 数组
        if let Some(json_start) = content.find('[') {
            if let Some(json_end) = content.rfind(']') {
                let json_str = &content[json_start..=json_end];
                if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) {
                    for item in items {
                        if let Some(obj) = item.as_object() {
                            let finding = Finding {
                                category: obj.get("category")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("未知")
                                    .to_string(),
                                description: obj.get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                severity: obj.get("severity")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| match s {
                                        "error" => Some(Severity::Error),
                                        "warning" => Some(Severity::Warning),
                                        _ => Some(Severity::Info),
                                    })
                                    .unwrap_or(Severity::Info),
                                location: obj.get("location").and_then(|v| v.as_str()).map(String::from),
                                suggestion: obj.get("suggestion").and_then(|v| v.as_str()).map(String::from),
                            };
                            findings.push(finding);
                        }
                    }
                }
            }
        }

        // 提取置信度
        if let Some(conf_line) = content.lines().find(|l| l.to_lowercase().contains("confidence")) {
            if let Some(num) = conf_line.split(':').last() {
                if let Ok(c) = num.trim().parse::<f64>() {
                    confidence = c.clamp(0.0, 1.0);
                }
            }
        }

        // 如果没有解析到任何发现，生成一个默认发现
        if findings.is_empty() {
            findings.push(Finding {
                category: format!("{:?}分析", role),
                description: content.lines().next().unwrap_or("分析完成").to_string(),
                severity: Severity::Info,
                location: None,
                suggestion: None,
            });
        }

        (findings, confidence)
    }

    /// 综合三个视角的发现，生成最终分析
    pub fn synthesize(&self, results: &[ExplorerResult]) -> Synthesis {
        let mut all_findings = Vec::new();
        let mut high_risk_count = 0;
        let mut total_confidence = 0.0;

        for result in results {
            for finding in &result.findings {
                all_findings.push(finding.clone());
                if matches!(finding.severity, Severity::Error) {
                    high_risk_count += 1;
                }
            }
            total_confidence += result.confidence;
        }

        let avg_confidence = if !results.is_empty() {
            total_confidence / results.len() as f64
        } else {
            0.5
        };

        let risk_level = if high_risk_count > 3 || avg_confidence < 0.5 {
            SynthesisRiskLevel::High
        } else if high_risk_count > 0 || avg_confidence < 0.7 {
            SynthesisRiskLevel::Medium
        } else {
            SynthesisRiskLevel::Low
        };

        let recommendations = self.generate_recommendations(results, risk_level);

        Synthesis {
            findings: all_findings,
            risk_level,
            avg_confidence,
            recommendations,
        }
    }

    fn generate_recommendations(&self, results: &[ExplorerResult], risk: SynthesisRiskLevel) -> Vec<String> {
        let mut recommendations = Vec::new();

        // 基于角色类型生成建议
        for result in results {
            match result.role {
                ExplorerRole::Architecture => {
                    if result.findings.len() > 3 {
                        recommendations.push("模块涉及较多，建议拆分子任务".to_string());
                    }
                }
                ExplorerRole::Change => {
                    let has_files = result.findings.iter().any(|f| f.category.contains("文件"));
                    if has_files {
                        recommendations.push("变更涉及文件操作，建议先备份".to_string());
                    }
                }
                ExplorerRole::Risk => {
                    if result.findings.iter().any(|f| matches!(f.severity, Severity::Error)) {
                        recommendations.push("存在高风险操作，需要人工确认".to_string());
                    }
                }
            }
        }

        // 基于风险等级生成建议
        match risk {
            SynthesisRiskLevel::High => {
                recommendations.insert(0, "⚠️ 高风险任务，建议分阶段执行并设置检查点".to_string());
            }
            SynthesisRiskLevel::Medium => {
                recommendations.insert(0, "⚡ 中等风险，建议并行执行独立步骤".to_string());
            }
            SynthesisRiskLevel::Low => {
                recommendations.insert(0, "✓ 低风险，可直接执行".to_string());
            }
        }

        recommendations
    }
}

/// 综合分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Synthesis {
    pub findings: Vec<Finding>,
    pub risk_level: SynthesisRiskLevel,
    pub avg_confidence: f64,
    pub recommendations: Vec<String>,
}

/// 综合风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SynthesisRiskLevel {
    Low,
    Medium,
    High,
}
