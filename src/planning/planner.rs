//! Planner — 任务规划器
//!
//! 将用户需求拆解为可执行计划，并自动决定 L2 策略：
//! - 并行度：哪些步骤可以并行执行
//! - 子任务拆分：复杂步骤如何拆成更小的可执行单元
//! - 检查点间隔：长任务多久保存一次进度

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// L2 规划决策 — 自动决定并行度、拆分、检查点
/// hermes-agent-rs L2 设计：接收长任务自动决定并行度、子任务拆分和检查点间隔
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2PlanDecision {
    /// 推荐并行度（同时执行的最大步骤数）
    pub parallelism_degree: usize,
    /// 是否启用检查点
    pub checkpoint_enabled: bool,
    /// 检查点间隔（每 N 个步骤保存一次）
    pub checkpoint_interval: usize,
    /// 子任务拆分建议
    pub split_hints: Vec<SplitHint>,
    /// 预估总耗时（秒）
    pub estimated_duration_secs: u64,
    /// 风险等级
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel { Low, Medium, High, Critical }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitHint {
    pub step_id: String,
    pub should_split: bool,
    pub sub_step_count: usize,
    pub reason: String,
}

/// 可执行的计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub task: String,
    pub steps: Vec<PlanStep>,
    pub created_at: DateTime<Utc>,
    pub status: PlanStatus,
    pub affected_files: Vec<String>,
    pub complexity: crate::planning::TaskComplexity,
}

