//! Git Merge Conflict 自动检测与解决

use crate::llm::{Client, LlmClientTrait, LlmRequest, Message, MessageRole};
use crate::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

const CONFLICT_START: &str = "<<<<<<< HEAD";
const CONFLICT_MID: &str = "=======";
const CONFLICT_END: &str = ">>>>>>> ";

/// Merge Conflict 信息
#[derive(Debug, Clone)]
pub struct MergeConflict {
    pub file: String,
    pub hunks: Vec<ConflictHunk>,
}

/// 冲突块
#[derive(Debug, Clone)]
pub struct ConflictHunk {
    pub hunk_id: usize,
    pub our_content: String,
    pub their_content: String,
    pub base_content: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
}

/// 冲突解决方案
#[derive(Debug, Clone)]
pub struct ConflictSolution {
    pub file: String,
    pub resolved_content: String,
    pub resolution_type: ResolutionType,
    pub ai_confidence: f32,
}

#[derive(Debug, Clone)]
pub enum ResolutionType {
    KeepOurs,
    KeepTheirs,
    AcceptBoth,
    AiGenerated { prompt: String },
}

/// Merge Conflict 分析器
pub struct MergeConflictAnalyzer {
    llm_client: Arc<dyn LlmClientTrait>,
}

impl MergeConflictAnalyzer {
    /// 创建分析器
    pub fn new(llm_client: Arc<dyn LlmClientTrait>) -> Self {
        Self { llm_client }
    }

    /// 从 Client 创建
    pub fn from_client(client: Client) -> Self {
        Self {
            llm_client: Arc::new(client) as Arc<dyn crate::llm::LlmClientTrait>,
        }
    }

    /// 检测文件中的冲突标记
    pub fn detect_conflicts(&self, content: &str, file: &str) -> Vec<MergeConflict> {
        let mut conflicts = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut hunks = Vec::new();
        let mut hunk_id = 0;
        let mut in_conflict = false;
        let mut conflict_start = 0;
        let mut our_lines = Vec::new();
        let mut their_lines = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if line.starts_with(CONFLICT_START) {
                in_conflict = true;
                conflict_start = idx;
                our_lines.clear();
                their_lines.clear();
            } else if line.starts_with(CONFLICT_MID) && in_conflict {
                // 切换到 their 部分
            } else if line.starts_with(CONFLICT_END) && in_conflict {
                // 结束冲突块
                in_conflict = false;
                hunk_id += 1;
                hunks.push(ConflictHunk {
                    hunk_id,
                    our_content: our_lines.join("\n"),
                    their_content: their_lines.join("\n"),
                    base_content: None,
                    start_line: conflict_start + 1,
                    end_line: idx + 1,
                });
            } else if in_conflict {
                // 根据当前状态决定添加到哪边
                if their_lines.is_empty() && !line.starts_with(CONFLICT_END) {
                    our_lines.push(*line);
                } else if !line.starts_with(CONFLICT_END) {
                    their_lines.push(*line);
                }
            }
        }

        if !hunks.is_empty() {
            conflicts.push(MergeConflict {
                file: file.to_string(),
                hunks,
            });
        }

