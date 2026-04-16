//! Commander — 指挥官 Agent
//!
//! 负责理解用户目标，协调专家团队，管理任务执行流程
//! 核心：每个 Expert 执行使用 ReAct 循环，支持工具调用

use std::path::PathBuf;
use std::sync::Arc;
use crate::config::LlmConfig;
use crate::llm::{Client as LlmClient, Message, LlmRequest};
use crate::context::Context;
use crate::execution::engine::{Artifact, ArtifactKind, ExecutionResult, ExecutionStatus};
use crate::execution::tool_registry::ToolRegistry;
use crate::error::Result;
use crate::agents::ExpertRegistry;

/// 指挥官 Agent：全局协调者
pub struct Commander {
    llm: LlmClient,
    config: Arc<LlmConfig>,
    experts: ExpertRegistry,
    tools: ToolRegistry,
    workdir: PathBuf,
}

impl Commander {
    pub fn new(llm: LlmClient, config: Arc<LlmConfig>, workdir: PathBuf) -> Self {
        Self {
            llm,
            config,
            experts: ExpertRegistry::new(),
            tools: ToolRegistry::new(),
            workdir,
        }
    }

    /// 接收任务，协调执行
    pub async fn execute(&self, task: &str, context: &Context) -> Result<ExecutionResult> {
        tracing::info!("Commander 接收任务: {}", task);

        // Step 1: 理解任务，选择专家团队
        let team = self.select_team(task).await;
        tracing::info!("选择的专家团队: {:?}", team);

        // Step 2: 分解任务
        let plan = self.decompose_task(task, &team).await?;

        // Step 3: 并行/串行执行各子任务
        let mut results = Vec::new();
        for (i, sub_task) in plan.iter().enumerate() {
            tracing::info!("[Commander] 执行子任务 {}/{}: {}", i + 1, plan.len(), sub_task);

            let expert = self.select_expert_for_task(sub_task);
            let result = self.execute_with_expert(sub_task, &expert, context).await?;
            results.push(result);
        }

        // Step 4: 汇总结果
        let summary = self.summarize_results(&results).await?;

        Ok(ExecutionResult {
            task: task.to_string(),
            status: if results.iter().all(|r| r.status == ExecutionStatus::Success) {
                ExecutionStatus::Success
            } else {
                ExecutionStatus::PartiallyCompleted
            },
            summary,
            artifacts: results.iter().flat_map(|r| r.artifacts.clone()).collect(),
            warnings: None,
            started_at: chrono::Utc::now(),
            ended_at: chrono::Utc::now(),
            tokens_used: results.iter().map(|r| r.tokens_used).sum(),
            steps_executed: results.len(),
            suggestions: Vec::new(),
        })
    }

    /// 选择专家团队
    async fn select_team(&self, task: &str) -> Vec<String> {
        let task_lower = task.to_lowercase();
        let mut team = vec!["coder".to_string()];

        if task_lower.contains("前端") || task_lower.contains("react") || task_lower.contains("vue") || task_lower.contains("页面") {
            team.push("frontend".to_string());
        }
        if task_lower.contains("后端") || task_lower.contains("api") || task_lower.contains("server") {
            team.push("backend".to_string());
        }
        if task_lower.contains("安全") || task_lower.contains("漏洞") {
            team.push("security".to_string());
        }
        if task_lower.contains("测试") || task_lower.contains("test") {
            team.push("tester".to_string());
        }
        if task_lower.contains("架构") || task_lower.contains("设计") || task_lower.contains("重构") {
            team.push("architect".to_string());
        }
        if task_lower.contains("review") || task_lower.contains("审查") {
            team.push("reviewer".to_string());
        }

        team
    }

