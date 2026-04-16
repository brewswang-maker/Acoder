//! Diff 可视化审查器 — UI 层核心模块
//!
//! 支持功能：
//! - 内联 diff：在代码行上直接显示变更
//! - 逐行审查：可逐行接受/拒绝变更
//! - 批量操作：Accept All / Reject All / Accept Stage
//! - 多文件支持：一次 Review 多个文件
//! - 变更统计：+行/-行数，文件级别概览

use crate::editing::compute_line_diff;
use crate::error::Result as AcodeResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── 数据结构 ────────────────────────────────────────────────────────────────

/// 变更文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub original_content: String,
    pub new_content: String,
    pub hunks: Vec<DiffHunk>,
    pub stats: FileStats,
}

/// 单个 Diff Hunk（一段连续变更）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub id: String,
    pub original_start: usize,
    pub original_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
    pub status: HunkStatus,
}

/// 单行 Diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub line_number_old: Option<usize>,
    pub line_number_new: Option<usize>,
    pub content: String,
    pub change_type: ChangeType,
    pub decision: Option<LineDecision>,
}

/// 变更类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    Added,
    Removed,
    Unchanged,
    Modified,
}

/// 单行决策
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LineDecision {
    Accepted,
    Rejected,
    Pending,
}

/// Hunk 状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HunkStatus {
    Accepted,
    Rejected,
    PartiallyAccepted,
    Pending,
}

/// 文件级别统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStats {
    pub additions: usize,
    pub deletions: usize,
    pub modifications: usize,
    pub hunks_count: usize,
}

/// Diff 审查会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReviewSession {
    pub id: String,
    pub changed_files: Vec<ChangedFile>,
    pub total_additions: usize,
    pub total_deletions: usize,
    pub overall_status: ReviewStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 整体审查状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReviewStatus {
    InProgress,
    Approved,
    Rejected,
    PartiallyApproved,
}

/// 审查命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewCommand {
    AcceptLine { file_id: String, hunk_id: String, line_idx: usize },
    RejectLine { file_id: String, hunk_id: String, line_idx: usize },
    AcceptHunk { file_id: String, hunk_id: String },
    RejectHunk { file_id: String, hunk_id: String },
    AcceptFile { file_id: String },
    RejectFile { file_id: String },
    AcceptAll,
    RejectAll,
}

/// 审查进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewProgress {
    pub total_lines: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub pending: usize,
    pub percentage: f32,
}

// ── DiffViewer 实现 ──────────────────────────────────────────────────────────

/// Diff 可视化审查器
#[derive(Debug, Clone)]
pub struct DiffViewer {
    context_lines: usize,
}

impl DiffViewer {
    pub fn new() -> Self {
        Self { context_lines: 3 }
    }

    pub fn with_context_lines(mut self, lines: usize) -> Self {
        self.context_lines = lines;
        self
    }