        conflicts
    }

    /// 从原始内容中提取冲突
    pub fn detect_conflicts_in_text(&self, text: &str) -> Vec<ConflictHunk> {
        let mut hunks = Vec::new();
        let hunk_re = Regex::new(r"(?s)(<<<<<<< HEAD\r?\n)(.*?)(\r?\n=======\r?\n)(.*?)(\r?\n>>>>>>> .*)").unwrap();

        for (hunk_id, caps) in hunk_re.captures_iter(text).enumerate() {
            hunks.push(ConflictHunk {
                hunk_id: hunk_id + 1,
                our_content: caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
                their_content: caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default(),
                base_content: None,
                start_line: text[..caps.get(0).map(|m| m.start()).unwrap_or(0)]
                    .lines()
                    .count()
                    + 1,
                end_line: text[..caps.get(0).map(|m| m.end()).unwrap_or(0)]
                    .lines()
                    .count(),
            });
        }

        hunks
    }

    /// 分析冲突并生成解决方案
    pub async fn analyze_and_resolve(
        &self,
        conflict: &MergeConflict,
    ) -> Result<ConflictSolution> {
        let file = conflict.file.clone();

        // 对每个 hunk 进行分析和合并
        let mut resolved_parts = Vec::new();
        let mut total_confidence = 0.0f32;

        for hunk in &conflict.hunks {
            let resolved = self.resolve_hunk(hunk).await?;
            total_confidence += resolved.1;
            resolved_parts.push(resolved.0);
        }

        let avg_confidence = if !conflict.hunks.is_empty() {
            total_confidence / conflict.hunks.len() as f32
        } else {
            0.0
        };

        Ok(ConflictSolution {
            file,
            resolved_content: resolved_parts.join("\n\n"),
            resolution_type: ResolutionType::AiGenerated {
                prompt: "Merged via ACoder AI".to_string(),
            },
            ai_confidence: avg_confidence,
        })
    }

    /// 解决单个冲突块
    async fn resolve_hunk(&self, hunk: &ConflictHunk) -> Result<(String, f32)> {
        let prompt = format!(
            r#"You are resolving a git merge conflict. Analyze both versions and produce a clean merged result.

## Our version (HEAD / current branch):
```
{}
```

## Their version (incoming changes):
```
{}
```

## Guidelines:
1. Preserve both sets of changes if they are non-overlapping
2. If changes conflict, prefer the more recent commit
3. Keep code style consistent with the existing codebase
4. Do NOT include any conflict markers (<<<<<<<, =======, >>>>>>>) in your output
5. Only output the merged code, no explanations

Output format: Return ONLY the merged code, nothing else."#,
            hunk.our_content,
            hunk.their_content
        );

        let response = self
            .llm_client
            .complete(LlmRequest {
                model: "default".to_string(),
                messages: vec![Message {
                    role: MessageRole::User,
                    content: prompt,
                    name: None,
                    tool_call_id: None,
                }],
                temperature: Some(0.3),
                max_tokens: Some(4096),
                stream: false,
                tools: None,
            })
            .await?;

        let resolved_content = response.content.trim().to_string();

        // 评估置信度（基于响应质量）
        let confidence = if resolved_content.contains("<<<<<<<") || resolved_content.contains(">>>>>>>") {
            0.3 // 包含冲突标记，置信度低
        } else if resolved_content.is_empty() {
            0.5
        } else {
            0.85
        };

        Ok((resolved_content, confidence))
    }

    /// 解决所有冲突
    pub async fn resolve_all(&self, conflicts: Vec<MergeConflict>) -> Vec<ConflictSolution> {
        let mut solutions = Vec::new();

        for conflict in conflicts {
            match self.analyze_and_resolve(&conflict).await {
                Ok(solution) => solutions.push(solution),
                Err(e) => {
                    tracing::error!("Failed to resolve conflicts in {}: {}", conflict.file, e);
                    // 降级：保留 our 版本
                    solutions.push(ConflictSolution {
                        file: conflict.file,
                        resolved_content: conflict
                            .hunks
                            .iter()
                            .map(|h| h.our_content.clone())
                            .collect::<Vec<_>>()
                            .join("\n\n"),
                        resolution_type: ResolutionType::KeepOurs,
                        ai_confidence: 0.0,
                    });
                }
            }
        }

        solutions
    }

    /// 生成合并建议（供人工审核）
    pub fn generate_review_suggestion(&self, conflict: &MergeConflict) -> String {
        let mut suggestion = format!(
            "Merge Conflict Review — {}\n\
             ================================\n\
             {} conflict hunk(s) detected.\n\n",
            conflict.file,
            conflict.hunks.len()
        );

        for hunk in &conflict.hunks {
            suggestion.push_str(&format!(
                "--- Hunk #{} (lines {}-{}) ---\n\
                 ## Our version:\n{}\n\n\
                 ## Their version:\n{}\n\n",
                hunk.hunk_id,
                hunk.start_line,
                hunk.end_line,
                hunk.our_content,
                hunk.their_content,
            ));
        }

        suggestion.push_str("Suggested action: Run `acode merge resolve --file <file>` to auto-resolve with AI.\n");
        suggestion
    }

    /// 检查内容是否包含冲突标记
    pub fn has_conflicts(&self, content: &str) -> bool {
        content.contains(CONFLICT_START) && content.contains(CONFLICT_END)
    }

    /// 尝试通过 git 命令获取 base 版本
    pub async fn get_base_content(&self, file: &str, our_ref: &str, their_ref: &str) -> Result<Option<String>> {
        let output = tokio::process::Command::new("git")
            .args(["merge-base", our_ref, their_ref])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(None);
        }

        let base_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if base_sha.is_empty() {
            return Ok(None);
        }

        let output = tokio::process::Command::new("git")
            .args(["show", &format!("{}:{}", base_sha, file)])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(None);
        }

        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    }

    /// 将冲突内容写入文件
    pub async fn apply_solution(&self, solution: &ConflictSolution) -> Result<()> {
        tokio::fs::write(&solution.file, &solution.resolved_content).await?;
        tracing::info!(
            "Applied conflict solution to {} (confidence: {:.0}%, type: {:?})",
            solution.file,
            solution.ai_confidence * 100.0,
            solution.resolution_type
        );
        Ok(())
    }
}

impl Default for MergeConflictAnalyzer {
    fn default() -> Self {
        panic!("MergeConflictAnalyzer requires an LLM client, use new() or from_client()")
    }
}
