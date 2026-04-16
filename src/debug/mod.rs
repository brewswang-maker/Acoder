//! 调试模块 — 编译错误诊断 + 自动修复
//!
//! Phase 1 自动化调试能力：
//! 1. ErrorRecognition  — 从命令输出中识别编译/运行错误
//! 2. RootCauseAnalysis — 分类错误并推断根因
//! 3. FixApplication   — 生成并应用修复方案
//!
//! 工作流：
//!   parse_errors(output) → Vec<Diagnostic>
//!   analyze_root_cause(diagnostic) → FixSuggestion
//!   apply_fix(suggestion, edit_session) → bool

use regex::Regex;
use serde::{Deserialize, Serialize};

/// 诊断严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity { Hint, Note, Warning, Error, Bug }

/// 错误来源语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorLang { Rust, TypeScript, JavaScript, Python, Go, Java, C, Unknown }

/// 单个诊断项
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub code: Option<String>,
    pub severity: Severity,
    pub message: String,
    pub raw_line: String,
    pub suggestion: Option<String>,
}

/// 根因类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootCauseCategory {
    MissingImport, TypeMismatch, NameNotFound, ModuleNotFound,
    SyntaxError, CyclicDependency, LifetimeError, OwnershipError,
    Deprecation, ConfigError, Unknown,
}

impl RootCauseCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MissingImport => "缺少 import/use",
            Self::TypeMismatch => "类型不匹配",
            Self::NameNotFound => "名称未找到",
            Self::ModuleNotFound => "模块未找到",
            Self::SyntaxError => "语法错误",
            Self::CyclicDependency => "循环依赖",
            Self::LifetimeError => "生命周期错误",
            Self::OwnershipError => "所有权错误",
            Self::Deprecation => "API 弃用",
            Self::ConfigError => "配置错误",
            Self::Unknown => "未知",
        }
    }
}

/// 根因分析结果
#[derive(Debug, Clone)]
pub struct RootCause {
    pub category: RootCauseCategory,
    pub confidence: f64,
    pub explanation: String,
    pub fix_hints: Vec<String>,
}

/// 修复建议
#[derive(Debug, Clone)]
pub struct FixSuggestion {
    pub diagnostic: Diagnostic,
    pub root_cause: RootCause,
    pub auto_fixable: bool,
    pub steps: Vec<FixStep>,
}

