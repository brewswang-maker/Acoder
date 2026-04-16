//! 编辑模块 — 变更预览 + 用户确认工作流
//!
//! 核心设计：
//! 1. LLM 生成 EditProposal（待确认的变更）
//! 2. 计算 diff 并展示预览
//! 3. 用户 apply（确认）或 reject（取消）
//!
//! 工作流：
//!   set_pending(proposal) → pending_edit = Some(proposal)
//!   apply()        → 写入磁盘，pending = None
//!   reject()       → 丢弃，pending = None

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 变更类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Create,
    Modify,
    Delete,
    Rename,
}

/// 单个文件的变更提案
#[derive(Debug, Clone)]
pub struct FileChange {
    pub change_type: ChangeType,
    pub path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub added_lines: usize,
    pub removed_lines: usize,
}

impl FileChange {
    pub fn compute_diff_stats(&mut self) {
        match (&self.old_content, &self.new_content) {
            (Some(old), Some(new)) => {
                let old_n = old.lines().count();
                let new_n = new.lines().count();
                if new_n > old_n { self.added_lines = new_n - old_n; }
                else { self.removed_lines = old_n - new_n; }
            }
            (None, Some(n)) => self.added_lines = n.lines().count(),
            (Some(o), None) => self.removed_lines = o.lines().count(),
            _ => {}
        }
    }
}

/// 编辑提案
#[derive(Debug, Clone)]
pub struct EditProposal {
    pub id: String,
    pub task: String,
    pub changes: Vec<FileChange>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl EditProposal {
    pub fn new(id: String, task: String) -> Self {
        Self { id, task, changes: Vec::new(), created_at: chrono::Utc::now() }
    }
    pub fn add_change(&mut self, change: FileChange) { self.changes.push(change); }
    pub fn total_changes(&self) -> usize { self.changes.len() }
    pub fn files_created(&self) -> usize { self.changes.iter().filter(|c| c.change_type == ChangeType::Create).count() }
    pub fn files_modified(&self) -> usize { self.changes.iter().filter(|c| c.change_type == ChangeType::Modify).count() }
    pub fn files_deleted(&self) -> usize { self.changes.iter().filter(|c| c.change_type == ChangeType::Delete).count() }
    pub fn summary(&self) -> String {
        format!(
            "{} 变更（+{}/-{} 行）| 新建:{} 修改:{} 删除:{}",
            self.total_changes(),
            self.changes.iter().map(|c| c.added_lines).sum::<usize>(),
            self.changes.iter().map(|c| c.removed_lines).sum::<usize>(),
            self.files_created(), self.files_modified(), self.files_deleted(),
        )
    }
}

/// 编辑会话
pub struct EditSession {
    pending: Option<EditProposal>,
    history: Vec<EditProposal>,
    workdir: PathBuf,
    expiry_secs: u64,
}

impl EditSession {
    pub fn new(workdir: PathBuf) -> Self {
        Self { pending: None, history: Vec::new(), workdir, expiry_secs: 300 }
    }

    pub fn set_pending(&mut self, proposal: EditProposal) { self.pending = Some(proposal); }
    pub fn pending(&self) -> Option<&EditProposal> { self.pending.as_ref() }

    fn is_pending_expired(&self) -> bool {
        self.pending.as_ref()
            .map(|p| (chrono::Utc::now() - p.created_at).num_seconds() as u64 > self.expiry_secs)
            .unwrap_or(false)
    }

    pub fn clear_expired(&mut self) {
        if self.is_pending_expired() { self.pending = None; }
    }

    pub async fn apply(&mut self) -> anyhow::Result<ApplyResult> {
        let proposal = self.pending.take()
            .ok_or_else(|| anyhow::anyhow!("没有待确认的变更"))?;
        if self.is_pending_expired() { return Err(anyhow::anyhow!("变更已过期")); }

        let total = proposal.total_changes();
        let mut applied = Vec::new();
        let mut failed = Vec::new();

        for change in &proposal.changes {
            match self.apply_change(change).await {
                Ok(path) => applied.push(path),
                Err(e) => failed.push((change.path.clone(), e.to_string())),
            }
        }
        self.history.push(proposal);
        Ok(ApplyResult { applied, failed, total })
    }

    pub fn reject(&mut self) { self.pending = None; }
    pub fn history(&self) -> &[EditProposal] { &self.history }

