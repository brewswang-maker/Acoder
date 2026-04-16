//! gstack .gstack/ 工作区状态管理
//!
//! 管理 ~/.gstack/ 目录下的会话状态、检查点、学习记录。

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::gstack::commands::CommandStatus;

/// .gstack/ 工作区
pub struct GstackWorkspace {
    /// ~/.gstack/ 根目录
    root: PathBuf,
    /// 当前项目 slug
    pub slug: String,
    /// 当前分支
    pub branch: String,
    /// skill 执行历史
    skill_history: Vec<SkillRecord>,
    /// 检查点
    checkpoints: HashMap<String, Checkpoint>,
}

#[derive(Debug, Clone)]
pub struct SkillRecord {
    pub skill: String,
    pub branch: String,
    pub status: CommandStatus,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub tool_calls: usize,
}

#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub id: String,
    pub created_at: u64,
    pub summary: String,
    pub branch: String,
    pub path: PathBuf,
}

impl GstackWorkspace {
    pub fn new() -> Result<Self> {
        let root = dirs::home_dir()
            .context("no home dir")?
            .join(".gstack");

        let slug = Self::detect_slug()?;
        let branch = Self::detect_branch();

        fs::create_dir_all(root.join("projects").join(&slug))?;
        fs::create_dir_all(root.join("sessions"))?;
        fs::create_dir_all(root.join("analytics"))?;

        Ok(Self {
            root,
            slug,
            branch,
            skill_history: Vec::new(),
            checkpoints: HashMap::new(),
        })
    }

    fn detect_slug() -> Result<String> {
        let git_root = Self::git_repo_root()?;
        let repo_name = git_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 尝试读 .gitstack/slug 文件（gstack-cli 创建的）
        let slug_file = git_root.join(".gitstack").join("slug");
        if let Ok(slug) = fs::read_to_string(&slug_file) {
            return Ok(slug.trim().to_string());
        }

        Ok(format!("local-{}", repo_name))
    }

    fn git_repo_root() -> Result<PathBuf> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("not in a git repo");
        }
        let path = String::from_utf8_lossy(&output.stdout);
        Ok(PathBuf::from(path.trim()))
    }

    fn detect_branch() -> String {
        std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).into_owned().into())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn project_dir(&self) -> PathBuf {
        self.root.join("projects").join(&self.slug)
    }

    /// 记录 skill 开始
    pub fn record_skill_start(&mut self, skill: &str, branch: &str) -> Result<()> {
        let rec = SkillRecord {
            skill: skill.to_string(),
            branch: branch.to_string(),
            status: CommandStatus::Success,
            started_at: now_ts(),
            completed_at: None,
            tool_calls: 0,
        };
        self.skill_history.push(rec);
        self.save_timeline()?;
        Ok(())
    }

    /// 记录 skill 完成
    pub fn record_skill_complete(&mut self, skill: &str, status: &CommandStatus) -> Result<()> {
        if let Some(rec) = self.skill_history.iter_mut().rev().find(|r| r.skill == skill && r.completed_at.is_none()) {
            rec.completed_at = Some(now_ts());
            rec.status = status.clone();
        }
        self.save_timeline()?;
        self.save_analytics(skill, status)?;
        Ok(())
    }

    /// 创建检查点
    pub fn create_checkpoint(&mut self, summary: &str, content: &str) -> Result<String> {
        let id = format!("cp-{}", now_ts());
        let path = self.project_dir().join("checkpoints").join(format!("{}.md", id));
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, content)?;

        let cp = Checkpoint {
            id: id.clone(),
            created_at: now_ts(),
            summary: summary.to_string(),
            branch: self.branch.clone(),
            path: path.clone(),
        };
        self.checkpoints.insert(id.clone(), cp.clone());
        Ok(id)
    }

    /// 读取最新检查点
    pub fn latest_checkpoint(&self) -> Option<Checkpoint> {
        self.checkpoints.values()
            .filter(|c| c.branch == self.branch)
            .max_by_key(|c| c.created_at)
            .cloned()
    }

    /// 保存 skill 使用分析
    fn save_analytics(&self, skill: &str, status: &CommandStatus) -> Result<()> {
        let path = self.root.join("analytics").join("skill-usage.jsonl");
        let status_str = match status {
            CommandStatus::Success => "success",
            CommandStatus::DoneWithConcerns => "done_with_concerns",
            CommandStatus::Blocked => "blocked",
            CommandStatus::Escalated => "escalated",
        };
        let entry = serde_json::json!({
            "skill": skill,
            "ts": iso_now(),
            "status": status_str,
            "repo": self.slug,
        });
        let line = serde_json::to_string(&entry)? + "\n";
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?
            .write_all(line.as_bytes())?;
        Ok(())
    }

    /// 保存 timeline.jsonl
    fn save_timeline(&self) -> Result<()> {
        let path = self.project_dir().join("timeline.jsonl");
        fs::create_dir_all(path.parent().unwrap())?;
        let mut lines = Vec::new();
        for r in &self.skill_history {
            let line = serde_json::json!({
                "skill": r.skill,
                "branch": r.branch,
                "status": format!("{:?}", r.status).to_lowercase(),
                "started_at": r.started_at,
                "completed_at": r.completed_at,
                "event": if r.completed_at.is_some() { "completed" } else { "started" },
            });
            lines.push(serde_json::to_string(&line)?);
        }
        fs::write(&path, lines.join("\n") + "\n")?;
        Ok(())
    }
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn iso_now() -> String {
    chrono_lite()
}

fn chrono_lite() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    // 简化版 ISO-8601: YYYY-MM-DDTHH:MM:SS
    let secs = now.as_secs();
    // 只取前10位做简单时间戳（避免引入 chrono 依赖）
    let dt = unix_to_components(secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.0, dt.1, dt.2, dt.3, dt.4, dt.5)
}

fn unix_to_components(secs: u64) -> (u16, u8, u8, u8, u8, u8) {
    // 简化: 基于 2000-01-01 的天数
    const EPOCH_2000: u64 = 946684800;
    let elapsed = secs.saturating_sub(EPOCH_2000);
    let days = elapsed / 86400;
    let rem = elapsed % 86400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    let year = 2000 + (days / 365) as u16;
    let yday = days % 365;
    // 粗略月份
    let month = ((yday / 30).min(11)) as u8 + 1;
    let day = ((yday % 30).min(29)) as u8 + 1;
    (year, month, day, hour as u8, min as u8, sec as u8)
}
