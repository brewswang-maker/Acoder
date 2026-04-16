//! 扩展内置工具集
//!
//! ACoder 的核心工具能力：
//! - 文件操作：编辑、创建项目
//! - 开发工作流：安装依赖、运行测试、代码检查、格式化
//! - 运维：Docker 构建、复杂度分析

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{Error, Result};

/// 工具定义
#[derive(Debug, Clone)]
pub struct BuiltinTool {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters_schema: &'static str,
}

/// 工具执行器
pub struct BuiltinToolExecutor {
    tools: Vec<BuiltinTool>,
}

impl BuiltinToolExecutor {
    pub fn new() -> Self {
        let tools = vec![
            BuiltinTool {
                name: "edit_file",
                description: "编辑文件指定行范围（替换行内容）",
                parameters_schema: r#"{
                    "type": "object",
                    "required": ["path", "start_line", "end_line", "new_content"],
                    "properties": {
                        "path": {"type": "string", "description": "文件路径"},
                        "start_line": {"type": "integer", "description": "起始行（1-based）"},
                        "end_line": {"type": "integer", "description": "结束行（1-based）"},
                        "new_content": {"type": "string", "description": "替换内容"}
                    }
                }"#,
            },
            BuiltinTool {
                name: "install_deps",
                description: "安装项目依赖",
                parameters_schema: r#"{
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "项目路径，默认当前目录"},
                        "language": {"type": "string", "enum": ["rust", "node", "python", "go", "auto"], "description": "语言，auto 自动检测"}
                    }
                }"#,
            },
            BuiltinTool {
                name: "test_runner",
                description: "运行项目测试",
                parameters_schema: r#"{
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "项目路径"},
                        "language": {"type": "string", "description": "语言"},
                        "test_filter": {"type": "string", "description": "测试过滤表达式"}
                    }
                }"#,
            },
            BuiltinTool {
                name: "lint_check",
                description: "代码静态检查",
                parameters_schema: r#"{
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "文件或目录路径"},
                        "language": {"type": "string", "description": "语言"}
                    }
                }"#,
            },
            BuiltinTool {
                name: "format_code",
                description: "代码格式化",
                parameters_schema: r#"{
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "文件或目录路径"},
                        "language": {"type": "string", "description": "语言"}
                    }
                }"#,
            },
            BuiltinTool {
                name: "docker_build",
                description: "Docker 镜像构建",
                parameters_schema: r#"{
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {"type": "string", "description": "Dockerfile 所在目录"},
                        "tag": {"type": "string", "description": "镜像标签"}
                    }
                }"#,
            },
            BuiltinTool {
                name: "analyze_complexity",
                description: "分析代码复杂度",
                parameters_schema: r#"{
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {"type": "string", "description": "文件或目录路径"},
                        "metric": {"type": "string", "enum": ["cyclomatic", "cognitive", "lines", "all"], "description": "分析指标"}
                    }
                }"#,
            },
        ];

        Self { tools }
    }

    /// 获取所有工具定义
    pub fn tool_definitions(&self) -> &[BuiltinTool] {
        &self.tools
    }

    /// 执行工具
    pub async fn execute(&self, name: &str, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        match name {
            "edit_file" => self.edit_file(args, workdir).await,
            "install_deps" => self.install_deps(args, workdir).await,
            "test_runner" => self.test_runner(args, workdir).await,
            "lint_check" => self.lint_check(args, workdir).await,
            "format_code" => self.format_code(args, workdir).await,
            "docker_build" => self.docker_build(args, workdir).await,
            "analyze_complexity" => self.analyze_complexity(args, workdir).await,
            _ => Err(Error::ToolNotFound { tool_name: name.into() }),
        }
    }

    /// 编辑文件指定行
    async fn edit_file(&self, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| Error::ToolCallFailed { tool_name: "edit_file".into(), reason: "缺少 path".into() })?;
        let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let end = args.get("end_line").and_then(|v| v.as_u64()).unwrap_or(start as u64) as usize;
        let new_content = args.get("new_content").and_then(|v| v.as_str())
            .ok_or_else(|| Error::ToolCallFailed { tool_name: "edit_file".into(), reason: "缺少 new_content".into() })?;

        let path = if PathBuf::from(path_str).is_absolute() {
            PathBuf::from(path_str)
        } else {
            workdir.join(path_str)
        };

        let content = tokio::fs::read_to_string(&path).await
            .map_err(|_| Error::FileNotFound { path: path.clone() })?;

        let mut lines: Vec<&str> = content.lines().collect();
        if start < 1 || end > lines.len() || start > end {
            return Err(Error::ToolCallFailed {
                tool_name: "edit_file".into(),
                reason: format!("无效行范围: {}-{} (总 {} 行)", start, end, lines.len()),
            });
        }

        let replacement_lines: Vec<&str> = new_content.lines().collect();
        let replacement_count = replacement_lines.len();
        let mut new_lines = lines[..start - 1].to_vec();
        new_lines.extend(replacement_lines);
        if end <= lines.len() {
            new_lines.extend(lines[end..].iter());
        }

        let new_content_str = new_lines.join("\n");
        tokio::fs::write(&path, &new_content_str).await?;

        Ok(format!("✅ 已编辑 {} (行 {}-{}, {} → {} 行)",
            path.display(), start, end, end - start + 1, replacement_count))
    }

    /// 安装依赖
    async fn install_deps(&self, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let lang = args.get("language").and_then(|v| v.as_str()).unwrap_or("auto");
        let path = if PathBuf::from(path_str).is_absolute() {
            PathBuf::from(path_str)
        } else {
            workdir.join(path_str)
        };

        // 自动检测语言
        let actual_lang = if lang != "auto" {
            lang
        } else if path.join("Cargo.toml").exists() {
            "rust"
        } else if path.join("package.json").exists() {
            "node"
        } else if path.join("requirements.txt").exists() || path.join("pyproject.toml").exists() {
            "python"
        } else if path.join("go.mod").exists() {
            "go"
        } else {
            return Err(Error::ToolCallFailed { tool_name: "install_deps".into(), reason: "无法检测项目语言".into() });
        };

        let (cmd, cmd_args) = match actual_lang {
            "rust" => ("cargo", vec!["build"]),
            "node" => ("npm", vec!["install"]),
            "python" => ("pip", vec!["install", "-r", "requirements.txt"]),
            "go" => ("go", vec!["mod", "download"]),
            _ => return Err(Error::ToolCallFailed { tool_name: "install_deps".into(), reason: format!("不支持的语言: {}", actual_lang) }),
        };

        let output = tokio::process::Command::new(cmd)
            .args(&cmd_args)
            .current_dir(&path)
            .output().await
            .map_err(|e| Error::ExecutionFailed { lang: actual_lang.into(), reason: e.to_string() })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(format!("✅ 依赖安装完成 ({})\n{}", actual_lang, stdout))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(Error::ExecutionFailed { lang: actual_lang.into(), reason: stderr.to_string() })
        }
    }

    /// 运行测试
    async fn test_runner(&self, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let path = workdir.join(path_str);

        let (cmd, cmd_args) = if path.join("Cargo.toml").exists() {
            ("cargo", vec!["test", "--", "--color", "always"])
        } else if path.join("package.json").exists() {
            ("npm", vec!["test"])
        } else if path.join("pytest.ini").exists() || path.join("pyproject.toml").exists() {
            ("python", vec!["-m", "pytest"])
        } else if path.join("go.mod").exists() {
            ("go", vec!["test", "./..."])
        } else {
            ("cargo", vec!["test"])
        };

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            tokio::process::Command::new(cmd)
                .args(&cmd_args)
                .current_dir(&path)
                .output(),
        ).await
            .map_err(|_| Error::TaskTimeout("测试执行超时 (120s)".into()))?
            .map_err(|e| Error::ExecutionFailed { lang: cmd.into(), reason: e.to_string() })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(format!("✅ 测试通过\n{}", stdout))
        } else {
            Err(Error::ExecutionFailed { lang: cmd.into(), reason: format!("{}\n{}", stdout, stderr) })
        }
    }

    /// 代码检查
    async fn lint_check(&self, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let path = workdir.join(path_str);

        let output = tokio::process::Command::new("cargo")
            .args(["clippy", "--", "-W", "clippy::all"])
            .current_dir(&path)
            .output().await
            .map_err(|e| Error::ExecutionFailed { lang: "rust".into(), reason: e.to_string() })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(format!("Clippy 检查结果:\n{}{}", stdout, stderr))
    }

    /// 代码格式化
    async fn format_code(&self, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let path = workdir.join(path_str);

        let output = tokio::process::Command::new("cargo")
            .args(["fmt"])
            .current_dir(&path)
            .output().await
            .map_err(|e| Error::ExecutionFailed { lang: "rust".into(), reason: e.to_string() })?;

        if output.status.success() {
            Ok("✅ 代码格式化完成".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(Error::ExecutionFailed { lang: "rust".into(), reason: stderr.to_string() })
        }
    }

    /// Docker 构建
    async fn docker_build(&self, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| Error::ToolCallFailed { tool_name: "docker_build".into(), reason: "缺少 path".into() })?;
        let tag = args.get("tag").and_then(|v| v.as_str()).unwrap_or("acode-app:latest");
        let path = workdir.join(path_str);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            tokio::process::Command::new("docker")
                .args(["build", "-t", tag, "."])
                .current_dir(&path)
                .output(),
        ).await
            .map_err(|_| Error::TaskTimeout("Docker 构建超时 (300s)".into()))?
            .map_err(|e| Error::ExecutionFailed { lang: "docker".into(), reason: e.to_string() })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(format!("✅ Docker 镜像构建完成: {}\n{}", tag, stdout))
        } else {
            Err(Error::ExecutionFailed { lang: "docker".into(), reason: format!("{}\n{}", stdout, stderr) })
        }
    }

    /// 代码复杂度分析
    async fn analyze_complexity(&self, args: &serde_json::Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| Error::ToolCallFailed { tool_name: "analyze_complexity".into(), reason: "缺少 path".into() })?;
        let path = workdir.join(path_str);

        if !path.exists() {
            return Err(Error::FileNotFound { path });
        }

        let mut report = String::new();
        report.push_str(&format!("代码复杂度分析: {}\n", path.display()));

        if path.is_file() {
            let content = tokio::fs::read_to_string(&path).await?;
            let lines = content.lines().count();
            let functions = content.matches("fn ").count() + content.matches("async fn ").count();
            let branches = content.matches("if ").count() + content.matches("match ").count();
            let loops = content.matches("for ").count() + content.matches("while ").count();

            let cyclomatic = 1 + branches + loops;
            report.push_str(&format!(
                "\n  总行数: {}\n  函数数: {}\n  分支数: {}\n  循环数: {}\n  圈复杂度: {}\n",
                lines, functions, branches, loops, cyclomatic,
            ));

            let rating = if cyclomatic <= 10 { "✅ 简单" }
                else if cyclomatic <= 20 { "⚠️ 中等" }
                else if cyclomatic <= 50 { "🔴 复杂" }
                else { "⛔ 极复杂" };
            report.push_str(&format!("  评级: {}", rating));
        } else if path.is_dir() {
            let mut total_files = 0usize;
            let mut total_lines = 0usize;
            let mut total_functions = 0usize;

            let mut entries = tokio::fs::read_dir(&path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let p = entry.path();
                if p.is_file() && p.extension().map(|e| e == "rs").unwrap_or(false) {
                    let content = tokio::fs::read_to_string(&p).await?;
                    let lines = content.lines().count();
                    let functions = content.matches("fn ").count() + content.matches("async fn ").count();
                    total_files += 1;
                    total_lines += lines;
                    total_functions += functions;
                }
            }

            report.push_str(&format!(
                "\n  文件数: {}\n  总行数: {}\n  函数数: {}\n  平均行/文件: {:.0}\n",
                total_files, total_lines, total_functions,
                if total_files > 0 { total_lines as f64 / total_files as f64 } else { 0.0 },
            ));
        }

        Ok(report)
    }
}

impl Default for BuiltinToolExecutor {
    fn default() -> Self { Self::new() }
}
