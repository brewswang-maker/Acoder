//! gstack slash command 注册表
//!
//! 把 /review、/qa、/ship 等命令映射到 SKILL.md 并触发执行引擎。

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::gstack::engine::{ExecutionEngine, StepResult};
use crate::gstack::roles::Role;
use crate::gstack::template::{SkillTemplate, TemplateContext};
use crate::gstack::workspace::GstackWorkspace;

/// slash command 元信息
#[derive(Debug, Clone)]
pub struct SlashCommand {
    /// 命令名，不含斜杠
    pub name: String,
    /// 完整命令文本，如 "/review"
    pub full_name: String,
    /// 对应的 skill 目录名
    pub skill_name: String,
    /// 对应角色
    pub role: Role,
    /// 简要说明
    pub description: String,
    /// SKILL.md 路径
    pub skill_path: PathBuf,
    /// 触发关键词（用于路由匹配）
    pub aliases: Vec<String>,
    /// 是否支持参数
    pub has_args: bool,
}

impl SlashCommand {
    pub fn new(name: &str, skill_name: &str, role: Role, description: &str, skills_dir: &PathBuf) -> Self {
        let path = skills_dir.join(skill_name).join("SKILL.md");
        let aliases = Self::default_aliases(name);
        Self {
            name: name.to_string(),
            full_name: format!("/{}", name),
            skill_name: skill_name.to_string(),
            role,
            description: description.to_string(),
            skill_path: path,
            aliases,
            has_args: false,
        }
    }

    fn default_aliases(name: &str) -> Vec<String> {
        match name {
            "review" => vec!["code review".into(), "pre-landing".into(), "check diff".into()],
            "qa" => vec!["test this site".into(), "find bugs".into(), "qa this".into()],
            "ship" => vec!["create PR".into(), "ship it".into(), "merge".into()],
            "investigate" => vec!["debug".into(), "why is this broken".into(), "error".into()],
            "office-hours" => vec!["is this worth building".into(), "product idea".into()],
            "plan-eng-review" => vec!["architecture review".into(), "tech design".into()],
            _ => vec![],
        }
    }
}

/// 命令执行结果
#[derive(Debug)]
pub struct SlashCommandResult {
    pub command: String,
    pub skill: String,
    pub step_results: Vec<StepResult>,
    pub total_tool_calls: usize,
    pub elapsed_ms: u128,
    pub status: CommandStatus,
}

/// 命令最终状态
#[derive(Debug, Clone)]
pub enum CommandStatus {
    Success,
    DoneWithConcerns,
    Blocked,
    Escalated,
}

impl From<&StepResult> for CommandStatus {
    fn from(r: &StepResult) -> Self {
        use crate::gstack::engine::StepOutcome::*;
        match &r.outcome {
            Done => CommandStatus::Success,
            DoneWithConcerns(_) => CommandStatus::DoneWithConcerns,
            Blocked(_) | NeedsContext(_) => CommandStatus::Blocked,
            UserConfirm(_) | Escalate(_) => CommandStatus::Escalated,
        }
    }
}

/// 命令注册表
pub struct CommandRegistry {
    commands: HashMap<String, Arc<SlashCommand>>,
    skills_dir: PathBuf,
    workspace: Arc<RwLock<GstackWorkspace>>,
}

impl CommandRegistry {
    pub fn new(skills_dir: PathBuf) -> Self {
        let ws = GstackWorkspace::new().expect("gstack workspace init failed");
        Self {
            commands: HashMap::new(),
            skills_dir,
            workspace: Arc::new(RwLock::new(ws)),
        }
    }

