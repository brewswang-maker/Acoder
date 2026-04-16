//! Pre-commit Hooks — AI 驱动的代码质量预检

use crate::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Pre-commit 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCommitConfig {
    pub enabled: bool,
    pub ai_review: bool,
    pub auto_format: bool,
    pub auto_lint: bool,
    pub blocked_patterns: Vec<String>,
}

impl Default for PreCommitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ai_review: true,
            auto_format: true,
            auto_lint: true,
            blocked_patterns: vec![
                r#"(?i)(api[_-]?key|secret[_-]?key|access[_-]?token|auth[_-]?token)\s*[=:]\s*['"]\w+['"]"#.to_string(),
                r#"password\s*[=:]\s*['"][^'"]{8,}['"]"#.to_string(),
                r#"-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----"#.to_string(),
            ],
        }
    }
}

/// Pre-commit 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCommitResult {
    pub passed: bool,
    pub checks: Vec<CheckResult>,
    pub ai_comments: Vec<AiComment>,
    pub blocked_files: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub check_name: String,
    pub passed: bool,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiComment {
    pub file: String,
    pub line: usize,
    pub severity: CommentSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommentSeverity {
    Info,
    Warning,
    Error,
}

/// Pre-commit Hook
pub struct PreCommitHook {
    config: PreCommitConfig,
    llm_client: Option<Arc<dyn crate::llm::LlmClientTrait>>,
}

impl PreCommitHook {
    pub fn new(config: PreCommitConfig) -> Self {
        Self {
            config,
            llm_client: None,
        }
    }

    pub fn with_llm(config: PreCommitConfig, llm_client: Arc<dyn crate::llm::LlmClientTrait>) -> Self {
        Self {
            config,
            llm_client: Some(llm_client),
        }
    }

    /// 运行 pre-commit 检查
    pub async fn run(&self, files: Vec<String>) -> PreCommitResult {
        let mut all_checks = Vec::new();
        let mut ai_comments = Vec::new();
        let mut blocked_files = Vec::new();

        for file in &files {
            let content = match tokio::fs::read_to_string(file).await {
                Ok(c) => c,
                Err(e) => {
                    all_checks.push(CheckResult {
                        check_name: "file_read".to_string(),
                        passed: false,
                        message: format!("Failed to read file {}: {}", file, e),
                        file: Some(file.clone()),
                        line: None,
                    });
                    continue;
                }
            };

            // 危险模式检测
            let danger_results = self.check_dangerous_patterns(&content, file);
            for result in danger_results {
                if !result.passed {
                    blocked_files.push(file.clone());
                }
                all_checks.push(result);
            }

            // AI 审查（如果启用且有 LLM 客户端）
            if self.config.ai_review && self.llm_client.is_some() {
                if let Some(comments) = self.ai_review(file, &content).await {
                    ai_comments.extend(comments);
                }
            }
        }

        // 自动格式化
        if self.config.auto_format {
            let fmt_results = self.auto_fix(files.clone()).await;
            all_checks.extend(fmt_results);
        }

        let passed = blocked_files.is_empty()
            && all_checks.iter().all(|c| c.passed);

        let summary = if passed {
            format!("All checks passed ({} checks, {} AI comments)", all_checks.len(), ai_comments.len())
        } else {
            let failures = all_checks.iter().filter(|c| !c.passed).count();
            format!(
                "Failed: {} failures, {} blocked files, {} AI comments",
                failures,
                blocked_files.len(),
                ai_comments.len()
            )
        };

        PreCommitResult {
            passed,
            checks: all_checks,
            ai_comments,
            blocked_files,
            summary,
        }
    }

    /// AI 代码审查
    async fn ai_review(&self, file: &str, content: &str) -> Option<Vec<AiComment>> {
        let client = self.llm_client.as_ref()?;
        let prompt = format!(
            "Review the following code in file '{}' and provide concise feedback. \
            Return a JSON array of comments with fields: file, line (number), severity (info/warning/error), message. \
            Focus on: code quality issues, potential bugs, security concerns, and improvements.\n\n\
            ```\n{}\n```",
            file,
            content.lines().take(100).collect::<Vec<_>>().join("\n")
        );

        let response = client
            .complete(crate::llm::LlmRequest {
                model: "default".to_string(),
                messages: vec![crate::llm::Message {
                    role: crate::llm::MessageRole::User,
                    content: prompt,
                    name: None,
                    tool_call_id: None,
                }],
                temperature: Some(0.3),
                max_tokens: Some(1024),
                stream: false,
                tools: None,
            })
            .await
            .ok()?;

        // 解析 LLM 返回的 JSON 注释（简化处理）
        let comments: Vec<AiComment> = serde_json::from_str(&response.content)
            .unwrap_or_else(|_| {
                // 降级：解析自由文本并提取行号
                content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| {
                        line.contains("TODO") || line.contains("FIXME") || line.contains("BUG")
                    })
                    .map(|(idx, line)| AiComment {
                        file: file.to_string(),
                        line: idx + 1,
                        severity: CommentSeverity::Warning,
                        message: line.trim().to_string(),
                    })
                    .collect()
            });

        Some(comments)
    }

    /// 自动格式化和 lint
    async fn auto_fix(&self, files: Vec<String>) -> Vec<CheckResult> {
        let mut results = Vec::new();

        for file in files {
            let path = Path::new(&file);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            // 检测文件类型并尝试格式化
            match ext {
                "rs" => {
                    // Rustfmt check
                    let output = tokio::process::Command::new("rustfmt")
                        .args(["--check", &file])
                        .output()
                        .await;
                    if let Ok(output) = output {
                        if !output.status.success() {
                            results.push(CheckResult {
                                check_name: "rustfmt".to_string(),
                                passed: false,
                                message: "Run `rustfmt` to format this file".to_string(),
                                file: Some(file),
                                line: None,
                            });
                        }
                    }
                }
                "ts" | "js" | "tsx" | "jsx" => {
                    // npm run format -- --check
                    let output = tokio::process::Command::new("npx")
                        .args(["prettier", "--check", &file])
                        .output()
                        .await;
                    if let Ok(output) = output {
                        if !output.status.success() {
                            results.push(CheckResult {
                                check_name: "prettier".to_string(),
                                passed: false,
                                message: "Run `prettier --write` to format this file".to_string(),
                                file: Some(file),
                                line: None,
                            });
                        }
                    }
                }
                "py" => {
                    // Black check
                    let output = tokio::process::Command::new("black")
                        .args(["--check", &file])
                        .output()
                        .await;
                    if let Ok(output) = output {
                        if !output.status.success() {
                            results.push(CheckResult {
                                check_name: "black".to_string(),
                                passed: false,
                                message: "Run `black` to format this file".to_string(),
                                file: Some(file),
                                line: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        results
    }

    /// 危险模式检测（secret, TODO with sensitive info）
    fn check_dangerous_patterns(&self, content: &str, file: &str) -> Vec<CheckResult> {
        let mut results = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for pattern in &self.config.blocked_patterns {
            if let Ok(re) = Regex::new(pattern) {
                for (idx, line) in lines.iter().enumerate() {
                    if re.is_match(line) {
                        results.push(CheckResult {
                            check_name: "dangerous_pattern".to_string(),
                            passed: false,
                            message: format!(
                                "Blocked pattern detected: potentially exposed secret or credential"
                            ),
                            file: Some(file.to_string()),
                            line: Some(idx + 1),
                        });
                    }
                }
            }
        }

        // 检测大文件
        if content.len() > 500_000 {
            results.push(CheckResult {
                check_name: "file_size".to_string(),
                passed: false,
                message: format!(
                    "File size ({:.1} MB) exceeds 500 KB limit",
                    content.len() as f64 / 1_000_000.0
                ),
                file: Some(file.to_string()),
                line: None,
            });
        }

        // 检测 TODO/FIXME 是否包含敏感上下文（宽松检测）
        for (idx, line) in lines.iter().enumerate() {
            if line.contains("TODO") || line.contains("FIXME") {
                results.push(CheckResult {
                    check_name: "todo_comment".to_string(),
                    passed: true, // 不阻止，但标记
                    message: format!("TODO/FIXME found: {}", line.trim()),
                    file: Some(file.to_string()),
                    line: Some(idx + 1),
                });
            }
        }

        if results.is_empty() {
            results.push(CheckResult {
                check_name: "dangerous_patterns".to_string(),
                passed: true,
                message: "No dangerous patterns detected".to_string(),
                file: Some(file.to_string()),
                line: None,
            });
        }

        results
    }

    /// 生成 pre-commit hook 脚本（安装时调用）
    pub fn generate_hook_script(&self) -> String {
        let config_json = serde_json::to_string(&self.config).unwrap_or_default();
        format!(
            r#"#!/bin/bash
# ACoder Pre-commit Hook — 自动生成，请勿手动修改
# 配置: {}

set -e

echo "Running ACoder pre-commit hooks..."

# 获取 staged 文件
FILES=$(git diff --cached --name-only --diff-filter=ACM)
if [ -z "$FILES" ]; then
    echo "No files to check"
    exit 0
fi

# 检查是否安装了 acode
if ! command -v acode &> /dev/null; then
    echo "Warning: 'acode' command not found, skipping AI review"
    CONFIG_AI_REVIEW=false
else
    CONFIG_AI_REVIEW={}
fi

# 运行检查
for FILE in $FILES; do
    if [ -f "$FILE" ]; then
        echo "Checking $FILE..."
        # 危险模式检测（内联简化版）
        if grep -iE "(api[_-]?key|secret[_-]?key|access[_-]?token|password\s*[=:])" "$FILE" > /dev/null 2>&1; then
            echo "ERROR: Potentially exposed secret in $FILE"
            exit 1
        fi
    fi
done

echo "Pre-commit checks passed!"
exit 0
"#,
            config_json,
            if self.config.ai_review { "true" } else { "false" }
        )
    }

    /// 安装 hook 到当前 git 仓库
    pub async fn install_hook(&self) -> Result<()> {
        let script = self.generate_hook_script();
        let hook_path = ".git/hooks/pre-commit";
        tokio::fs::write(hook_path, &script).await?;
        // 设置可执行权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(hook_path)
                .await?
                .permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(hook_path, perms).await?;
        }
        Ok(())
    }
}

use std::sync::Arc;
