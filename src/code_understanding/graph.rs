//! 代码图谱 — 调用关系图和依赖图
//!
//! 参考 GitNexus + Code-Review-Graph 设计：
//! - 调用关系图（CallGraph）：函数 → 函数调用链
//! - 依赖图（DependencyGraph）：模块 → 模块依赖
//! - 影响面分析（Blast Radius）：变更 → 受影响文件
//!
//! Code-Review-Graph 核心数据：
//! - Token 效率：平均减少 8.2x，gin 最高 16.4x
//! - 爆炸半径分析：100% 召回率
//! - 增量更新：< 2 秒（2900 文件项目）

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// 代码图谱
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeGraph {
    /// 节点：文件 → 文件信息
    pub files: HashMap<String, FileInfo>,
    /// 边：调用关系（caller → callees）
    pub call_edges: HashMap<String, Vec<String>>,
    /// 边：依赖关系（module → dependencies）
    pub dep_edges: HashMap<String, Vec<String>>,
    /// 边：被调用关系（callee → callers，反向索引）
    pub reverse_call_edges: HashMap<String, Vec<String>>,
}

/// 文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub language: Language,
    pub functions: Vec<FunctionInfo>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub loc: usize,
    pub complexity: f64,
}

/// 函数信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
    pub complexity: f64,
}

/// 编程语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    C,
    Cpp,
    Unknown,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// 爆炸半径分析（Blast Radius）
    ///
    /// 给定变更文件列表，返回所有受影响的文件
    /// 参考 Code-Review-Graph：100% 召回率
    pub fn blast_radius(&self, changed_files: &[&str]) -> BlastRadius {
        let mut affected = HashSet::new();
        let mut call_chain = Vec::new();

        for file in changed_files {
            affected.insert(file.to_string());

            // 沿反向调用链向上追踪（谁调用了变更文件）
            let mut to_visit = vec![file.to_string()];
            let mut visited = HashSet::new();

            while let Some(current) = to_visit.pop() {
                if visited.contains(&current) {
                    continue;
                }
                visited.insert(current.clone());

                if let Some(callers) = self.reverse_call_edges.get(&current) {
                    for caller in callers {
                        if !affected.contains(caller) {
                            affected.insert(caller.clone());
                            call_chain.push(CallChainEntry {
                                from: caller.clone(),
                                to: current.clone(),
                                relation: RelationType::Call,
                            });
                        }
                        to_visit.push(caller.clone());
                    }
                }

                // 沿依赖链追踪
                if let Some(deps) = self.dep_edges.get(&current) {
                    for dep in deps {
                        if affected.contains(dep) {
                            continue;
                        }
                        // 只有被变更文件直接依赖的才算
                        if changed_files.iter().any(|f| *f == current.as_str()) {
                            // 不追踪下游依赖（变更影响向上传播，不向下）
                        }
                    }
                }
            }
        }

        BlastRadius {
            changed_files: changed_files.iter().map(|f| f.to_string()).collect(),
            affected_files: affected.into_iter().collect(),
            call_chain,
            risk_level: self.calculate_risk_level(changed_files),
        }
    }

    /// 计算风险等级
    fn calculate_risk_level(&self, changed_files: &[&str]) -> RiskLevel {
        let mut total_callers = 0;

        for file in changed_files {
            if let Some(callers) = self.reverse_call_edges.get(*file) {
                total_callers += callers.len();
            }
        }

        if total_callers > 20 {
            RiskLevel::Critical
        } else if total_callers > 10 {
            RiskLevel::High
        } else if total_callers > 3 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }

    /// 添加调用关系
    pub fn add_call(&mut self, caller: String, callee: String) {
        self.call_edges.entry(caller.clone()).or_default().push(callee.clone());
        self.reverse_call_edges.entry(callee).or_default().push(caller);
    }

    /// 添加依赖关系
    pub fn add_dependency(&mut self, module: String, dependency: String) {
        self.dep_edges.entry(module).or_default().push(dependency);
    }

    /// 获取函数的所有调用者（传递闭包）
    pub fn all_callers(&self, function: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut to_visit = vec![function.to_string()];
        let mut visited = HashSet::new();

        while let Some(current) = to_visit.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            if let Some(callers) = self.reverse_call_edges.get(&current) {
                for caller in callers {
                    result.push(caller.clone());
                    to_visit.push(caller.clone());
                }
            }
        }

        result
    }
}

/// 爆炸半径分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadius {
    /// 直接变更的文件
    pub changed_files: Vec<String>,
    /// 受影响的文件（包含变更文件）
    pub affected_files: Vec<String>,
    /// 调用链
    pub call_chain: Vec<CallChainEntry>,
    /// 风险等级
    pub risk_level: RiskLevel,
}

/// 调用链条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallChainEntry {
    pub from: String,
    pub to: String,
    pub relation: RelationType,
}

/// 关系类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    Call,
    Dependency,
    Import,
}

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

// ── Analyzer ───────────────────────────────────────────────

use anyhow::Result;

