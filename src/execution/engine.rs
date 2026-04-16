//! 执行引擎主文件
//!
//! 整合规划、上下文、工具调用、LLM 的核心执行循环
//!
//! hermes-agent-rs 能力落地：
//! - L2 规划决策自动决定并行度、检查点、子任务拆分
//! - JoinSet 真并发执行独立步骤（30s 浏览器抓取不阻塞 50ms 文件读取）

use std::path::PathBuf;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use tokio::task::JoinSet;

use crate::config::Config;
use crate::llm::{Client as LlmClient, Message, LlmRequest};
use crate::context::Context;
use crate::planning::{Planner, TaskComplexity, L2PlanDecision, StepStatus as PlanStepStatus};
use crate::execution::tool_registry::ToolRegistry;
use crate::execution::sandbox::Sandbox;
use crate::error::{Error, Result};
use crate::agents::commander::Commander;
use crate::observability::Metrics;
use crate::code_understanding::KnowledgeGraph;

/// ── 执行结果 ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub task: String,
    pub status: ExecutionStatus,
    pub summary: String,
    pub artifacts: Vec<Artifact>,
    pub warnings: Option<Vec<String>>,
    pub suggestions: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub tokens_used: usize,
    pub steps_executed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Success,
    Failed,
    PartiallyCompleted,
    Cancelled,
    Timeout,
}

impl ExecutionResult {
    pub fn duration_secs(&self) -> f64 {
        (self.ended_at - self.started_at).num_milliseconds() as f64 / 1000.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub path: String,
    pub description: String,
    pub content_preview: Option<String>,
    pub verified: bool,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File, Directory, Test, Documentation, Config, Diff, Summary,
}

/// ── 主执行引擎 ───────────────────────────────────────────────

#[derive(Default)]
pub struct Engine;

impl Engine {
    pub async fn new(_config: Config, _workdir: PathBuf) -> Result<Self> {
        Ok(Self)
    }
}

pub struct EngineInstance {
    config: Arc<Config>,
    workdir: PathBuf,
    llm: LlmClient,
    context: Context,
    planner: Planner,
    tools: ToolRegistry,
    sandbox: Sandbox,
    commander: Commander,
    metrics: Metrics,
    /// 知识图谱（用于上下文注入）
    knowledge_graph: Option<KnowledgeGraph>,
}

impl EngineInstance {
    pub async fn new(config: Config, workdir: PathBuf) -> Result<Self> {
        let config = Arc::new(config);

        if !workdir.exists() {
            return Err(Error::ProjectNotFound { path: workdir });
        }

        let llm = LlmClient::new(Arc::clone(&config).llm.clone());
        let context = Context::load(&workdir).await?;
        let planner = Planner::new(llm.clone());

        // 初始化知识图谱（用于上下文注入 + Code-Review-Graph 工具）
        let knowledge_graph = Some(KnowledgeGraph::new(workdir.clone()));
        let tools = if let Some(ref kg) = knowledge_graph {
            ToolRegistry::with_knowledge_graph(KnowledgeGraph::new(workdir.clone()))
        } else {
            ToolRegistry::new()
        };

        let sandbox = Sandbox::new(config.clone()).await?;
        let commander = Commander::new(llm.clone(), Arc::new(config.llm.clone()), workdir.clone());
        let metrics = Metrics::new();

        tracing::info!("执行引擎初始化完成 | workdir: {}", workdir.display());

        Ok(Self { config, workdir, llm, context, planner, tools, sandbox, commander, metrics, knowledge_graph })
    }

    /// 执行一个任务
    pub async fn run(&self, task: &str) -> Result<ExecutionResult> {
        let started_at = Utc::now();
        tracing::info!("开始执行任务: {}", task);
        self.metrics.record_task_start(task);

        // 知识图谱上下文注入（Phase 1.5 核心能力）
        let graph_context = if let Some(ref kg) = self.knowledge_graph {
            kg.inject_prompt_context(task).await
        } else {
            None
        };

        let complexity = self.assess_complexity(task).await;
        let workflow = self.select_workflow(task, complexity).await;

        tracing::info!("任务复杂度: {} | 工作流: {}", complexity, workflow);

        let result = match workflow.as_str() {
            "react" => self.run_react(task, graph_context.clone()).await,
            "plan_execute" => self.run_plan_execute(task).await,
            "multi_agent" => self.run_multi_agent(task).await,
            _ => self.run_react(task, graph_context.clone()).await,
        }.map(|mut r| {
            r.started_at = started_at;
            r.ended_at = Utc::now();
            self.metrics.record_task_end(&r);
            r
        });

        result
    }