/// 修复步骤
#[derive(Debug, Clone)]
pub struct FixStep {
    pub step_type: FixStepType,
    pub file: String,
    pub description: String,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub enum FixStepType { Insert, Replace, Delete, RunCommand, Manual }

// ── Rust 错误解析 ─────────────────────────────────────────────────────────

pub fn parse_rust_errors(output: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let patterns: &[(&str, &str)] = &[
        (r"(?m)^error\[E0308\]: (.+?)\n\s+-->\s+(\S+):(\d+):(\d+)", "E0308"),
        (r"(?m)^error\[E0599\]: (.+?)\n\s+-->\s+(\S+):(\d+):(\d+)", "E0599"),
        (r"(?m)^error\[E0433\]: (.+?)\n\s+-->\s+(\S+):(\d+):(\d+)", "E0433"),
        (r"(?m)^error\[E0580\]: (.+?)\n\s+-->\s+(\S+):(\d+):(\d+)", "E0580"),
        (r"(?m)^error\[E0277\]: (.+?)\n\s+-->\s+(\S+):(\d+):(\d+)", "E0277"),
        (r"(?m)^error\[E0425\]: (.+?)\n\s+-->\s+(\S+):(\d+):(\d+)", "E0425"),
        (r"(?m)^error\[E0502\]: (.+?)\n\s+-->\s+(\S+):(\d+):(\d+)", "E0502"),
    ];

    for (pat, code) in patterns {
        let re = Regex::new(pat).unwrap();
        for cap in re.captures_iter(output) {
            diags.push(Diagnostic {
                file: cap[2].to_string(),
                line: cap[3].parse().ok(),
                column: cap[4].parse().ok(),
                code: Some(code.to_string()),
                severity: Severity::Error,
                message: cap[1].trim().to_string(),
                raw_line: cap[0].to_string(),
                suggestion: None,
            });
        }
    }

    // expected X, found Y
    let re = Regex::new(r"(?m)^error: expected `(.+?)`, found `(.+?)`\n\s+-->\s+(\S+):(\d+):(\d+)").unwrap();
    for cap in re.captures_iter(output) {
        let msg = format!("expected `{}`, found `{}`", &cap[1], &cap[2]);
        if !diags.iter().any(|d| &d.message == &msg) {
            diags.push(Diagnostic {
                file: cap[3].to_string(),
                line: cap[4].parse().ok(),
                column: cap[5].parse().ok(),
                code: None,
                severity: Severity::Error,
                message: msg,
                raw_line: cap[0].to_string(),
                suggestion: None,
            });
        }
    }

    // unused warning
    let re = Regex::new(r"(?m)^warning: unused variable: `(.+?)`\n\s+-->\s+(\S+):(\d+):(\d+)").unwrap();
    for cap in re.captures_iter(output) {
        diags.push(Diagnostic {
            file: cap[2].to_string(),
            line: cap[3].parse().ok(),
            column: cap[4].parse().ok(),
            code: Some("unused".to_string()),
            severity: Severity::Warning,
            message: format!("unused variable: `{}`", &cap[1]),
            raw_line: cap[0].to_string(),
            suggestion: None,
        });
    }

    diags
}

// ── TypeScript 错误解析 ────────────────────────────────────────────────────

pub fn parse_ts_errors(output: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let re = Regex::new(r"(?m)^TS(\d+): (.+?)\n\s+at (\S+):(\d+):(\d+)").unwrap();
    for cap in re.captures_iter(output) {
        let code = format!("TS{}", &cap[1]);
        diags.push(Diagnostic {
            file: cap[3].to_string(),
            line: cap[4].parse().ok(),
            column: cap[5].parse().ok(),
            code: Some(code.clone()),
            severity: Severity::Error,
            message: cap[2].trim().to_string(),
            raw_line: cap[0].to_string(),
            suggestion: None,
        });
    }

    let re = Regex::new(r"SyntaxError: (.+?) at (\S+):(\d+)").unwrap();
    for cap in re.captures_iter(output) {
        diags.push(Diagnostic {
            file: cap[2].to_string(),
            line: cap[3].parse().ok(),
            column: None,
            code: Some("SyntaxError".to_string()),
            severity: Severity::Error,
            message: cap[1].to_string(),
            raw_line: cap[0].to_string(),
            suggestion: None,
        });
    }

    diags
}

// ── Python 错误解析 ────────────────────────────────────────────────────────

pub fn parse_python_errors(output: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // NameError: name 'x' is not defined
    //   File "foo.py", line 10, in <module>
    // Note: Python 错误和 File 行可能是分开的
    let re = Regex::new(r#"(?m)^(\w+Error): (.+?)\n\s+File "([^"]+)", line (\d+)"#).unwrap();
    for cap in re.captures_iter(output) {
        let (code, sev) = match &cap[1] {
            "SyntaxError" | "IndentationError" => ("SyntaxError", Severity::Error),
            "NameError" | "TypeError" => (&cap[1] as &str, Severity::Error),
            "ImportError" | "ModuleNotFoundError" => ("ImportError", Severity::Error),
            _ => (&cap[1] as &str, Severity::Error),
        };
        diags.push(Diagnostic {
            file: cap[3].to_string(),
            line: cap[4].parse().ok(),
            column: None,
            code: Some(code.to_string()),
            severity: sev,
            message: format!("{}: {}", &cap[1], &cap[2]),
            raw_line: cap[0].to_string(),
            suggestion: None,
        });
    }

    // Fallback: 单独匹配 NameError/TypeError 等
    if diags.is_empty() {
        let re = Regex::new(r"(?m)^(\w+Error): (.+)$").unwrap();
        for cap in re.captures_iter(output) {
            let code = &cap[1];
            if matches!(code, "NameError" | "TypeError" | "SyntaxError" | "IndentationError" | "ImportError" | "ModuleNotFoundError" | "AttributeError" | "ValueError") {
                // 尝试找相关文件行
                let file_line_re = Regex::new(r#"\n\s+File "([^"]+)", line (\d+)"#).unwrap();
                let file_match = file_line_re.captures(output);
                diags.push(Diagnostic {
                    file: file_match.as_ref().map(|m| m[1].to_string()).unwrap_or_default(),
                    line: file_match.as_ref().and_then(|m| m[2].parse().ok()),
                    column: None,
                    code: Some(code.to_string()),
                    severity: Severity::Error,
                    message: cap[2].to_string(),
                    raw_line: cap[0].to_string(),
                    suggestion: None,
                });
            }
        }
    }

    diags
}

// ── Go 错误解析 ───────────────────────────────────────────────────────────

pub fn parse_go_errors(output: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let re = Regex::new(r"(?m)^(./\S+):(\d+):(\d+): (.+)").unwrap();
    for cap in re.captures_iter(output) {
        let msg = cap[4].trim().to_string();
        let sev = if msg.contains("undefined") || msg.contains("cannot") {
            Severity::Error
        } else {
            Severity::Warning
        };
        diags.push(Diagnostic {
            file: cap[1].to_string(),
            line: cap[2].parse().ok(),
            column: cap[3].parse().ok(),
            code: None,
            severity: sev,
            message: msg,
            raw_line: cap[0].to_string(),
            suggestion: None,
        });
    }

    diags
}

// ── 自动分发 ─────────────────────────────────────────────────────────────

pub fn parse_errors(output: &str, lang: ErrorLang) -> Vec<Diagnostic> {
    match lang {
        ErrorLang::Rust => parse_rust_errors(output),
        ErrorLang::TypeScript | ErrorLang::JavaScript => parse_ts_errors(output),
        ErrorLang::Python => parse_python_errors(output),
        ErrorLang::Go => parse_go_errors(output),
        _ => parse_generic_errors(output),
    }
}

fn parse_generic_errors(output: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let re = Regex::new(r"(?m)^([^:\s]+):(\d+):(\d+): (.+)").unwrap();
    for cap in re.captures_iter(output) {
        let msg = cap[4].trim().to_string();
        let sev = if msg.starts_with("error") || msg.starts_with("Error") {
            Severity::Error
        } else if msg.starts_with("warning") {
            Severity::Warning
        } else {
            Severity::Error
        };
        diags.push(Diagnostic {
            file: cap[1].to_string(),
            line: cap[2].parse().ok(),
            column: cap[3].parse().ok(),
            code: None,
            severity: sev,
            message: msg,
            raw_line: cap[0].to_string(),
            suggestion: None,
        });
    }
    diags
}

// ── 根因分析 ──────────────────────────────────────────────────────────────

pub fn analyze_root_cause(diag: &Diagnostic, lang: ErrorLang) -> RootCause {
    let msg_lower = diag.message.to_lowercase();
    let code = diag.code.as_deref().unwrap_or("");

    // Missing import
    if msg_lower.contains("use of undeclared")
        || msg_lower.contains("cannot find")
        || msg_lower.contains("not found in this scope")
        || code == "E0433" || code == "E0432"
    {
        return RootCause {
            category: RootCauseCategory::MissingImport,
            confidence: 0.95,
            explanation: "代码引用了未导入的模块、类型或函数".to_string(),
            fix_hints: vec![
                "检查是否需要添加 import 或 use 语句".to_string(),
                "如果是外部 crate，确保 Cargo.toml 中已声明依赖".to_string(),
                "检查模块路径是否正确".to_string(),
            ],
        };
    }

    // Type mismatch
    if msg_lower.contains("mismatched types")
        || msg_lower.contains("expected") && msg_lower.contains("found")
        || code == "E0308"
    {
        let hints = if msg_lower.contains("&str") && msg_lower.contains("string") {
            vec![
                "使用 .as_str() 将 String 转为 &str".to_string(),
                "使用 .to_string() 将 &str 转为 String".to_string(),
            ]
        } else {
            vec!["检查类型声明是否正确".to_string(), "使用类型注解明确变量类型".to_string()]
        };
        return RootCause {
            category: RootCauseCategory::TypeMismatch,
            confidence: 0.85,
            explanation: format!("类型不匹配：{}", diag.message),
            fix_hints: hints,
        };
    }

    // Name not found
    if msg_lower.contains("name ") || msg_lower.contains("undefined") || msg_lower.contains("undeclared") || code == "E0425" {
        return RootCause {
            category: RootCauseCategory::NameNotFound,
            confidence: 0.95,
            explanation: "名称未在当前作用域声明".to_string(),
            fix_hints: vec![
                "检查名称拼写是否正确".to_string(),
                "确保在使用前已声明变量/函数/类型".to_string(),
            ],
        };
    }

    // Lifetime / Borrow
    if msg_lower.contains("lifetime")
        || msg_lower.contains("borrow")
        || msg_lower.contains("does not live long enough")
        || code == "E0502" || code == "E0596"
    {
        return RootCause {
            category: RootCauseCategory::LifetimeError,
            confidence: 0.9,
            explanation: "生命周期或借用错误".to_string(),
            fix_hints: vec![
                "检查是否存在悬空引用".to_string(),
                "考虑使用 Clone 或 Copy trait".to_string(),
                "调整生命周期标注".to_string(),
            ],
        };
    }

    // Ownership / Move
    if msg_lower.contains("move occurs")
        || msg_lower.contains("value borrowed")
        || msg_lower.contains("use after move")
        || code == "E0382" || code == "E0505"
    {
        return RootCause {
            category: RootCauseCategory::OwnershipError,
            confidence: 0.9,
            explanation: "值被移动后仍然被使用".to_string(),
            fix_hints: vec![
                "在移动前克隆值：.clone()".to_string(),
                "使用引用代替移动：&x".to_string(),
            ],
        };
    }

    // Trait not satisfied
    if msg_lower.contains("trait bound") || msg_lower.contains("is not satisfied") || code == "E0277" {
        return RootCause {
            category: RootCauseCategory::TypeMismatch,
            confidence: 0.85,
            explanation: "类型未实现所需 trait".to_string(),
            fix_hints: vec![
                "为类型实现缺失的 trait".to_string(),
                "检查泛型参数是否正确".to_string(),
            ],
        };
    }

    // Syntax error
    if msg_lower.contains("syntax") || code == "E0580" || code == "E0603" {
        return RootCause {
            category: RootCauseCategory::SyntaxError,
            confidence: 0.85,
            explanation: format!("语法错误：{}", diag.message),
            fix_hints: vec![
                "检查括号、引号是否匹配".to_string(),
                "检查是否有缺少的分号或逗号".to_string(),
            ],
        };
    }

    // Unused
    if msg_lower.contains("unused") || code == "unused" {
        return RootCause {
            category: RootCauseCategory::Unknown,
            confidence: 1.0,
            explanation: "变量声明后未被使用".to_string(),
            fix_hints: vec![
                "使用变量或删除未使用的声明".to_string(),
                "使用 _ 忽略不需要的值".to_string(),
            ],
        };
    }

    RootCause {
        category: RootCauseCategory::Unknown,
        confidence: 0.5,
        explanation: format!("未知错误：{}", diag.message),
        fix_hints: vec!["查看完整错误信息".to_string(), "搜索错误代码获取帮助".to_string()],
    }
}

// ── 修复建议 ─────────────────────────────────────────────────────────────

pub fn suggest_fix(diag: &Diagnostic, root_cause: &RootCause, file_content: &str) -> FixSuggestion {
    let steps = generate_fix_steps(diag, root_cause, file_content);
    let auto_fixable = steps.iter().all(|s| {
        matches!(s.step_type, FixStepType::Insert | FixStepType::Replace | FixStepType::Delete)
    });
    FixSuggestion { diagnostic: diag.clone(), root_cause: root_cause.clone(), auto_fixable, steps }
}

fn generate_fix_steps(diag: &Diagnostic, root_cause: &RootCause, file_content: &str) -> Vec<FixStep> {
    match root_cause.category {
        RootCauseCategory::MissingImport => {
            let missing = extract_missing_name(&diag.message);
            let insert_line = find_import_insert_line(file_content);
            vec![FixStep {
                step_type: FixStepType::Insert,
                file: diag.file.clone(),
                description: format!("添加缺失的 import: {}", missing),
                old_text: None,
                new_text: Some(format!("use {};", missing)),
                line: Some(insert_line),
            }]
        }
        RootCauseCategory::TypeMismatch | RootCauseCategory::OwnershipError | RootCauseCategory::LifetimeError => {
            vec![FixStep {
                step_type: FixStepType::Manual,
                file: diag.file.clone(),
                description: format!("{} — 请手动修复", root_cause.explanation),
                old_text: None,
                new_text: None,
                line: diag.line,
            }]
        }
        RootCauseCategory::NameNotFound => {
            let name = extract_missing_name(&diag.message);
            vec![FixStep {
                step_type: FixStepType::Manual,
                file: diag.file.clone(),
                description: format!("名称 `{}` 未找到，需要声明或导入", name),
                old_text: None,
                new_text: None,
                line: diag.line,
            }]
        }
        _ => {
            vec![FixStep {
                step_type: FixStepType::Manual,
                file: diag.file.clone(),
                description: format!("{} — {}", root_cause.category.name(), root_cause.explanation),
                old_text: None,
                new_text: None,
                line: diag.line,
            }]
        }
    }
}

fn extract_missing_name(msg: &str) -> String {
    if let Some(start) = msg.find('`') {
        if let Some(end) = msg[start+1..].find('`') {
            return msg[start+1..start+1+end].to_string();
        }
    }
    let lower = msg.to_lowercase();
    if let Some(start) = lower.find("cannot find `") {
        let rest = &msg[start + 12..];
        if let Some(end) = rest.find('`') {
            return rest[..end].to_string();
        }
    }
    msg.lines().next().unwrap_or("unknown").trim().to_string()
}

fn find_import_insert_line(content: &str) -> usize {
    let lines: Vec<&str> = content.lines().collect();
    let mut insert_after = 0;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("use ") || t.starts_with("pub use ") {
            insert_after = i + 1;
        }
    }
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if (t.starts_with("mod ") || t.starts_with("const ") || t.starts_with("fn ")
            || t.starts_with("struct ") || t.starts_with("enum ") || t.starts_with("impl "))
            && i > insert_after
        {
            return i;
        }
    }
    insert_after
}

// ── 报告 ─────────────────────────────────────────────────────────────────

pub struct DiagnosticReport {
    pub diagnostics: Vec<Diagnostic>,
    pub root_causes: Vec<RootCause>,
    pub summary: String,
}

impl DiagnosticReport {
    pub fn from_output(output: &str, lang: ErrorLang) -> Self {
        let diagnostics = parse_errors(output, lang);
        let root_causes: Vec<RootCause> = diagnostics.iter()
            .map(|d| analyze_root_cause(d, lang))
            .collect();
        let err = diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
        let warn = diagnostics.iter().filter(|d| d.severity == Severity::Warning).count();
        let summary = if err > 0 {
            format!("{} errors, {} warnings", err, warn)
        } else if warn > 0 {
            format!("{} warnings", warn)
        } else {
            "No errors found".to_string()
        };
        Self { diagnostics, root_causes, summary }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_e0308() {
        let out = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n   |\n10 |     let x: &str = y;\n   |         ------    ^ expected `&str`, found `String`";
        let diags = parse_rust_errors(out);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_deref(), Some("E0308"));
        assert_eq!(diags[0].file, "src/main.rs");
        assert_eq!(diags[0].line, Some(10));
    }