    /// 初始化所有内置 gstack 命令
    pub fn init(&mut self) {
        let dir = &self.skills_dir;

        let builtins = [
            ("office-hours", "office-hours", Role::Ceo, "需求澄清、产品方向讨论"),
            ("plan-ceo-review", "plan-ceo-review", Role::Ceo, "CEO + EM 联合评审重大决策"),
            ("plan-eng-review", "plan-eng-review", Role::EngineeringManager, "技术架构评审"),
            ("plan-design-review", "plan-design-review", Role::Designer, "设计系统评审"),
            ("plan-devex-review", "plan-devex-review", Role::DevEx, "开发者体验评审"),
            ("review", "review", Role::QaLead, "PR 代码审查和安全审计"),
            ("qa", "qa", Role::QaLead, "端到端浏览器测试并修复 bug"),
            ("qa-only", "qa-only", Role::QaLead, "仅测试不修复"),
            ("ship", "ship", Role::ReleaseEngineer, "创建 PR、代码合并"),
            ("land-and-deploy", "land-and-deploy", Role::ReleaseEngineer, "合并 + 部署"),
            ("canary", "canary", Role::EngineeringManager, "金丝雀发布"),
            ("benchmark", "benchmark", Role::EngineeringManager, "性能基准测试"),
            ("investigate", "investigate", Role::EngineeringManager, "线上问题排查"),
            ("design-consultation", "design-consultation", Role::Designer, "设计咨询"),
            ("design-review", "design-review", Role::Designer, "视觉 + UX 评审"),
            ("design-shotgun", "design-shotgun", Role::Designer, "快速设计评审"),
            ("document-release", "document-release", Role::ReleaseEngineer, "发布文档生成"),
            ("health", "health", Role::EngineeringManager, "代码健康检查"),
            ("checkpoint", "checkpoint", Role::Ceo, "保存进度快照"),
            ("learn", "learn", Role::Ceo, "学习新技能或框架"),
            ("guard", "guard", Role::SecurityOfficer, "门卫安全检查"),
            ("careful", "careful", Role::SecurityOfficer, "谨慎模式（强制 review）"),
            ("freeze", "freeze", Role::ReleaseEngineer, "冻结主干分支"),
            ("unfreeze", "unfreeze", Role::ReleaseEngineer, "解冻主干分支"),
            ("devex-review", "devex-review", Role::DevEx, "开发体验专项评审"),
            ("cso", "cso", Role::SecurityOfficer, "云安全评审"),
            ("autoplan", "autoplan", Role::EngineeringManager, "自动任务规划"),
            ("browse", "browse", Role::DevEx, "抓取网页内容"),
            ("connect-chrome", "connect-chrome", Role::QaLead, "连接 Chrome 浏览器"),
            ("gstack-upgrade", "gstack-upgrade", Role::DevEx, "升级 gstack 版本"),
        ];

        for (cmd, skill, role, desc) in builtins {
            let skill_path = dir.join(skill).join("SKILL.md");
            if skill_path.exists() {
                self.register(SlashCommand::new(cmd, skill, role, desc, dir));
            } else {
                tracing::warn!("gstack skill not found: {} at {}", skill, skill_path.display());
            }
        }

        tracing::info!(count = self.commands.len(), "gstack command registry initialized");
    }

    pub fn register(&mut self, cmd: SlashCommand) {
        let full = cmd.full_name.clone();
        self.commands.insert(full, Arc::new(cmd));
    }

    /// 根据命令名查找
    pub fn get(&self, name: &str) -> Option<Arc<SlashCommand>> {
        // 支持 / 前缀和无前缀
        let name = name.trim_start_matches('/');
        self.commands.get(name)
            .or_else(|| self.commands.get(&format!("/{}", name)))
            .cloned()
    }

    /// 根据关键词匹配（用于自然语言路由）
    pub fn match_by_keyword(&self, text: &str) -> Option<Arc<SlashCommand>> {
        let text = text.to_lowercase();
        for cmd in self.commands.values() {
            if cmd.aliases.iter().any(|a| text.contains(&a.to_lowercase())) {
                return Some(Arc::clone(cmd));
            }
            if text.contains(&cmd.name) {
                return Some(Arc::clone(cmd));
            }
        }
        None
    }

    /// 列出所有可用命令
    pub fn list(&self) -> Vec<Arc<SlashCommand>> {
        self.commands.values().cloned().collect()
    }

    /// 执行命令
    pub async fn execute(
        &self,
        name: &str,
        args: Vec<String>,
        ctx: TemplateContext,
    ) -> Result<SlashCommandResult> {
        let start = std::time::Instant::now();
        let cmd = self.get(name).context("unknown gstack command")?;

        tracing::info!(command = %cmd.full_name, args = ?args, "executing gstack command");

        // 加载 skill 模板
        let template = SkillTemplate::from_file(&cmd.skill_path)
            .with_context(|| format!("load {}", cmd.skill_path.display()))?;

        // 更新 workspace
        {
            let mut ws = self.workspace.write().await;
            ws.record_skill_start(&cmd.name, &ctx.branch)?;
        }

        // 构建引擎并执行
        let engine = ExecutionEngine::new(template, ctx);
        let step_results = engine.run().await?;

        let total_tool_calls: usize = step_results.iter().map(|r| r.tool_calls).sum();
        let elapsed_ms = start.elapsed().as_millis();

        let overall = step_results.last();
        let status = overall
            .map(CommandStatus::from)
            .unwrap_or(CommandStatus::Success);

        // 更新 workspace
        {
            let mut ws = self.workspace.write().await;
            ws.record_skill_complete(&cmd.name, &status)?;
        }

        Ok(SlashCommandResult {
            command: cmd.full_name.clone(),
            skill: cmd.skill_name.clone(),
            step_results,
            total_tool_calls,
            elapsed_ms,
            status,
        })
    }
}
