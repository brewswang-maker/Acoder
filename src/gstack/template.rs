//! gstack SKILL.md 解析器
//!
//! 解析 YAML frontmatter 和 step 结构，生成 SkillTemplate。
//! 不执行任何工具，只负责解析和模板变量替换。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// YAML frontmatter
#[derive(Debug, Clone, Deserialize)]
pub struct Frontmatter {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub preamble_tier: Option<u8>,
    #[serde(default)]
    pub voice: Option<String>,
}

impl Frontmatter {
    pub fn from_markdown(content: &str) -> Result<Self> {
        let frontmatter = extract_yaml_frontmatter(content)
            .context("SKILL.md missing YAML frontmatter")?;
        serde_yaml::from_str(&frontmatter)
            .context("YAML frontmatter parse failed")
    }
}

/// 单个执行步骤
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    /// 步骤编号，从 1 开始
    pub number: u8,
    /// 步骤名称，如 "Pre-flight Checks"
    pub name: String,
    /// 步骤目标（一句话）
    pub goal: String,
    /// 步骤详细指导（markdown）
    pub guidance: String,
    /// 该步骤允许的工具列表（空=all）
    pub allowed_tools: Vec<String>,
    /// 条件触发（如 "if BLAZE=1"）
    pub condition: Option<String>,
    /// 完成后是否需要用户确认
    pub user_confirm: bool,
    /// AskUserQuestion 的选项（用于交互式分支）
    pub choices: Vec<Choice>,
}

/// AskUserQuestion 选项
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub label: String,        // "A", "B", "C"
    pub text: String,         // 选项文本
    pub branch: Option<String>, // 分支 ID 或 "continue"
    pub bash: Option<String>, // 执行命令
}

/// 解析后的完整 SKILL.md 模板
#[derive(Debug, Clone)]
pub struct SkillTemplate {
    pub frontmatter: Frontmatter,
    /// 所有步骤（按顺序）
    pub steps: Vec<Step>,
    /// preamble bash 脚本（跳过或适配）
    pub preamble_script: Option<String>,
    /// voice 定义文本
    pub voice_text: Option<String>,
    /// AskUserQuestion 格式规范（从 preamble 提取）
    pub ask_format: Option<String>,
    /// Completeness Principle 文本
    pub completeness: Option<String>,
    /// 完整 markdown 正文（解析前的原始内容）
    pub raw: String,
}

impl SkillTemplate {
    /// 从 SKILL.md 文件加载
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("read {}", path.as_ref().display()))?;
        Self::from_markdown(&content)
    }

    /// 从 markdown 字符串解析
    pub fn from_markdown(content: &str) -> Result<Self> {
        let frontmatter = Frontmatter::from_markdown(content)?;
        let preamble_script = extract_section(content, "## Preamble (run first)")
            .map(|s| s.to_string());
        let voice_text = extract_section(content, "## Voice")
            .map(|s| s.to_string());
        let ask_format = extract_section(content, "## AskUserQuestion Format")
            .map(|s| s.to_string());
        let completeness = extract_section(content, "## Completeness Principle")
            .map(|s| s.to_string());

        let steps = parse_steps(content);
        if steps.is_empty() {
            anyhow::bail!("SKILL.md has no ## Step sections — cannot execute");
        }

        Ok(Self {
            frontmatter,
            steps,
            preamble_script,
            voice_text,
            ask_format,
            completeness,
            raw: content.to_string(),
        })
    }

    /// 用上下文变量替换模板中的占位符
    pub fn render(&self, ctx: &TemplateContext) -> String {
        let mut out = self.raw.clone();

        // 替换常用变量
        out = out.replace("{{BRANCH}}", &ctx.branch);
        out = out.replace("{{SLUG}}", &ctx.slug);
        out = out.replace("{{SESSION_ID}}", &ctx.session_id);
        out = out.replace("{{REPO_MODE}}", &ctx.repo_mode);
        out = out.replace("{{TOOLS}}", &ctx.tools.join(", "));

        out
    }
}

/// 模板渲染上下文
#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub branch: String,
    pub slug: String,
    pub session_id: String,
    pub repo_mode: String,
    pub tools: Vec<String>,
}

// ── 内部解析函数 ──────────────────────────────────────────────

fn extract_yaml_frontmatter(content: &str) -> Option<String> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("---")?;
    Some(rest[..end].trim().to_string())
}

/// 提取指定标题下的内容块（到下一个 ## 标题为止）
fn extract_section(content: &str, heading: &str) -> Option<String> {
    let heading_pattern = format!("\n{}", heading);
    let start = content.find(&heading_pattern).or_else(|| content.find(heading))?;
    let start = start + heading.len();
    let rest = &content[start..];

    // 找到下一个 ## 标题
    let end = rest[1..].find("\n## ").map(|i| i + 1).unwrap_or(rest.len());
    let section = rest[..end].trim();

    // 去掉代码块标记中的 bash 脚本（preamble 单独处理）
    Some(section.to_string())
}