    /// 分解任务
    async fn decompose_task(&self, task: &str, team: &[String]) -> Result<Vec<String>> {
        let prompt = format!(
            "将以下任务分解为可并行执行的子任务列表（用 JSON 数组输出）：\n\n任务：{}\n可用专家：{:?}\n\n要求：\n- 每个子任务描述要具体\n- 子任务之间尽可能独立以支持并行\n- 只输出 JSON 数组",
            task, team
        );

        let messages = vec![
            Message::system("你是任务分解专家，输出 JSON 数组格式的子任务列表。"),
            Message::user(&prompt),
        ];

        let request = LlmRequest {
            model: "auto".into(),
            messages,
            temperature: Some(0.3),
            max_tokens: Some(4096),
            stream: false,
            tools: None,
        };

        let response = self.llm.complete(request).await
            .map_err(|e| crate::Error::PlanningFailed(e.to_string()))?;

        let tasks: Vec<String> = serde_json::from_str(&response.content)
            .unwrap_or_else(|_| {
                if let Some(start) = response.content.find('[') {
                    if let Some(end) = response.content.rfind(']') {
                        serde_json::from_str(&response.content[start..=end]).unwrap_or_default()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            });

        Ok(tasks)
    }

    /// 选择最适合的专家
    fn select_expert_for_task(&self, task: &str) -> String {
        self.experts.select(task)
    }

    /// 使用专家执行任务 — ReAct 工具调用循环
    async fn execute_with_expert(&self, task: &str, expert: &str, context: &Context) -> Result<ExecutionResult> {
        let expert_info = self.experts.get(expert);

        let system_prompt = format!(
            "你是 {name}（{description}）。\n\n你的专长：{specialty:?}\n\n\
            项目上下文：{context}\n\n\
            你可以调用工具完成任务。可用工具：read_file, write_file, search_files, \
            list_directory, run_command, git_status, git_diff, grep, web_search\n\n\
            当用户要求创建/修改文件时，必须使用 write_file 工具。\
            当需要执行命令时，使用 run_command 工具。\
            完成后给出简洁的总结。",
            name = expert_info.name,
            description = expert_info.description,
            specialty = expert_info.specialty,
            context = context.summary(),
        );

        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(task),
        ];

        let mut iterations = 0;
        let max_iterations = 30;
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
                max_tokens: Some(8192),
                stream: false,
                tools: Some(self.tools.available_tools()),
            };

            let response = self.llm.complete(request).await
                .map_err(|e| crate::Error::AgentExecutionFailed {
                    agent_id: expert.into(),
                    reason: e.to_string(),
                })?;

            total_tokens += response.usage.total_tokens;
            last_response = response.content.clone();

            if let Some(tool_calls) = response.tool_calls {
                for call in tool_calls {
                    tracing::debug!("[Expert {}] 执行工具: {}", expert, call.name);
                    let result = self.tools.execute(&call.name, &call.arguments, &self.workdir).await;

                    match result {
                        Ok(output) => {
                            messages.push(Message::assistant(response.content.clone()));
                            messages.push(Message::tool(output, &call.name, &call.id));

                            // 记录 artifact
                            if call.name == "write_file" {
                                if let Ok(args) = serde_json::from_str::<serde_json::Value>(&call.arguments) {
                                    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                                        artifacts.push(Artifact {
                                            kind: ArtifactKind::File,
                                            path: path.to_string(),
                                            description: format!("Expert {} 创建的文件", expert),
                                            content_preview: None,
                                            verified: true,
                                            language: None,
                                        });
                                    }
                                }
                            }
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
            started_at: chrono::Utc::now(),
            ended_at: chrono::Utc::now(),
            tokens_used: total_tokens,
            steps_executed: iterations,
            suggestions: Vec::new(),
        })
    }

    /// 汇总结果
    async fn summarize_results(&self, results: &[ExecutionResult]) -> Result<String> {
        let summaries: Vec<_> = results.iter()
            .map(|r| format!("- {}: {}", r.task, r.summary.chars().take(200).collect::<String>()))
            .collect();

        let summary_text = summaries.join("\n");
        let success_count = results.iter().filter(|r| r.status == ExecutionStatus::Success).count();
        let total_count = results.len();

        Ok(format!(
            "任务完成: {}/{} 子任务成功\n\n{}\n\n总耗时: {:.1}s | Token消耗: {}",
            success_count, total_count,
            summary_text,
            results.iter().map(|r| r.duration_secs()).sum::<f64>(),
            results.iter().map(|r| r.tokens_used).sum::<usize>(),
        ))
    }
}
