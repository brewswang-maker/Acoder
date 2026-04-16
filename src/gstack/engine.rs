//! gstack 结构化执行引擎
//!
//! 按 SKILL.md 的 steps 顺序执行，每个 step 生成 LLM prompt，
//! 收集工具调用结果，循环直到 step 完成。

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::gstack::template::{Choice, SkillTemplate, Step, TemplateContext};
use crate::gstack::types::{ExecStatus, StepOutcome, StepResult};
use crate::gstack::exec_log::ExecLog;

/// 引擎配置
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 最大工具调用次数（防无限循环）
    pub max_tool_calls_per_step: usize,
    /// 最大 step 循环轮次
    pub max_llm_turns: usize,
    /// 是否自动处理 AskUserQuestion（spawned session 跳过）
    pub auto_choose_recommended: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_tool_calls_per_step: 200,
            max_llm_turns: 50,
            auto_choose_recommended: true,
        }
    }
}

/// 执行引擎
pub struct ExecutionEngine {
    template: Arc<SkillTemplate>,
    ctx: TemplateContext,
    exec_log: Arc<Mutex<ExecLog>>,
    config: EngineConfig,
}

impl ExecutionEngine {
    pub fn new(template: SkillTemplate, ctx: TemplateContext) -> Self {
        Self {
            template: Arc::new(template),
            ctx,
            exec_log: Arc::new(Mutex::new(ExecLog::default())),
            config: EngineConfig::default(),
        }
    }

    pub fn with_config(template: SkillTemplate, ctx: TemplateContext, config: EngineConfig) -> Self {
        Self {
            template: Arc::new(template),
            ctx,
            exec_log: Arc::new(Mutex::new(ExecLog::default())),
            config,
        }
    }

    /// 执行完整 skill，返回每步结果
    pub async fn run(&self) -> Result<Vec<StepResult>> {
        let step_count = self.template.steps.len();

        tracing::info!(
            skill = %self.template.frontmatter.name,
            version = %self.template.frontmatter.version,
            steps = step_count,
            "gstack: starting skill execution",
        );

        // 初始化 log
        {
            let mut log = self.exec_log.lock().await;
            log.skill = self.template.frontmatter.name.clone();
            log.version = self.template.frontmatter.version.clone();
            log.branch = self.ctx.branch.clone();
            log.started_at = now_ts();
        }

        let mut results = Vec::new();

        for step in &self.template.steps {
            let result = self.run_step(step).await;
            let status = ExecStatus::from(&result.outcome);

            // 记录到 log
            {
                let mut log = self.exec_log.lock().await;
                log.record_step(result.step, result.name.clone(), status);
            }

            results.push(result);

            // 致命错误直接终止
            if matches!(
                results.last().map(|r| &r.outcome),
                Some(StepOutcome::Blocked(_) | StepOutcome::NeedsContext(_))
            ) {
                tracing::warn!(
                    step = results.last().unwrap().step,
                    "gstack: step blocked, aborting skill",
                );
                break;
            }

            // 超限则 escalation
            if matches!(results.last(), Some(r) if r.tool_calls >= self.config.max_tool_calls_per_step) {
                tracing::error!("gstack: step exceeded max tool calls, escalating");
                break;
            }
        }

        // 完成日志
        {
            let mut log = self.exec_log.lock().await;
            log.finalize();
            let json = log.to_json();
            tracing::debug!(log = %json, "gstack: execution log");
        }

        Ok(results)
    }