    async fn assess_complexity(&self, task: &str) -> TaskComplexity {
        TaskComplexity::infer_from(task, self.context.files.len())
    }

    async fn select_workflow(&self, _task: &str, complexity: TaskComplexity) -> String {
        complexity.suggested_workflow().to_string()
    }

    /// ── ReAct 工作流（简单任务）─────────────────────────────

    async fn run_react(&self, task: &str, graph_context: Option<String>) -> Result<ExecutionResult> {
        tracing::debug!("使用 ReAct 工作流");

        let system_prompt = self.build_system_prompt(task, graph_context.as_deref());
        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(task),
        ];

        let mut iterations = 0;
        let max_iterations = 20;
        let mut artifacts = Vec::new();
        let mut warnings = Vec::new();
        let mut total_tokens = 0;
        let mut last_response = String::new();

        loop {
            iterations += 1;
            if iterations > max_iterations {
                warnings.push(format!("达到最大迭代次数 ({})，任务可能未完成", max_iterations));
                break;
            }

            let request = LlmRequest {
                model: "auto".into(),
                messages: messages.clone(),
                temperature: Some(0.7),
                max_tokens: Some(4096),
                stream: false,
                tools: Some(self.tools.available_tools()),
            };

            let response = self.llm.complete(request).await?;
            total_tokens += response.usage.total_tokens;
            last_response = response.content.clone();

            if let Some(tool_calls) = response.tool_calls {
                for call in tool_calls {
                    tracing::debug!("执行工具: {} | 参数: {}", call.name, call.arguments);
                    let result = self.tools.execute(&call.name, &call.arguments, &self.workdir).await;

                    match result {
                        Ok(output) => {
                            messages.push(Message::assistant(response.content.clone()));
                            messages.push(Message::tool(output, &call.name, &call.id));
                            artifacts.push(Artifact {
                                kind: ArtifactKind::File,
                                path: call.name.clone(),
                                description: format!("工具 {} 执行结果", call.name),
                                content_preview: None,
                                verified: true,
                                language: None,
                            });
                        }
                        Err(e) => {
                            warnings.push(format!("工具 {} 执行失败: {}", call.name, e));
                            messages.push(Message::assistant(response.content.clone()));
                            messages.push(Message::tool(format!("错误: {}", e), &call.name, &call.id));
                        }
                    }
                }
            } else {
                messages.push(Message::assistant(response.content.clone()));
                break;
            }
        }

        Ok(ExecutionResult {
            task: task.to_string(),
            status: if warnings.is_empty() { ExecutionStatus::Success } else { ExecutionStatus::PartiallyCompleted },
            summary: last_response,
            artifacts,
            warnings: if warnings.is_empty() { None } else { Some(warnings) },
            started_at: Utc::now(),
            ended_at: Utc::now(),
            tokens_used: total_tokens,
            steps_executed: iterations,
            suggestions: Vec::new(),
        })
    }