impl Plan {
    pub fn new(task: String) -> Self {
        Self { id: Uuid::new_v4().to_string(), task, steps: Vec::new(),
            created_at: Utc::now(), status: PlanStatus::Draft,
            affected_files: Vec::new(), complexity: crate::planning::TaskComplexity::Simple }
    }
    pub fn add_step(&mut self, step: PlanStep) { self.steps.push(step); }
    pub fn total_steps(&self) -> usize { self.steps.len() }
    pub fn completed_steps(&self) -> usize {
        self.steps.iter().filter(|s| s.status == StepStatus::Done).count()
    }
    pub fn progress(&self) -> f64 {
        if self.steps.is_empty() { return 0.0; }
        self.completed_steps() as f64 / self.total_steps() as f64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    pub details: Option<String>,
    pub executor: Option<String>,
    pub depends_on: Vec<String>,
    pub parallel: bool,
    pub status: StepStatus,
    pub requires_approval: bool,
    pub acceptance_criteria: Vec<String>,
    pub quality_gates: Vec<QualityGate>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
}

impl PlanStep {
    pub fn new(description: impl Into<String>) -> Self {
        Self { id: Uuid::new_v4().to_string(), description: description.into(),
            details: None, executor: None, depends_on: Vec::new(), parallel: false,
            status: StepStatus::Pending, requires_approval: false,
            acceptance_criteria: Vec::new(), quality_gates: Vec::new(),
            started_at: None, completed_at: None, result: None }
    }
    pub fn with_executor(mut self, executor: impl Into<String>) -> Self {
        self.executor = Some(executor.into()); self
    }
    pub fn with_approval(mut self) -> Self { self.requires_approval = true; self }
    pub fn with_gates(mut self, gates: Vec<QualityGate>) -> Self {
        self.quality_gates = gates; self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus { Pending, Running, WaitingApproval, Done, Failed, Skipped }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus { Draft, Approved, Running, Paused, Completed, Failed, Cancelled }

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum QualityGate { Lint, Test, Review, Security, Coverage, None }

/// Planner：生成执行计划
pub struct Planner { llm: crate::llm::Client }

impl Planner {
    pub fn new(llm: crate::llm::Client) -> Self { Self { llm } }

    /// L2 规划决策：自动决定并行度、子任务拆分、检查点间隔
    ///
    /// hermes-agent-rs L2 核心设计：
    /// 接收长任务自动决定并行度、子任务拆分和检查点间隔。
    /// 核心思想：不是所有任务都值得全速并行，也不是所有任务都值得线性执行。
    pub async fn make_l2_decision(&self, plan: &Plan, context: &crate::context::Context) -> L2PlanDecision {
        let step_count = plan.total_steps();
        let has_dependencies = plan.steps.iter().any(|s| !s.depends_on.is_empty());
        let has_parallel_markers = plan.steps.iter().any(|s| s.parallel);
        let has_approval = plan.steps.iter().any(|s| s.requires_approval);
        let affected = plan.affected_files.len();
        let file_count = context.file_count;

        // 并行度决策（启发式）
        let base = if has_dependencies { 2 } else { 3 };
        let parallelism_degree = if has_approval {
            base.min(2)
        } else if has_parallel_markers && !has_dependencies {
            (base + 2).min(6)
        } else if affected > 20 || file_count > 50 {
            (base + 1).min(4)
        } else {
            base
        };

        // 检查点间隔
        let checkpoint_enabled = step_count >= 4;
        let checkpoint_interval = if step_count >= 10 { 3 } else if step_count >= 6 { 2 } else { step_count.max(1) };

        // 子任务拆分决策
        let split_hints: Vec<SplitHint> = plan.steps.iter().map(|step| {
            let should_split = step.acceptance_criteria.len() > 3
                || step.description.len() > 200
                || (step.quality_gates.len() >= 2 && !step.parallel);
            let sub_step_count = if step.acceptance_criteria.len() > 5 {
                (step.acceptance_criteria.len() / 2).min(5)
            } else if step.description.len() > 300 { 3 } else { 0 };
            SplitHint {
                step_id: step.id.clone(),
                should_split,
                sub_step_count,
                reason: if step.acceptance_criteria.len() > 3 {
                    "验收标准过多，建议拆解".into()
                } else if step.description.len() > 200 {
                    "步骤描述过长，建议拆解".into()
                } else { "无需拆分".into() },
            }
        }).collect();

        // 风险评估
        let risk_level = if plan.steps.iter().any(|s| s.quality_gates.contains(&QualityGate::Security)) {
            RiskLevel::High
        } else if has_approval || affected > 10 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        let estimated_duration_secs = (step_count as u64) * match plan.complexity {
            crate::planning::TaskComplexity::Simple => 60,
            crate::planning::TaskComplexity::Moderate => 120,
            crate::planning::TaskComplexity::Complex => 300,
            crate::planning::TaskComplexity::Exploratory => 600,
        };

        tracing::info!(
            "L2 决策: 并行度={}, 检查点={}步/次, 风险={:?}, 拆分={}步",
            parallelism_degree, checkpoint_interval, risk_level,
            split_hints.iter().filter(|h| h.should_split).count()
        );

        L2PlanDecision {
            parallelism_degree,
            checkpoint_enabled,
            checkpoint_interval,
            split_hints,
            estimated_duration_secs,
            risk_level,
        }
    }

    /// 生成计划
    pub async fn plan(&self, task: &str, context: &crate::context::Context) -> Result<Plan, crate::Error> {
        tracing::info!("生成执行计划: {}", task);

        let prompt = format!(r#"
你是一位资深软件工程师，为以下任务生成执行计划：

任务：{}

项目上下文：
- 技术栈：{:?}
- 涉及文件：{:?}
- 代码语言：{:?}
- 项目规模：{} 个文件

请生成详细的执行计划，格式为 JSON：
{{
  "steps": [
    {{
      "description": "步骤描述",
      "details": "具体要做什么",
      "executor": "推荐执行者（frontend/backend/security/qa/architect）",
      "depends_on": ["上一步ID"],
      "parallel": false,
      "requires_approval": false,
      "acceptance_criteria": ["验收标准1", "验收标准2"],
      "quality_gates": ["lint", "test"]
    }}
  ],
  "complexity": "simple/moderate/complex/exploratory",
  "affected_files": ["需要修改的文件路径"],
  "estimated_time": "预估时间"
}}

要求：
- 步骤要具体可执行
- 标注需要人工审批的高危步骤
- 明确每个步骤的验收标准
- 涉及安全/部署/删除的操作必须 requires_approval = true
- 只输出 JSON，不要有其他文字
"#, task, context.tech_stack, context.files, context.languages, context.file_count);

        let messages = vec![
            crate::llm::Message::system("你是一个专业的任务规划助手，输出 JSON 格式的计划。"),
            crate::llm::Message::user(&prompt),
        ];

        let request = crate::llm::LlmRequest {
            model: "auto".into(), messages,
            temperature: Some(0.3), max_tokens: Some(8192),
            stream: false, tools: None,
        };

        let response = self.llm.complete(request).await
            .map_err(|e| crate::Error::PlanningFailed(e.to_string()))?;

        let plan_json: serde_json::Value = serde_json::from_str(&response.content)
            .map_err(|_| crate::Error::PlanningFailed("无法解析 LLM 返回的计划 JSON".into()))?;

        let mut result = Plan::new(task.to_string());

        if let Some(c) = plan_json.get("complexity").and_then(|v| v.as_str()) {
            result.complexity = match c {
                "simple" => crate::planning::TaskComplexity::Simple,
                "moderate" => crate::planning::TaskComplexity::Moderate,
                "complex" => crate::planning::TaskComplexity::Complex,
                "exploratory" => crate::planning::TaskComplexity::Exploratory,
                _ => crate::planning::TaskComplexity::Moderate,
            };
        }

        if let Some(steps) = plan_json.get("steps").and_then(|v| v.as_array()) {
            for step_value in steps {
                let description = step_value.get("description")
                    .and_then(|v| v.as_str()).unwrap_or("未知步骤").to_string();
                let mut step = PlanStep::new(description);

                if let Some(d) = step_value.get("details").and_then(|v| v.as_str()) {
                    step.details = Some(d.to_string());
                }
                if let Some(e) = step_value.get("executor").and_then(|v| v.as_str()) {
                    step.executor = Some(e.to_string());
                }
                if let Some(p) = step_value.get("parallel").and_then(|v| v.as_bool()) {
                    step.parallel = p;
                }
                if let Some(a) = step_value.get("requires_approval").and_then(|v| v.as_bool()) {
                    step.requires_approval = a;
                }
                if let Some(criteria) = step_value.get("acceptance_criteria").and_then(|v| v.as_array()) {
                    step.acceptance_criteria = criteria.iter()
                        .filter_map(|v| v.as_str().map(String::from)).collect();
                }
                if let Some(gates) = step_value.get("quality_gates").and_then(|v| v.as_array()) {
                    step.quality_gates = gates.iter()
                        .filter_map(|v| v.as_str().and_then(|g| match g {
                            "lint" => Some(QualityGate::Lint),
                            "test" => Some(QualityGate::Test),
                            "review" => Some(QualityGate::Review),
                            "security" => Some(QualityGate::Security),
                            "coverage" => Some(QualityGate::Coverage),
                            _ => None,
                        })).collect();
                }

                result.add_step(step);
            }
        }

        if let Some(files) = plan_json.get("affected_files").and_then(|v| v.as_array()) {
            result.affected_files = files.iter()
                .filter_map(|v| v.as_str().map(String::from)).collect();
        }

        result.status = PlanStatus::Approved;
        tracing::info!("计划生成完成: {} 个步骤", result.total_steps());

        Ok(result)
    }
}
