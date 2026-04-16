//! 上下文层（Context）
//!
//! 负责加载、构建、管理项目上下文
//! - 文件系统扫描
//! - 技术栈检测
//! - 编程语言统计

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::error::Result;

pub mod assembler;
pub mod retriever;
pub mod compressor;
pub mod splitter;

pub struct Context {
    pub root: PathBuf,
    pub tech_stack: Vec<String>,
    pub languages: Vec<String>,
    pub files: Vec<FileEntry>,
    pub tree: DirectoryTree,
    pub file_count: usize,
    pub readme: Option<String>,
    pub configs: Vec<ConfigEntry>,
    pub git_info: Option<GitInfo>,
    pub recent_commits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub language: String,
    pub size: u64,
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryTree {
    pub name: String,
    pub path: String,
    pub children: Vec<DirectoryNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DirectoryNode {
    File { name: String, size: u64 },
    Directory { name: String, children: Vec<DirectoryNode> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub path: String,
    pub name: String,
    pub config_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: String,
    pub is_dirty: bool,
    pub remotes: Vec<String>,
}

impl Context {
    pub async fn load(root: &PathBuf) -> Result<Self> {
        tracing::info!("加载项目上下文: {}", root.display());

        let mut files = Vec::new();
        let mut configs = Vec::new();
        let mut readme: Option<String> = None;

        // 遍历项目文件（使用标准 walkdir）
        let walker = walkdir::WalkDir::new(root)
            .max_depth(10)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !n.starts_with('.') && n != "node_modules" && n != "target" && n != "dist" && n != "build"
            });

        for entry_result in walker {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_file() { continue; }

            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }

            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size > 5_000_000 { continue; }

            let ext = path.extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            let language = detect_language(&ext, &name);

            let rel_path = path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            let lines = if size < 500_000 {
                if let Ok(content) = std::fs::read_to_string(path) {
                    content.lines().count()
                } else { 0 }
            } else { 0 };

            files.push(FileEntry {
                path: rel_path.clone(),
                language,
                size,
                lines,
            });

            if is_config_file(&name) {
                configs.push(ConfigEntry {
                    path: rel_path,
                    name: name.clone(),
                    config_type: config_type(&ext),
                });
            }

            if name.to_lowercase() == "readme.md" || name.to_lowercase() == "readme" {
                if readme.is_none() {
                    readme = std::fs::read_to_string(path).ok()
                        .map(|c| c.chars().take(3000).collect());
                }
            }
        }

        let tech_stack = Self::detect_tech_stack(&files);
        let languages: Vec<String> = files.iter()
            .map(|f| f.language.clone())
            .filter(|l| !l.is_empty() && l != "other")
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let tree = build_tree(root)?;
        let git_info = Self::git_info(root).await.ok();
        let recent_commits = Self::recent_commits(root).await.unwrap_or_default();
        let file_count = files.len();

        tracing::info!("上下文加载完成: {} 个文件, {:?} 技术栈", file_count, tech_stack);

        Ok(Self {
            root: root.clone(),
            tech_stack,
            languages,
            files,
            tree,
            file_count,
            readme,
            configs,
            git_info,
            recent_commits,
        })
    }

    fn detect_language(ext: &str, name: &str) -> String {
        match ext {
            "rs" => "Rust".into(),
            "go" => "Go".into(),
            "ts" | "tsx" => "TypeScript".into(),
            "js" | "jsx" | "mjs" => "JavaScript".into(),
            "py" | "pyi" => "Python".into(),
            "java" => "Java".into(),
            "kt" | "kts" => "Kotlin".into(),
            "swift" => "Swift".into(),
            "c" => "C".into(),
            "cpp" | "cc" | "cxx" | "hpp" => "C++".into(),
            "cs" => "C#".into(),
            "rb" => "Ruby".into(),
            "php" => "PHP".into(),
            "html" | "htm" => "HTML".into(),
            "css" | "scss" | "sass" | "less" => "CSS".into(),
            "json" => "JSON".into(),
            "yaml" | "yml" => "YAML".into(),
            "toml" => "TOML".into(),
            "xml" => "XML".into(),
            "sql" => "SQL".into(),
            "sh" | "bash" | "zsh" => "Shell".into(),
            "md" | "markdown" => "Markdown".into(),
            _ => {
                if name.eq_ignore_ascii_case("Makefile") || name.eq_ignore_ascii_case("makefile") {
                    "Make".into()
                } else if name.eq_ignore_ascii_case("CMakeLists.txt") {
                    "CMake".into()
                } else {
                    "other".into()
                }
            }
        }
    }