    /// 执行单个 step
    async fn run_step(&self, step: &Step) -> StepResult {
        let start = std::time::Instant::now();

        // 条件检查
        if let Some(cond) = &step.condition {
            if !self.eval_condition(cond) {
                tracing::debug!(step = step.number, condition = cond, "step skipped");
                return StepResult {
                    step: step.number,
                    name: step.name.clone(),
                    outcome: StepOutcome::Done,
                    tool_calls: 0,
                    reasoning_tokens: 0,
                    error: None,
                };
            }
        }

        tracing::info!(step = step.number, name = %step.name, "gstack: executing step");

        let prompt = self.build_step_prompt(step);
        let mut tool_calls = 0;
        let mut done = false;
        let mut concerns = Vec::new();
        let mut needs_context = Vec::new();
        let mut response = String::new();

        while tool_calls < self.config.max_tool_calls_per_step && !done {
            tool_calls += 1;

            match self.call_llm(&prompt, step, &response).await {
                Ok(resp) => {
                    response = resp;
                    done = self.assess_done(&response);

                    for line in response.lines() {
                        let line = line.trim();
                        if line.starts_with("CONCERN: ") {
                            concerns.push(line.replace("CONCERN: ", "").trim().to_string());
                        }
                        if line.starts_with("NEEDS_CONTEXT: ") {
                            needs_context.push(line.replace("NEEDS_CONTEXT: ", "").trim().to_string());
                        }
                        if line.starts_with("STATUS: BLOCKED") {
                            done = true;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(step = step.number, error = %e, "llm call failed");
                    return StepResult {
                        step: step.number,
                        name: step.name.clone(),
                        outcome: StepOutcome::Blocked(e.to_string()),
                        tool_calls,
                        reasoning_tokens: 0,
                        error: Some(e.to_string()),
                    };
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let outcome = if !needs_context.is_empty() {
            StepOutcome::NeedsContext(needs_context)
        } else if !concerns.is_empty() {
            StepOutcome::DoneWithConcerns(concerns)
        } else if response.contains("STATUS: BLOCKED") {
            StepOutcome::Blocked("step marked blocked".to_string())
        } else if !step.choices.is_empty() && step.user_confirm {
            StepOutcome::UserConfirm(step.choices.clone())
        } else {
            StepOutcome::Done
        };

        StepResult {
            step: step.number,
            name: step.name.clone(),
            outcome,
            tool_calls,
            reasoning_tokens: (tool_calls as u32) * 50,
            error: None,
        }
    }

    /// 构建 step 的 LLM prompt
    fn build_step_prompt(&self, step: &Step) -> String {
        let mut out = String::new();

        // Voice（只在第一步注入）
        if step.number == 1 {
            if let Some(v) = &self.template.voice_text {
                out.push_str(v);
                out.push_str("\n\n---\n\n");
            }
        }

        // AskUserQuestion 格式
        if let Some(fmt) = &self.template.ask_format {
            out.push_str("## AskUserQuestion Format\n");
            out.push_str(fmt);
            out.push_str("\n\n");
        }

        // Completeness Principle
        if let Some(cp) = &self.template.completeness {
            out.push_str("## Completeness Principle\n");
            out.push_str(cp);
            out.push_str("\n\n");
        }

        // 当前任务
        out.push_str("## Current Task\n");
        out.push_str(&format!("Skill: /{}\n", self.template.frontmatter.name));
        out.push_str(&format!("Step {}/{}: {}\n", step.number, self.template.steps.len(), step.name));
        out.push_str(&format!("Goal: {}\n\n", step.goal));

        // 工具约束
        let tools = if step.allowed_tools.is_empty() {
            self.template.frontmatter.allowed_tools.join(", ")
        } else {
            step.allowed_tools.join(", ")
        };
        if !tools.is_empty() {
            out.push_str(&format!("Allowed tools: {}\n\n", tools));
        }

        // Step 指导
        out.push_str("## Step Guidance\n");
        out.push_str(&step.guidance);
        out.push_str("\n\n");

        // 选项
        if !step.choices.is_empty() {
            out.push_str("## Options\n");
            for c in &step.choices {
                out.push_str(&format!("{})\n  {}\n", c.label, c.text));
            }
            out.push_str("\nUse the AskUserQuestion tool to present these options.\n");
        }

        if step.user_confirm {
            out.push_str("\nAfter completing this step, use AskUserQuestion to confirm before proceeding.\n");
        }

        out
    }

    /// 调用 LLM
    async fn call_llm(
        &self,
        prompt: &str,
        step: &Step,
        _conversation: &str,
    ) -> Result<String> {
        // TODO: 注入 acode 的 LLM client
        // 使用 src/llm/ 中的 LlmClient trait
        let _ = (prompt, step);
        anyhow::bail!(
            "LLM client not injected yet — implement LlmClient in src/llm/ \
             and call GstackLlmClient::new(llm_client).call(prompt)"
        )
    }

    /// 评估 step 是否完成
    fn assess_done(&self, response: &str) -> bool {
        response.contains("STATUS: DONE")
            || response.contains("STATUS: DONE_WITH_CONCERNS")
            || response.contains("DONE")
    }

    /// 评估条件
    fn eval_condition(&self, cond: &str) -> bool {
        if let Some((k, v)) = cond.split_once('=') {
            let v = v.trim();
            match k.trim() {
                "BRANCH" => &self.ctx.branch == v,
                "SLUG" => &self.ctx.slug == v,
                "REPO_MODE" => &self.ctx.repo_mode == v,
                "PROACTIVE" => v == "true",
                "SKILL_PREFIX" => v == "true",
                "SPAWNED_SESSION" => v == "true",
                "VENDORED_GSTACK" => v == "yes",
                _ => false,
            }
        } else {
            true
        }
    }
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