    /// ── Plan-Execute 工作流（中等复杂度）───────────────────
    ///
    /// hermes-agent-rs L2 核心能力落地：
    /// 1. 生成计划后调用 make_l2_decision 决定并行度、检查点、拆分
    /// 2. 根据 L2.parallelism_degree 决定批次大小
    /// 3. 无依赖 + parallel=true 的步骤用 JoinSet 真并发执行
    /// 4. 高风险步骤（requires_approval）强制串行
    async fn run_plan_execute(&self, task: &str) -> Result<ExecutionResult> {
        tracing::info!("使用 Plan-Execute 工作流");

        // Step 1: 生成计划
        let plan = self.planner.plan(task, &self.context).await?;

        // Step 2: L2 决策（hermes L2 核心能力）
        let l2 = self.planner.make_l2_decision(&plan, &self.context).await;

        tracing::info!(
            "L2 决策: 并行度={}, 检查点={}/次, 风险={:?}, 拆分={}步",
            l2.parallelism_degree, l2.checkpoint_interval, l2.risk_level,
            l2.split_hints.iter().filter(|h| h.should_split).count()
        );

        let mut artifacts = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut total_tokens = 0;
        let mut completed = 0;

        // ── 找可并行步骤 ──────────────────────────────────────
        // parallel=true 且无依赖的步骤可以并发执行
        let parallel_steps: Vec<_> = plan.steps.iter()
            .filter(|s| s.parallel && s.depends_on.is_empty())
            .cloned()
            .collect();

        if parallel_steps.len() >= 2 && l2.parallelism_degree >= 2 {
            // JoinSet 真并发执行（hermes 核心优势）
            tracing::info!("L2 并行执行 {} 个无依赖步骤（批次大小={}）",
                parallel_steps.len(), l2.parallelism_degree);

            for chunk in parallel_steps.chunks(l2.parallelism_degree) {
                tracing::info!("并行批次: {} 个步骤", chunk.len());
                let mut join_set = JoinSet::new();

                for step in chunk {
                    let step_instr = self.build_step_instruction(task, step);
                    let workdir = self.workdir.clone();
                    let tools = self.tools.clone();
                    let llm = self.llm.clone();

                    // 每个步骤独立执行，不阻塞其他步骤
                    join_set.spawn(async move {
                        Self::execute_step_react(&tools, &llm, &step_instr, &workdir).await
                    });
                }

                // 收集本批次结果（任一失败不影响其他）
                while let Some(result) = join_set.join_next().await {
                    match result {
                        Ok(Ok(step_result)) => {
                            total_tokens += step_result.tokens_used;
                            artifacts.extend(step_result.artifacts);
                            if let Some(ref ws) = step_result.warnings {
                                warnings.extend(ws.clone());
                            }
                            completed += 1;

                            // 检查点：长任务定期保存进度
                            if l2.checkpoint_enabled && completed % l2.checkpoint_interval == 0 {
                                tracing::info!("📌 检查点: 已完成 {}/{} 步骤",
                                    completed, plan.total_steps());
                            }
                        }
                        Ok(Err(e)) => {
                            warnings.push(format!("并行步骤执行失败: {}", e));
                        }
                        Err(panic) => {
                            warnings.push(format!("并行任务panic: {}", panic));
                        }
                    }
                }
            }
        }

        // ── 串行执行有依赖或需要审批的步骤 ─────────────────────
        for step in &plan.steps {
            // 已并行的跳过
            if step.parallel && step.depends_on.is_empty() {
                continue;
            }

            // 检查依赖是否失败
            let dep_failed = step.depends_on.iter().any(|dep| {
                plan.steps.iter().any(|s| s.id == *dep && s.status == PlanStepStatus::Failed)
            });
            if dep_failed {
                warnings.push(format!("步骤 '{}' 依赖失败，跳过", step.description));
                continue;
            }

            // 高风险步骤强制串行 + 警告（hermes 安全保障）
            if step.requires_approval {
                tracing::warn!("⚠️  步骤 '{}' 需要人工审批，当前跳过", step.description);
                warnings.push(format!("步骤 '{}' 需要人工审批，已跳过", step.description));
                continue;
            }

            tracing::info!("串行执行: {}", step.description);

            let step_instr = self.build_step_instruction(task, step);
            match Self::execute_step_react(&self.tools, &self.llm, &step_instr, &self.workdir).await {
                Ok(step_result) => {
                    total_tokens += step_result.tokens_used;
                    artifacts.extend(step_result.artifacts);
                    if let Some(ref ws) = step_result.warnings {
                        warnings.extend(ws.clone());
                    }
                    completed += 1;

                    if l2.checkpoint_enabled && completed % l2.checkpoint_interval == 0 {
                        tracing::info!("📌 检查点: 已完成 {}/{} 步骤", completed, plan.total_steps());
                    }
                }
                Err(e) => {
                    warnings.push(format!("步骤 '{}' 执行失败: {}", step.description, e));
                }
            }
        }

        let success_count = completed.saturating_sub(parallel_steps.len());

        let summary = format!(
            "计划执行完成 | L2并行度={} | {}/{} 步骤成功 | {} 个产物 | 风险={:?}",
            l2.parallelism_degree, completed, plan.total_steps(),
            artifacts.len(), l2.risk_level,
        );

        tracing::info!("{}", summary);

        Ok(ExecutionResult {
            task: task.to_string(),
            status: if warnings.is_empty() { ExecutionStatus::Success } else { ExecutionStatus::PartiallyCompleted },
            summary,
            artifacts,
            warnings: if warnings.is_empty() { None } else { Some(warnings) },
            started_at: Utc::now(),
            ended_at: Utc::now(),
            tokens_used: total_tokens,
            steps_executed: completed,
            suggestions: Vec::new(),
        })
    }