/// 分析深度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalysisDepth {
    /// 轻量：文件列表 + 行数统计
    Light = 0,
    /// 中等：符号提取（函数/结构体/枚举）
    Medium = 1,
    /// 深度：完整 AST + 调用图
    Deep = 2,
}

impl Default for AnalysisDepth {
    fn default() -> Self { Self::Medium }
}

impl From<usize> for AnalysisDepth {
    fn from(v: usize) -> Self {
        match v {
            0 => AnalysisDepth::Light,
            1 => AnalysisDepth::Medium,
            _ => AnalysisDepth::Deep,
        }
    }
}

impl From<String> for AnalysisDepth {
    fn from(v: String) -> Self {
        match v.as_str() {
            "light" | "0" => AnalysisDepth::Light,
            "deep" | "2" => AnalysisDepth::Deep,
            _ => AnalysisDepth::Medium,
        }
    }
}

use std::fmt;
impl fmt::Display for AnalysisReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== 代码分析报告 ===")?;
        writeln!(f, "总文件数: {}", self.total_files)?;
        writeln!(f, "总函数数: {}", self.total_functions)?;
        writeln!(f, "总结构体数: {}", self.total_structs)?;
        writeln!(f, "总代码行数: {}", self.total_lines)?;
        writeln!(f, "\n语言分布:")?;
        for lang in &self.language_breakdown {
            writeln!(f, "  {}: {} 文件, {} 行", lang.language, lang.count, lang.lines)?;
        }
        if !self.files.is_empty() && self.total_files <= 20 {
            writeln!(f, "\n文件详情:")?;
            for file in &self.files {
                writeln!(f, "  {} ({} 行, {} 函数)", file.path, file.lines, file.functions)?;
            }
        }
        Ok(())
    }
}

/// 分析报告
#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub total_files: usize,
    pub total_functions: usize,
    pub total_structs: usize,
    pub total_lines: usize,
    pub files: Vec<FileSummary>,
    pub language_breakdown: Vec<LanguageCount>,
}

#[derive(Debug, Clone)]
pub struct FileSummary {
    pub path: String,
    pub lines: usize,
    pub functions: usize,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct LanguageCount {
    pub language: String,
    pub count: usize,
    pub lines: usize,
}

/// 代码分析器
pub struct Analyzer {
    project_path: String,
}

impl Analyzer {
    pub fn new(path: &str) -> Result<Self> {
        Ok(Self { project_path: path.into() })
    }

    pub async fn analyze(&self, depth: AnalysisDepth) -> Result<AnalysisReport> {
        use std::collections::HashMap;

        let parser = crate::code_understanding::parser::Parser::new(&self.project_path);
        let mut files = Vec::new();
        let mut total_functions = 0usize;
        let mut total_structs = 0usize;
        let mut total_lines = 0usize;
        let mut lang_stats: HashMap<String, (usize, usize)> = HashMap::new();

        let paths = self.walk_source_files()?;
        for path in paths {
            let rel = std::path::Path::new(&path);
            let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");

            let lang = match ext {
                "rs" => "Rust",
                "ts" | "tsx" => "TypeScript",
                "js" | "jsx" | "mjs" => "JavaScript",
                "py" => "Python",
                "go" => "Go",
                "java" => "Java",
                _ => "Other",
            }.to_string();

            if depth >= AnalysisDepth::Medium {
                if let Ok(result) = parser.parse_file(std::path::Path::new(&path)).await {
                    total_functions += result.functions.len();
                    total_structs += result.structs.len();
                    let lines = result.loc;
                    total_lines += lines;
                    files.push(FileSummary {
                        path,
                        lines,
                        functions: result.functions.len(),
                        language: lang.clone(),
                    });
                    let entry = lang_stats.entry(lang).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += result.loc;
                }
            } else {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let lines = content.lines().count();
                    total_lines += lines;
                    files.push(FileSummary {
                        path,
                        lines,
                        functions: 0,
                        language: lang.clone(),
                    });
                    let entry = lang_stats.entry(lang).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += lines;
                }
            }
        }

        let language_breakdown = lang_stats.into_iter()
            .map(|(language, (count, lines))| LanguageCount { language, count, lines })
            .collect();

        Ok(AnalysisReport {
            total_files: files.len(),
            total_functions,
            total_structs,
            total_lines,
            files,
            language_breakdown,
        })
    }

    fn walk_source_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();
        self.walk_dir(std::path::Path::new(&self.project_path), &mut files, 0)?;
        Ok(files)
    }

    fn walk_dir(&self, dir: &std::path::Path, files: &mut Vec<String>, depth: usize) -> Result<()> {
        if depth > 8 { return Ok(()); }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "target" || name == "node_modules" || name == "dist" {
                    continue;
                }
            }
            if path.is_dir() {
                self.walk_dir(&path, files, depth + 1)?;
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "rs"|"ts"|"tsx"|"js"|"jsx"|"py"|"go"|"java"|"c"|"cpp") {
                    files.push(path.display().to_string());
                }
            }
        }
        Ok(())
    }
}