    async fn apply_change(&self, change: &FileChange) -> anyhow::Result<String> {
        use tokio::fs;
        match change.change_type {
            ChangeType::Create | ChangeType::Modify => {
                let content = change.new_content.as_ref().ok_or_else(|| anyhow::anyhow!("新内容为空"))?;
                let path = self.workdir.join(&change.path);
                if let Some(parent) = path.parent() { fs::create_dir_all(parent).await?; }
                fs::write(&path, content).await?;
                Ok(change.path.clone())
            }
            ChangeType::Delete => {
                tokio::fs::remove_file(self.workdir.join(&change.path)).await?;
                Ok(change.path.clone())
            }
            ChangeType::Rename => Ok(change.path.clone()),
        }
    }
}

/// 应用结果
#[derive(Debug)]
pub struct ApplyResult {
    pub applied: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub total: usize,
}

impl ApplyResult {
    pub fn is_full_success(&self) -> bool { self.failed.is_empty() && self.applied.len() == self.total }
    pub fn summary(&self) -> String {
        if self.failed.is_empty() {
            format!("✅ 全部 {} 个变更已应用", self.applied.len())
        } else {
            format!("⚠️ {}/{} 变更已应用，{} 个失败", self.applied.len(), self.total, self.failed.len())
        }
    }
}

// ── Diff 计算 ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct DiffResult { pub hunks: Vec<DiffHunk> }

impl DiffResult {
    pub fn render(&self, old_path: &str, new_path: &str) -> String {
        let mut out = format!("--- a/{}\n+++ b/{}\n", old_path, new_path);
        for hunk in &self.hunks {
            match hunk.hunk_type {
                DiffHunkType::Add => { for line in &hunk.lines { out.push_str(&format!("+{}\n", line)); } }
                DiffHunkType::Delete => { for line in &hunk.lines { out.push_str(&format!("-{}\n", line)); } }
            }
        }
        out
    }
}

#[derive(Debug)]
pub struct DiffHunk {
    pub hunk_type: DiffHunkType,
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum DiffHunkType { Add, Delete }

/// 计算两个文本的行级 diff
pub fn compute_line_diff(old_text: &str, new_text: &str) -> DiffResult {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();
    let olen = old_lines.len();
    let nlen = new_lines.len();

    let mut dp = vec![vec![0usize; nlen + 1]; olen + 1];
    for i in 1..=olen {
        for j in 1..=nlen {
            if old_lines[i-1] == new_lines[j-1] { dp[i][j] = dp[i-1][j-1] + 1; }
            else { dp[i][j] = dp[i-1][j].max(dp[i][j-1]); }
        }
    }

    let mut segs = Vec::new();
    let mut i = olen;
    let mut j = nlen;
    while i > 0 && j > 0 {
        if old_lines[i-1] == new_lines[j-1] { segs.push((i, j)); i -= 1; j -= 1; }
        else if dp[i-1][j] > dp[i][j-1] { i -= 1; }
        else { j -= 1; }
    }
    segs.reverse();

    let mut hunks = Vec::new();
    let mut oi = 0usize;
    let mut ni = 0usize;
    for (end_i, end_j) in segs {
        if oi < end_i - 1 {
            let deleted: Vec<String> = old_lines[oi..end_i-1].iter().map(|s| (*s).to_string()).collect();
            if !deleted.is_empty() {
                hunks.push(DiffHunk { hunk_type: DiffHunkType::Delete, old_start: oi+1, new_start: ni+1, lines: deleted });
            }
        }
        if ni < end_j - 1 {
            let added: Vec<String> = new_lines[ni..end_j-1].iter().map(|s| (*s).to_string()).collect();
            if !added.is_empty() {
                hunks.push(DiffHunk { hunk_type: DiffHunkType::Add, old_start: oi+1, new_start: ni+1, lines: added });
            }
        }
        oi = end_i - 1;
        ni = end_j - 1;
    }

    DiffResult { hunks }
}

// ── 全局会话管理 ────────────────────────────────────────────────────────────

pub struct EditSessionManager {
    sessions: HashMap<String, Arc<RwLock<EditSession>>>,
}

impl EditSessionManager {
    pub fn new() -> Self { Self { sessions: HashMap::new() } }
    pub fn get_or_create(&mut self, workdir: &str) -> Arc<RwLock<EditSession>> {
        self.sessions.entry(workdir.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(EditSession::new(PathBuf::from(workdir)))))
            .clone()
    }
    pub async fn clear_expired(&self, workdir: &str) {
        if let Some(s) = self.sessions.get(workdir) { s.write().await.clear_expired(); }
    }
}

impl Default for EditSessionManager { fn default() -> Self { Self::new() } }

// ── 批量重命名 ──────────────────────────────────────────────────────────────

/// 重命名规则
#[derive(Debug, Clone)]
pub enum RenameRule {
    Exact { from: String, to: String },
    Regex { pattern: String, replacement: String },
    Glob { from: String, to: String },
    Case { style: CaseStyle },
    Prefix { prefix: String, start: usize, padding: usize },
    Suffix { suffix: String, start: usize, padding: usize },
    Depth { levels: i32 },
}

/// 大小写风格
#[derive(Debug, Clone, Copy)]
pub enum CaseStyle { CamelCase, PascalCase, SnakeCase, SCREAMING_SNAKE, KebabCase }

impl CaseStyle {
    pub fn apply(&self, s: &str) -> String {
        let re = regex::Regex::new(r"[-_\s]+").unwrap();
        let normalized = re.replace_all(s, "_").to_string();
        let words: Vec<String> = normalized.split('_').filter(|w| !w.is_empty())
            .map(|w| {
                let mut cs = w.chars();
                match self {
                    CaseStyle::CamelCase => {
                        let f = cs.next().map(|c| c.to_lowercase().to_string()).unwrap_or_default();
                        format!("{}{}", f, cs.as_str())
                    }
                    CaseStyle::PascalCase => {
                        let f = cs.next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                        format!("{}{}", f, cs.as_str())
                    }
                    CaseStyle::SnakeCase => w.to_lowercase(),
                    CaseStyle::SCREAMING_SNAKE => w.to_uppercase(),
                    CaseStyle::KebabCase => w.to_lowercase(),
                }
            }).collect();
        match self {
            CaseStyle::SnakeCase => words.join("_"),
            CaseStyle::KebabCase => words.join("-"),
            CaseStyle::SCREAMING_SNAKE => words.join("_"),
            CaseStyle::CamelCase => words.join(""),
            CaseStyle::PascalCase => words.join(""),
        }
    }
}

/// 分割文件名和扩展名（返回 (name, ".ext")）
fn split_ext(file: &str) -> (&str, &str) {
    if let Some(pos) = file.rfind('.') {
        if pos > 0 { return (&file[..pos], &file[pos..]); }
    }
    (file, "")
}

/// 预览批量重命名
pub fn preview_rename(files: &[String], rule: &RenameRule) -> HashMap<String, String> {
    let mut results = HashMap::new();
    for (idx, file) in files.iter().enumerate() {
        let new_name = apply_rename_rule(file, rule, idx);
        if new_name != *file {
            results.insert(file.clone(), new_name);
        }
    }
    results
}

fn apply_rename_rule(file: &str, rule: &RenameRule, index: usize) -> String {
    match rule {
        RenameRule::Exact { from, to } => file.replace(from, to),

        RenameRule::Regex { pattern, replacement } => {
            regex::Regex::new(pattern).ok()
                .map(|re| re.replace_all(file, replacement.as_str()).to_string())
                .unwrap_or_else(|| file.to_string())
        }

        RenameRule::Glob { from, to } => {
            let re_pattern = from.replace('*', ".*").replace('?', ".");
            regex::Regex::new(&re_pattern).ok()
                .filter(|re| re.is_match(file))
                .map(|re| re.replace(file, to.as_str()).to_string())
                .unwrap_or_else(|| file.to_string())
        }

        RenameRule::Case { style } => {
            let (dir, name_ext) = if let Some(slash) = file.rfind('/') {
                (&file[..=slash], &file[slash + 1..])
            } else {
                ("", file)
            };
            let (name, ext) = split_ext(name_ext);
            format!("{}{}{}", dir, style.apply(name), ext)
        }

        RenameRule::Prefix { prefix, start, padding } => {
            let num = start + index;
            let num_str = format!("{:0>width$}", num, width=*padding);
            if let Some(slash) = file.rfind('/') {
                let dir = &file[..=slash];
                format!("{}{}{}", dir, num_str, &file[slash + 1..])
            } else {
                format!("{}{}", num_str, file)
            }
        }

        RenameRule::Suffix { suffix, start, padding } => {
            let num = start + index;
            let num_str = format!("{:0>width$}", num, width=*padding);
            let (stem, ext) = split_ext(file);
            format!("{}-{}-{}{}", stem, num_str, suffix, ext)
        }

        RenameRule::Depth { levels } => {
            if *levels > 0 {
                let parts: Vec<&str> = file.split('/').collect();
                if parts.len() >= 2 {
                    let mut new_parts = parts[..parts.len() - 1].to_vec();
                    for _ in 0..*levels as usize {
                        if new_parts.len() >= 2 {
                            new_parts.insert(new_parts.len() - 1, "nested");
                        }
                    }
                    new_parts.push(parts.last().unwrap());
                    new_parts.join("/")
                } else { file.to_string() }
            } else {
                let skip = levels.unsigned_abs() as usize;
                file.split('/').skip(skip).collect::<Vec<_>>().join("/")
            }
        }
    }
}

/// 执行批量重命名
pub async fn execute_batch_rename(
    workdir: &PathBuf,
    rename_map: &HashMap<String, String>,
) -> anyhow::Result<Vec<(String, String)>> {
    use tokio::fs;
    let mut results = Vec::new();
    let mut by_dir: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (old, new) in rename_map {
        let dir = old.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        by_dir.entry(dir.to_string()).or_default().push((old.clone(), new.clone()));
    }
    for (dir, pairs) in by_dir {
        for (old, new) in pairs {
            let old_path = workdir.join(&old);
            let tmp_path = workdir.join(format!("{}.tmp_rename", old));
            fs::rename(&old_path, &tmp_path).await?;
            fs::rename(&tmp_path, &workdir.join(&new)).await?;
            results.push((old, new));
        }
    }
    Ok(results)
}