/// 从 markdown 解析所有 ## Step / ### Step / ### [Step N] 块
fn parse_steps(content: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut number = 0u8;

    // 匹配各种 step 标题格式
    // ## Step 1: Pre-flight Checks
    // ### Step 1: ...
    // ### [Step 1] ...
    let step_patterns = [
        r"(?m)^### \[Step (\d+)\] (.+)$",
        r"(?m)^## Step (\d+): (.+)$",
        r"(?m)^### Step (\d+): (.+)$",
    ];

    let mut positions: Vec<(usize, u8, String)> = Vec::new();

    for pattern in &step_patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            for cap in re.captures_iter(content) {
                let m = cap.get(0).unwrap();
                let n: u8 = cap[1].parse().unwrap_or(0);
                let name = cap[2].trim().to_string();
                positions.push((m.start(), n, name));
            }
        }
    }

    // 去重（同名 step 取第一次出现）
    positions.sort_by_key(|p| p.0);
    positions.dedup_by(|a, b| a.1 == b.1 && a.2 == b.2);

    for (i, (pos, n, name)) in positions.iter().enumerate() {
        let next_pos = positions.get(i + 1).map(|p| p.0).unwrap_or(content.len());
        let block = &content[*pos..next_pos];

        let goal = extract_goal_from_block(block);
        let guidance = block.to_string();
        let condition = extract_condition(block);
        let user_confirm = block.contains("AskUserQuestion");
        let choices = extract_choices(block);
        let allowed_tools = extract_tools_from_block(block);

        steps.push(Step {
            number: *n,
            name: name.clone(),
            goal,
            guidance,
            allowed_tools,
            condition,
            user_confirm,
            choices,
        });
    }

    steps.sort_by_key(|s| s.number);
    steps
}

fn extract_goal_from_block(block: &str) -> String {
    // 尝试从第一行提取 goal：格式 "### Step N: Goal text"
    for line in block.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            // 标题行，去掉 # 和 Step N: 前缀
            let s = line.trim_start_matches('#').trim_start_matches(|c: char| c.is_ascii_whitespace());
            if s.starts_with("Step ") || s.starts_with('[') {
                // "Step 1: Do the thing" 或 "[Step 1] Do the thing"
                let s = s.replace("Step ", "").replace(']', "");
                if let Some(pos) = s.find(':') {
                    return s[pos + 1..].trim().to_string();
                }
                return s.chars().skip_while(|c| c.is_ascii_digit() || *c == '.' || c.is_ascii_whitespace())
                    .skip_while(|c| c == &':' || c.is_ascii_whitespace())
                    .collect();
            }
        }
    }
    "Complete this step".to_string()
}

fn extract_condition(block: &str) -> Option<String> {
    // 查找 "If `VAR` is ..." 模式
    let re = regex::Regex::new(r"(?m)^If `([A-Z_]+)` is [`\"](.*)[`\"]").ok()?;
    let caps = re.captures(block)?;
    let var = caps.get(1)?.as_str();
    let val = caps.get(2)?.as_str();
    Some(format!("{}={}", var, val))
}

fn extract_tools_from_block(block: &str) -> Vec<String> {
    let mut tools = Vec::new();
    // 匹配 "Use: Bash, Read, Edit" 或 "Use: Bash" 等
    let re = regex::Regex::new(r"(?m)^Use:\s*(.+)$").ok();
    if let Some(re) = re {
        for cap in re.captures_iter(block) {
            for t in cap[1].split(',') {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    tools.push(t);
                }
            }
        }
    }
    tools
}

fn extract_choices(block: &str) -> Vec<Choice> {
    let mut choices = Vec::new();
    let re = regex::Regex::new(r"(?m)^([A-Z])\)\s*(.+)$").ok()?;
    for cap in re.captures_iter(block) {
        let label = cap[1].to_string();
        let text = cap[2].trim().to_string();
        choices.push(Choice {
            label,
            text,
            branch: None,
            bash: None,
        });
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: review
version: 1.0.0
description: Pre-landing PR review
allowed-tools:
  - Bash
  - Read
---
some content"#;
        let fm = Frontmatter::from_markdown(content).unwrap();
        assert_eq!(fm.name, "review");
        assert_eq!(fm.version, "1.0.0");
        assert_eq!(fm.allowed_tools, &["Bash", "Read"]);
    }

    #[test]
    fn test_parse_steps() {
        let content = r#"## Step 1: Pre-flight checks
Do this first.

## Step 2: Review code
Use: Bash, Read
A) Option A
B) Option B

### Step 3: Finalize
Wrap up."#;
        let steps = parse_steps(content);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].name, "Pre-flight checks");
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[1].name, "Review code");
        assert_eq!(steps[1].choices.len(), 2);
        assert_eq!(steps[1].allowed_tools, &["Bash", "Read"]);
    }
}