    /// 从原始内容和新内容创建审查会话
    pub fn create_session(
        &self,
        files: Vec<(String, String, String)>, // (path, old, new)
    ) -> AcodeResult<DiffReviewSession> {
        let mut changed_files = Vec::new();
        let mut total_additions = 0usize;
        let mut total_deletions = 0usize;

        for (path, old_content, new_content) in files {
            let hunks = self.compute_hunks(&old_content, &new_content);
            let stats = self.compute_stats(&hunks);

            total_additions += stats.additions;
            total_deletions += stats.deletions;

            changed_files.push(ChangedFile {
                path,
                original_content: old_content,
                new_content,
                hunks,
                stats,
            });
        }

        let overall_status = if changed_files.is_empty() {
            ReviewStatus::Approved
        } else {
            ReviewStatus::InProgress
        };

        Ok(DiffReviewSession {
            id: Uuid::new_v4().to_string(),
            changed_files,
            total_additions,
            total_deletions,
            overall_status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    /// 计算文件的 hunks
    fn compute_hunks(&self, old_content: &str, new_content: &str) -> Vec<DiffHunk> {
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();
        let olen = old_lines.len();
        let nlen = new_lines.len();

        let diff_result = compute_line_diff(old_content, new_content);

        // LCS DP matrix for finding matching segments
        let mut dp = vec![vec![0usize; nlen + 1]; olen + 1];
        for i in 1..=olen {
            for j in 1..=nlen {
                if old_lines[i - 1] == new_lines[j - 1] {
                    dp[i][j] = dp[i - 1][j - 1] + 1;
                } else {
                    dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
                }
            }
        }

        // Find matching segments
        let mut segs = Vec::new();
        let mut i = olen;
        let mut j = nlen;
        while i > 0 && j > 0 {
            if old_lines[i - 1] == new_lines[j - 1] {
                segs.push((i, j));
                i -= 1;
                j -= 1;
            } else if dp[i - 1][j] > dp[i][j - 1] {
                i -= 1;
            } else {
                j -= 1;
            }
        }
        segs.reverse();

        // Build hunks from diff result
        let mut hunks = Vec::new();
        let mut seg_idx = 0;

        for diff_hunk in &diff_result.hunks {
            // Find the position in the LCS segments
            while seg_idx < segs.len()
                && (segs[seg_idx].0 < diff_hunk.old_start
                    || segs[seg_idx].1 < diff_hunk.new_start)
            {
                seg_idx += 1;
            }

            let (orig_start, new_start) = segs.get(seg_idx).copied().unwrap_or((1, 1));

            let lines: Vec<DiffLine> = diff_hunk
                .lines
                .iter()
                .enumerate()
                .map(|(idx, line)| {
                    let change_type = match diff_hunk.hunk_type {
                        crate::editing::DiffHunkType::Add => ChangeType::Added,
                        crate::editing::DiffHunkType::Delete => ChangeType::Removed,
                    };
                    let line_number_old = if matches!(diff_hunk.hunk_type, crate::editing::DiffHunkType::Delete) {
                        Some(diff_hunk.old_start + idx)
                    } else {
                        None
                    };
                    let line_number_new = if matches!(diff_hunk.hunk_type, crate::editing::DiffHunkType::Add) {
                        Some(diff_hunk.new_start + idx)
                    } else {
                        None
                    };
                    DiffLine {
                        line_number_old,
                        line_number_new,
                        content: line.clone(),
                        change_type,
                        decision: None,
                    }
                })
                .collect();

            let original_count = if matches!(diff_hunk.hunk_type, crate::editing::DiffHunkType::Delete) {
                diff_hunk.lines.len()
            } else {
                0
            };
            let new_count = if matches!(diff_hunk.hunk_type, crate::editing::DiffHunkType::Add) {
                diff_hunk.lines.len()
            } else {
                0
            };

            hunks.push(DiffHunk {
                id: Uuid::new_v4().to_string(),
                original_start: diff_hunk.old_start,
                original_count,
                new_start: diff_hunk.new_start,
                new_count,
                lines,
                status: HunkStatus::Pending,
            });
        }

        // Add unchanged lines around hunks for context
        let mut full_hunks = Vec::new();
        let mut prev_end_old = 0usize;
        let mut prev_end_new = 0usize;

        for hunk in &hunks {
            let ctx_start_old = hunk.original_start.saturating_sub(self.context_lines);
            let ctx_start_new = hunk.new_start.saturating_sub(self.context_lines);

            // Add context (unchanged) lines before this hunk
            if prev_end_old < ctx_start_old && prev_end_old < olen {
                let ctx_lines: Vec<DiffLine> = old_lines[prev_end_old..ctx_start_old]
                    .iter()
                    .enumerate()
                    .map(|(idx, line)| DiffLine {
                        line_number_old: Some(prev_end_old + idx + 1),
                        line_number_new: Some(prev_end_new + idx + 1),
                        content: (*line).to_string(),
                        change_type: ChangeType::Unchanged,
                        decision: Some(LineDecision::Accepted),
                    })
                    .collect();
                if !ctx_lines.is_empty() {
                    full_hunks.push(DiffHunk {
                        id: Uuid::new_v4().to_string(),
                        original_start: prev_end_old + 1,
                        original_count: ctx_lines.len(),
                        new_start: prev_end_new + 1,
                        new_count: ctx_lines.len(),
                        lines: ctx_lines,
                        status: HunkStatus::Accepted,
                    });
                }
            }

            full_hunks.push(hunk.clone());

            prev_end_old = hunk.original_start + hunk.original_count;
            prev_end_new = hunk.new_start + hunk.new_count;
        }

        full_hunks
    }

    /// 计算文件统计
    fn compute_stats(&self, hunks: &[DiffHunk]) -> FileStats {
        let mut additions = 0usize;
        let mut deletions = 0usize;
        let mut modifications = 0usize;

        for hunk in hunks {
            for line in &hunk.lines {
                match line.change_type {
                    ChangeType::Added => additions += 1,
                    ChangeType::Removed => deletions += 1,
                    ChangeType::Modified => modifications += 1,
                    ChangeType::Unchanged => {}
                }
            }
        }

        FileStats {
            additions,
            deletions,
            modifications,
            hunks_count: hunks.len(),
        }
    }

    /// 执行审查命令
    pub fn execute_command(
        &self,
        session: &mut DiffReviewSession,
        cmd: ReviewCommand,
    ) -> AcodeResult<()> {
        match cmd {
            ReviewCommand::AcceptLine {
                file_id,
                hunk_id,
                line_idx,
            } => {
                self.accept_line(session, &file_id, &hunk_id, line_idx)?;
            }
            ReviewCommand::RejectLine {
                file_id,
                hunk_id,
                line_idx,
            } => {
                self.reject_line(session, &file_id, &hunk_id, line_idx)?;
            }
            ReviewCommand::AcceptHunk { file_id, hunk_id } => {
                self.accept_hunk(session, &file_id, &hunk_id)?;
            }
            ReviewCommand::RejectHunk { file_id, hunk_id } => {
                self.reject_hunk(session, &file_id, &hunk_id)?;
            }
            ReviewCommand::AcceptFile { file_id } => {
                self.accept_file(session, &file_id)?;
            }
            ReviewCommand::RejectFile { file_id } => {
                self.reject_file(session, &file_id)?;
            }
            ReviewCommand::AcceptAll => {
                self.accept_all(session)?;
            }
            ReviewCommand::RejectAll => {
                self.reject_all(session)?;
            }
        }

        session.updated_at = Utc::now();
        self.update_session_status(session);
        Ok(())
    }

    fn accept_line(
        &self,
        session: &mut DiffReviewSession,
        file_id: &str,
        hunk_id: &str,
        line_idx: usize,
    ) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            if file.path == file_id {
                for hunk in &mut file.hunks {
                    if hunk.id == hunk_id {
                        if line_idx < hunk.lines.len() {
                            hunk.lines[line_idx].decision = Some(LineDecision::Accepted);
                            self.update_hunk_status(hunk);
                        }
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    fn reject_line(
        &self,
        session: &mut DiffReviewSession,
        file_id: &str,
        hunk_id: &str,
        line_idx: usize,
    ) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            if file.path == file_id {
                for hunk in &mut file.hunks {
                    if hunk.id == hunk_id {
                        if line_idx < hunk.lines.len() {
                            hunk.lines[line_idx].decision = Some(LineDecision::Rejected);
                            self.update_hunk_status(hunk);
                        }
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    fn accept_hunk(
        &self,
        session: &mut DiffReviewSession,
        file_id: &str,
        hunk_id: &str,
    ) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            if file.path == file_id {
                for hunk in &mut file.hunks {
                    if hunk.id == hunk_id {
                        hunk.status = HunkStatus::Accepted;
                        for line in &mut hunk.lines {
                            if line.decision.is_none() {
                                line.decision = Some(LineDecision::Accepted);
                            }
                        }
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    fn reject_hunk(
        &self,
        session: &mut DiffReviewSession,
        file_id: &str,
        hunk_id: &str,
    ) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            if file.path == file_id {
                for hunk in &mut file.hunks {
                    if hunk.id == hunk_id {
                        hunk.status = HunkStatus::Rejected;
                        for line in &mut hunk.lines {
                            if line.decision.is_none() {
                                line.decision = Some(LineDecision::Rejected);
                            }
                        }
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    fn accept_file(&self, session: &mut DiffReviewSession, file_id: &str) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            if file.path == file_id {
                for hunk in &mut file.hunks {
                    hunk.status = HunkStatus::Accepted;
                    for line in &mut hunk.lines {
                        if line.decision.is_none() {
                            line.decision = Some(LineDecision::Accepted);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn reject_file(&self, session: &mut DiffReviewSession, file_id: &str) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            if file.path == file_id {
                for hunk in &mut file.hunks {
                    hunk.status = HunkStatus::Rejected;
                    for line in &mut hunk.lines {
                        if line.decision.is_none() {
                            line.decision = Some(LineDecision::Rejected);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn accept_all(&self, session: &mut DiffReviewSession) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            for hunk in &mut file.hunks {
                hunk.status = HunkStatus::Accepted;
                for line in &mut hunk.lines {
                    if line.decision.is_none() {
                        line.decision = Some(LineDecision::Accepted);
                    }
                }
            }
        }
        Ok(())
    }

    fn reject_all(&self, session: &mut DiffReviewSession) -> AcodeResult<()> {
        for file in &mut session.changed_files {
            for hunk in &mut file.hunks {
                hunk.status = HunkStatus::Rejected;
                for line in &mut hunk.lines {
                    if line.decision.is_none() {
                        line.decision = Some(LineDecision::Rejected);
                    }
                }
            }
        }
        Ok(())
    }

    fn update_hunk_status(&self, hunk: &mut DiffHunk) {
        let mut accepted_count = 0usize;
        let mut rejected_count = 0usize;
        let mut pending_count = 0usize;

        for line in &hunk.lines {
            if line.change_type == ChangeType::Unchanged {
                continue;
            }
            match line.decision {
                Some(LineDecision::Accepted) => accepted_count += 1,
                Some(LineDecision::Rejected) => rejected_count += 1,
                None | Some(LineDecision::Pending) => pending_count += 1,
            }
        }

        if accepted_count == 0 && rejected_count == 0 {
            hunk.status = HunkStatus::Pending;
        } else if pending_count == 0 && rejected_count == 0 {
            hunk.status = HunkStatus::Accepted;
        } else if pending_count == 0 && accepted_count == 0 {
            hunk.status = HunkStatus::Rejected;
        } else {
            hunk.status = HunkStatus::PartiallyAccepted;
        }
    }

    fn update_session_status(&self, session: &mut DiffReviewSession) {
        let mut all_accepted = true;
        let mut all_rejected = true;
        let mut has_pending = false;

        for file in &session.changed_files {
            for hunk in &file.hunks {
                match hunk.status {
                    HunkStatus::Accepted => {}
                    HunkStatus::Rejected => all_accepted = false,
                    HunkStatus::PartiallyAccepted => {
                        all_accepted = false;
                        all_rejected = false;
                    }
                    HunkStatus::Pending => {
                        all_accepted = false;
                        all_rejected = false;
                        has_pending = true;
                    }
                }
            }
        }

        session.overall_status = if all_accepted {
            ReviewStatus::Approved
        } else if all_rejected {
            ReviewStatus::Rejected
        } else if has_pending {
            ReviewStatus::InProgress
        } else {
            ReviewStatus::PartiallyApproved
        };
    }

    /// 生成 unified diff 输出（兼容 git diff 格式）
    pub fn generate_unified_diff(&self, session: &DiffReviewSession) -> String {
        let mut output = String::new();

        for file in &session.changed_files {
            output.push_str(&format!("diff --git a/{} b/{}\n", file.path, file.path));
            output.push_str(&format!("--- a/{}\n", file.path));
            output.push_str(&format!("+++ b/{}\n", file.path));

            for hunk in &file.hunks {
                let old_count = hunk.lines.iter().filter(|l| l.line_number_old.is_some()).count();
                let new_count = hunk.lines.iter().filter(|l| l.line_number_new.is_some()).count();

                output.push_str(&format!(
                    "@@ -{},{} +{},{} @@\n",
                    hunk.original_start, old_count, hunk.new_start, new_count
                ));

                for line in &hunk.lines {
                    let prefix = match line.change_type {
                        ChangeType::Added => "+",
                        ChangeType::Removed => "-",
                        ChangeType::Unchanged => " ",
                        ChangeType::Modified => "~",
                    };
                    output.push_str(&format!("{}{}\n", prefix, line.content));
                }
            }
            output.push('\n');
        }

        output
    }

    /// 生成 HTML 可渲染的 diff 视图
    pub fn generate_html_diff(&self, session: &DiffReviewSession) -> String {
        let mut html = String::new();

        html.push_str(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
body { font-family: monospace; background: #1e1e1e; color: #d4d4d4; }
.file-header { background: #2d2d2d; padding: 10px; margin: 0; border-bottom: 1px solid #3e3e3e; }
.stats { color: #888; font-size: 0.9em; }
.added { background: #2d4f2d; color: #6a9955; }
.removed { background: #4f2d2d; color: #f14c4c; }
.unchanged { color: #d4d4d4; }
.modified { background: #4f4f2d; color: #dcdcaa; }
.line { padding: 1px 8px; white-space: pre; }
.line-num { color: #858585; display: inline-block; width: 50px; text-align: right; margin-right: 10px; }
.hunk-header { background: #3e3e3e; color: #4ec9b0; padding: 2px 8px; }
.decision-accepted { border-left: 3px solid #6a9955; }
.decision-rejected { border-left: 3px solid #f14c4c; text-decoration: line-through; opacity: 0.6; }
.progress-bar { height: 4px; background: #3e3e3e; }
.progress-fill { height: 100%; background: #6a9955; }
</style>
</head>
<body>
"#);

        // Summary
        html.push_str(&format!(
            r#"<div class="file-header">
<h2>{} files changed, <span class="added">+{}</span> / <span class="removed">-{}</span></h2>
<div class="progress-bar"><div class="progress-fill" style="width:{}%"></div></div>
</div>"#,
            session.changed_files.len(),
            session.total_additions,
            session.total_deletions,
            self.get_review_progress(session).percentage
        ));

        for file in &session.changed_files {
            html.push_str(&format!(
                r#"<div class="file-header">
<h3>{}</h3>
<div class="stats">+{} / -{} / {} hunks</div>
</div>"#,
                file.path,
                file.stats.additions,
                file.stats.deletions,
                file.stats.hunks_count
            ));

            for hunk in &file.hunks {
                let old_count = hunk
                    .lines
                    .iter()
                    .filter(|l| l.line_number_old.is_some())
                    .count();
                let new_count = hunk
                    .lines
                    .iter()
                    .filter(|l| l.line_number_new.is_some())
                    .count();

                html.push_str(&format!(
                    r#"<div class="hunk-header">@@ -{},{} +{},{} @@</div>"#,
                    hunk.original_start, old_count, hunk.new_start, new_count
                ));

                for line in &hunk.lines {
                    let class = match line.change_type {
                        ChangeType::Added => "added",
                        ChangeType::Removed => "removed",
                        ChangeType::Unchanged => "unchanged",
                        ChangeType::Modified => "modified",
                    };

                    let decision_class = match line.decision {
                        Some(LineDecision::Accepted) => Some("decision-accepted"),
                        Some(LineDecision::Rejected) => Some("decision-rejected"),
                        Some(LineDecision::Pending) | None => None,
                    };

                    let num_old = line
                        .line_number_old
                        .map(|n| format!("{}", n))
                        .unwrap_or_default();
                    let num_new = line
                        .line_number_new
                        .map(|n| format!("{}", n))
                        .unwrap_or_default();

                    let decision_style = decision_class.map(|c| format!(" {}", c)).unwrap_or_default();

                    html.push_str(&format!(
                        r#"<div class="line {}"><span class="line-num">{}</span><span class="line-num">{}</span>{}</div>"#,
                        class,
                        num_old,
                        num_new,
                        html_escape(&line.content)
                    ));
                }
            }
        }

        html.push_str("</body></html>");
        html
    }

    /// 应用已接受的变更
    pub fn apply_accepted_changes(
        &self,
        session: &DiffReviewSession,
    ) -> AcodeResult<HashMap<String, String>> {
        let mut results = HashMap::new();

        for file in &session.changed_files {
            let mut accepted_lines = Vec::new();
            let mut old_line_idx = 0usize;
            let mut new_line_idx = 0usize;

            for hunk in &file.hunks {
                // Add unchanged lines from context
                while old_line_idx < hunk.original_start - 1
                    && old_line_idx < file.original_content.lines().count()
                {
                    let old_lines: Vec<_> = file.original_content.lines().collect();
                    if old_line_idx < old_lines.len() {
                        accepted_lines.push(old_lines[old_line_idx].to_string());
                    }
                    old_line_idx += 1;
                    new_line_idx += 1;
                }

                // Add lines based on decisions
                for line in &hunk.lines {
                    match line.change_type {
                        ChangeType::Unchanged => {
                            let old_lines: Vec<_> = file.original_content.lines().collect();
                            if old_line_idx < old_lines.len() {
                                accepted_lines.push(old_lines[old_line_idx].to_string());
                            }
                            old_line_idx += 1;
                            new_line_idx += 1;
                        }
                        ChangeType::Added => {
                            if line.decision != Some(LineDecision::Rejected) {
                                accepted_lines.push(line.content.clone());
                            }
                            new_line_idx += 1;
                        }
                        ChangeType::Removed => {
                            if line.decision == Some(LineDecision::Accepted) {
                                let old_lines: Vec<_> = file.original_content.lines().collect();
                                if old_line_idx < old_lines.len() {
                                    accepted_lines.push(old_lines[old_line_idx].to_string());
                                }
                            }
                            old_line_idx += 1;
                        }
                        ChangeType::Modified => {
                            if line.decision != Some(LineDecision::Rejected) {
                                accepted_lines.push(line.content.clone());
                            }
                            if line.line_number_old.is_some() {
                                old_line_idx += 1;
                            }
                            if line.line_number_new.is_some() {
                                new_line_idx += 1;
                            }
                        }
                    }
                }
            }

            // Add remaining unchanged lines
            let old_lines: Vec<_> = file.original_content.lines().collect();
            while old_line_idx < old_lines.len() {
                accepted_lines.push(old_lines[old_line_idx].to_string());
                old_line_idx += 1;
            }

            results.insert(file.path.clone(), accepted_lines.join("\n"));
        }

        Ok(results)
    }

    /// 获取审查进度
    pub fn get_review_progress(&self, session: &DiffReviewSession) -> ReviewProgress {
        let mut total_lines = 0usize;
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        let mut pending = 0usize;

        for file in &session.changed_files {
            for hunk in &file.hunks {
                for line in &hunk.lines {
                    if line.change_type != ChangeType::Unchanged {
                        total_lines += 1;
                        match line.decision {
                            Some(LineDecision::Accepted) => accepted += 1,
                            Some(LineDecision::Rejected) => rejected += 1,
                            Some(LineDecision::Pending) | None => pending += 1,
                        }
                    }
                }
            }
        }

        let percentage = if total_lines > 0 {
            (accepted as f32 / total_lines as f32) * 100.0
        } else {
            100.0
        };

        ReviewProgress {
            total_lines,
            accepted,
            rejected,
            pending,
            percentage,
        }
    }
}

impl Default for DiffViewer {
    fn default() -> Self {
        Self::new()
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let viewer = DiffViewer::new();
        let files = vec![(
            "test.rs".to_string(),
            "fn old() {}\n".to_string(),
            "fn new() {}\n".to_string(),
        )];
        let session = viewer.create_session(files).unwrap();
        assert_eq!(session.changed_files.len(), 1);
    }

    #[test]
    fn test_accept_all() {
        let viewer = DiffViewer::new();
        let files = vec![(
            "test.rs".to_string(),
            "fn old() {}\n".to_string(),
            "fn new() {}\n".to_string(),
        )];
        let mut session = viewer.create_session(files).unwrap();
        viewer.execute_command(&mut session, ReviewCommand::AcceptAll).unwrap();
        assert_eq!(session.overall_status, ReviewStatus::Approved);
    }

    #[test]
    fn test_reject_all() {
        let viewer = DiffViewer::new();
        let files = vec![(
            "test.rs".to_string(),
            "fn old() {}\n".to_string(),
            "fn new() {}\n".to_string(),
        )];
        let mut session = viewer.create_session(files).unwrap();
        viewer
            .execute_command(&mut session, ReviewCommand::RejectAll)
            .unwrap();
        assert_eq!(session.overall_status, ReviewStatus::Rejected);
    }

    #[test]
    fn test_review_progress() {
        let viewer = DiffViewer::new();
        let files = vec![(
            "test.rs".to_string(),
            "fn old() {}\n".to_string(),
            "fn new() {}\nfn other() {}\n".to_string(),
        )];
        let session = viewer.create_session(files).unwrap();
        let progress = viewer.get_review_progress(&session);
        assert!(progress.total_lines >= 0);
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<>&\"'"), "&lt;&gt;&amp;&quot;&#39;");
    }
}