    #[test]
    fn test_parse_rust_missing_import() {
        let out = "error[E0433]: failed to resolve: `foo` not found in this crate\n  --> src/lib.rs:5:12\n   |\n5  | use crate::foo;\n   |            ^^^ not found in this crate";
        let diags = parse_rust_errors(out);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_deref(), Some("E0433"));
    }

    #[test]
    fn test_root_cause_missing_import() {
        let diag = Diagnostic {
            file: "src/main.rs".into(),
            line: Some(5), column: Some(12),
            code: Some("E0433".into()),
            severity: Severity::Error,
            message: "failed to resolve: `foo` not found in this crate".into(),
            raw_line: String::new(),
            suggestion: None,
        };
        let rc = analyze_root_cause(&diag, ErrorLang::Rust);
        assert_eq!(rc.category, RootCauseCategory::MissingImport);
        assert!(rc.confidence > 0.9);
    }

    #[test]
    fn test_extract_missing_name() {
        assert_eq!(extract_missing_name("use of undeclared type `MyStruct`"), "MyStruct");
        assert_eq!(extract_missing_name("cannot find `foo` in this scope"), "foo");
    }

    #[test]
    fn test_parse_python_error() {
        let out = "Traceback (most recent call last):\n  File \"foo.py\", line 10, in <module>\n    foo()\nNameError: name 'x' is not defined";
        let diags = parse_python_errors(out);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_deref(), Some("NameError"));
    }

    #[test]
    fn test_diagnostic_report() {
        let out = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n   |\n10 |     x";
        let report = DiagnosticReport::from_output(out, ErrorLang::Rust);
        assert_eq!(report.diagnostics.len(), 1);
        assert!(report.summary.contains("1 errors"));
    }
}