    fn is_config_file(name: &str) -> bool {
        matches!(
            name.to_lowercase().as_str(),
            "package.json" | "cargo.toml" | "go.mod" | "requirements.txt"
            | "pyproject.toml" | "setup.py" | "pipfile" | "gemfile"
            | "build.gradle" | "pom.xml" | "webpack.config"
            | "vite.config" | "tsconfig.json" | "rustfmt.toml"
            | "cargo.lock" | "package-lock.json" | "yarn.lock"
            | "docker-compose.yml" | ".gitignore" | ".dockerignore"
        )
    }

    fn config_type(ext: &str) -> String {
        match ext {
            "json" => "json".into(),
            "yaml" | "yml" => "yaml".into(),
            "toml" => "toml".into(),
            "xml" => "xml".into(),
            _ => "text".into(),
        }
    }

    fn detect_tech_stack(files: &[FileEntry]) -> Vec<String> {
        let mut stack = Vec::new();
        for f in files {
            let p = f.path.to_lowercase();
            if p.contains("cargo.toml") && !stack.contains(&"Rust".into()) {
                stack.push("Rust".into());
            }
            if p.contains("go.mod") && !stack.contains(&"Go".into()) {
                stack.push("Go".into());
            }
            if p.contains("package.json") {
                if !stack.contains(&"Node.js".into()) { stack.push("Node.js".into()); }
                if p.contains("react") && !stack.contains(&"React".into()) { stack.push("React".into()); }
            }
            if p.contains("pyproject.toml") || p.contains("requirements.txt") {
                if !stack.contains(&"Python".into()) { stack.push("Python".into()); }
            }
            if p.contains("dockerfile") || p.contains("docker-compose") {
                if !stack.contains(&"Docker".into()) { stack.push("Docker".into()); }
            }
        }
        stack
    }

    fn build_tree(root: &Path) -> Result<DirectoryTree> {
        let name = root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "root".into());
        Ok(DirectoryTree { name, path: root.to_string_lossy().to_string(), children: Vec::new() })
    }

    async fn git_info(root: &Path) -> Result<GitInfo> {
        let branch = Self::run_git(root, &["branch", "--show-current"]).await
            .unwrap_or_default().trim().to_string();
        let status = Self::run_git(root, &["status", "--porcelain"]).await.unwrap_or_default();
        Ok(GitInfo { branch, is_dirty: !status.is_empty(), remotes: Vec::new() })
    }

    async fn recent_commits(root: &Path) -> Result<Vec<String>> {
        let output = Self::run_git(root, &["log", "-5", "--oneline"]).await
            .unwrap_or_default();
        Ok(output.lines().map(String::from).collect())
    }

    async fn run_git(root: &Path, args: &[&str]) -> Option<String> {
        tokio::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output().await.ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
    }

    pub fn summary(&self) -> String {
        format!(
            "项目根目录: {}\n技术栈: {:?}\n编程语言: {:?}\n文件数量: {}\n配置文件: {} 个\nGit: {}{}",
            self.root.display(), self.tech_stack, self.languages, self.file_count,
            self.configs.len(),
            self.git_info.as_ref().map(|g| g.branch.as_str()).unwrap_or("N/A"),
            if self.git_info.as_ref().map(|g| g.is_dirty).unwrap_or(false) { " (有未提交变更)" } else { "" },
        )
    }
}

// 本地辅助函数
fn detect_language(ext: &str, name: &str) -> String {
    Context::detect_language(ext, name)
}

fn is_config_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "package.json" || lower == "cargo.toml" || lower == "go.mod"
        || lower == "requirements.txt" || lower == "pyproject.toml"
        || lower == "rustfmt.toml" || lower == ".gitignore"
}

fn config_type(ext: &str) -> String {
    match ext {
        "json" => "json".into(),
        "yaml" | "yml" => "yaml".into(),
        "toml" => "toml".into(),
        _ => "text".into(),
    }
}

fn build_tree(root: &Path) -> Result<DirectoryTree> {
    Context::build_tree(root)
}
