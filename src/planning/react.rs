//! ReAct 执行引擎 — Thought→Action→Observation 循环
//!
//! 核心循环：观察 → 思考 → 行动 → 观察 → ...
//! 直到任务完成或达到最大迭代次数。
//!
//! 参考 ReAct 论文 (Yao et al., 2023)：
//! - Thought: 推理当前状态，决定下一步行动
//! - Action: 调用工具执行具体操作
//! - Observation: 获取工具返回结果，更新认知

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::error::{Error, Result};
use crate::execution::tool_registry::ToolRegistry;

/// ReAct 循环的最大迭代次数
const MAX_ITERATIONS: usize = 15;

/// 单次思考的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    pub iteration: usize,
    pub reasoning: String,
    pub action: Option<Action>,
    pub is_final: bool,
    pub timestamp: DateTime<Utc>,
}

/// 要执行的动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub tool: String,
    pub arguments: serde_json::Value,
    pub reason: String,
}

/// 工具执行的观察结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub iteration: usize,
    pub tool: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

/// ReAct 循环的完整执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActTrace {
    pub task: String,
    pub thoughts: Vec<Thought>,
    pub observations: Vec<Observation>,
    pub final_answer: Option<String>,
    pub total_iterations: usize,
    pub total_duration_ms: u64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// ReAct 执行引擎
pub struct ReActRunner {
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
}

impl ReActRunner {
    pub fn new(tools: Arc<ToolRegistry>) -> Self {
        Self {
            tools,
            max_iterations: MAX_ITERATIONS,
        }
    }

    /// 设置最大迭代次数
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// 运行 ReAct 循环
    ///
    /// 每次迭代：
    /// 1. Thought: LLM 推理当前状态，选择工具和参数
    /// 2. Action: 执行工具调用
    /// 3. Observation: 获取结果，更新上下文
    pub async fn run(&self, task: &str) -> Result<ReActTrace> {
        let started_at = Utc::now();
        let mut trace = ReActTrace {
            task: task.to_string(),
            thoughts: Vec::new(),
            observations: Vec::new(),
            final_answer: None,
            total_iterations: 0,
            total_duration_ms: 0,
            started_at,
            ended_at: None,
        };

        let mut context = format!("任务: {}\n\n请分析任务并决定下一步行动。", task);

        for iteration in 0..self.max_iterations {
            // Thought: 推理当前状态
            let thought = self.think(&context, iteration).await?;
            let is_final = thought.is_final;

            trace.thoughts.push(thought.clone());

            if is_final {
                trace.final_answer = Some(thought.reasoning.clone());
                break;
            }

            // Action + Observation
            if let Some(action) = &thought.action {
                let observation = self.act(action).await?;
                context = format!(
                    "{}\n\n[迭代 {}] 思考: {}\n行动: {}({})\n观察: {}",
                    context,
                    iteration + 1,
                    thought.reasoning,
                    action.tool,
                    action.arguments,
                    observation.output,
                );
                trace.observations.push(observation);
            }

            trace.total_iterations = iteration + 1;
        }

        // 如果达到最大迭代次数仍未完成，标记为超时
        if trace.final_answer.is_none() && trace.total_iterations >= self.max_iterations {
            trace.final_answer = Some(format!(
                "达到最大迭代次数 {}，任务未完成。最后思考: {}",
                self.max_iterations,
                trace.thoughts.last()
                    .map(|t| t.reasoning.as_str())
                    .unwrap_or("无"),
            ));
        }

        let ended_at = Utc::now();
        trace.total_duration_ms = (ended_at - started_at).num_milliseconds() as u64;
        trace.ended_at = Some(ended_at);

        Ok(trace)
    }

    /// 思考阶段：LLM 推理当前状态，决定下一步行动
    async fn think(&self, context: &str, iteration: usize) -> Result<Thought> {
        // TODO: 实际调用 LLM 进行推理
        // 当前返回占位结果，后续对接 LLM Client
        Ok(Thought {
            iteration,
            reasoning: format!("分析上下文，决定下一步行动（迭代 {}）", iteration),
            action: None,
            is_final: iteration >= self.max_iterations - 1,
            timestamp: Utc::now(),
        })
    }

    /// 行动阶段：执行工具调用
    async fn act(&self, action: &Action) -> Result<Observation> {
        let start = std::time::Instant::now();
        let workdir = std::path::PathBuf::from(".");

        let result = self.tools.execute(
            &action.tool,
            &action.arguments.to_string(),
            &workdir,
        ).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => Ok(Observation {
                iteration: 0,
                tool: action.tool.clone(),
                success: true,
                output,
                duration_ms,
            }),
            Err(e) => Ok(Observation {
                iteration: 0,
                tool: action.tool.clone(),
                success: false,
                output: format!("错误: {}", e),
                duration_ms,
            }),
        }
    }

    /// 从 LLM 响应中解析 Thought
    pub fn parse_thought(response: &str) -> Thought {
        let is_final = response.contains("FINAL ANSWER")
            || response.contains("任务完成")
            || response.contains("已完成");

        // 尝试解析 Action
        let action = if let Some(action_start) = response.find("Action:") {
            let action_str = &response[action_start + 7..];
            let tool_end = action_str.find('(').unwrap_or(action_str.len());
            let tool = action_str[..tool_end].trim().to_string();
            Some(Action {
                tool,
                arguments: serde_json::Value::Null,
                reason: response.to_string(),
            })
        } else {
            None
        };

        Thought {
            iteration: 0,
            reasoning: response.to_string(),
            action,
            is_final,
            timestamp: Utc::now(),
        }
    }
}