    /// 构建步骤执行指令
    fn build_step_instruction(&self, task: &str, step: &crate::planning::PlanStep) -> String {
        format!(
            "当前任务: {}\n\n执行步骤: {}\n{}\n\n请完成此步骤，输出执行结果和产物。",
            task,
            step.description,
            step.details.as_deref().unwrap_or(""),
        )
    }

    /// 执行单个步骤（ReAct 模式）
    async fn execute_step_react(
        tools: &ToolRegistry,
        llm: &LlmClient,
        instruction: &str,
        workdir: &PathBuf,
    ) -> Result<ExecutionResult> {
        let system_prompt = "你是 Acode，一个全流程自主编码引擎。使用工具完成当前步骤。";
        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(instruction),
        ];

        let mut iterations = 0;
        let max_iterations = 10;
        let mut artifacts = Vec::new();
        let mut warnings = Vec::new();
        let mut total_tokens = 0;
        let mut last_response = String::new();

        loop {
            iterations += 1;
            if iterations > max_iterations {
                warnings.push(format!("步骤达到最大迭代次数 ({})", max_iterations));
                break;
            }

            let request = LlmRequest {
                model: "auto".into(),
                messages: messages.clone(),
                temperature: Some(0.7),
                max_tokens: Some(4096),
                stream: false,
                tools: Some(tools.available_tools()),
            };

            let response = llm.complete(request).await?;
            total_tokens += response.usage.total_tokens;
            last_response = response.content.clone();

            if let Some(tool_calls) = response.tool_calls {
                for call in tool_calls {
                    match tools.execute(&call.name, &call.arguments, workdir).await {
                        Ok(output) => {
                            messages.push(Message::assistant(response.content.clone()));
                            messages.push(Message::tool(output, &call.name, &call.id));
                        }
                        Err(e) => {
                            warnings.push(format!("工具 {} 执行失败: {}", call.name, e));
                            messages.push(Message::assistant(response.content.clone()));
                            messages.push(Message::tool(format!("错误: {}", e), &call.name, &call.id));
                        }
                    }
                }
            } else {
                messages.push(Message::assistant(response.content.clone()));
                break;
            }
        }

        Ok(ExecutionResult {
            task: instruction.to_string(),
            status: if warnings.is_empty() { ExecutionStatus::Success } else { ExecutionStatus::PartiallyCompleted },
            summary: last_response,
            artifacts,
            warnings: if warnings.is_empty() { None } else { Some(warnings) },
            started_at: Utc::now(),
            ended_at: Utc::now(),
            tokens_used: total_tokens,
            steps_executed: iterations,
            suggestions: Vec::new(),
        })
    }

    /// ── Multi-Agent 工作流（复杂任务）────────────────────────

    async fn run_multi_agent(&self, task: &str) -> Result<ExecutionResult> {
        tracing::info!("使用 Multi-Agent 工作流");
        self.commander.execute(task, &self.context).await
    }

    /// 构建系统提示词
    fn build_system_prompt(&self, task: &str, graph_context: Option<&str>) -> String {
        format!(r#"你是 Acode，一个全流程自主编码引擎。

## 核心规则
1. 当用户要求创建/修改文件时，**必须**使用 write_file 工具，不能只输出代码
2. 当用户要求执行命令时，**必须**使用 run_command 工具
3. 当需要了解项目结构时，使用 list_directory / read_file 工具
4. 所有代码必须实际写入文件才算完成

## 当前项目
- 工作目录：{}
- 技术栈：{:?}
- 语言：{:?}
- 文件数量：{}
- 主要文件：{:?}

## 可用工具（严格按 JSON 格式调用）
- write_file: {{"path":"文件路径","content":"文件内容"}}
- read_file: {{"path":"文件路径"}}
- run_command: {{"command":"命令","timeout":秒}}
- list_directory: {{"path":"目录路径","recursive":true/false}}
- search_files: {{"query":"搜索文本"}}
- git_status: {{}}
- git_diff: {{}}
- web_search: {{"query":"搜索文本"}}

## 任务：{}
{}

"#, self.workdir.display(), self.context.tech_stack, self.context.languages,
           self.context.file_count, self.context.files.iter().take(20).collect::<Vec<_>>(), task,
           graph_context.unwrap_or_default())
    }
}
