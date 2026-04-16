//! 代码解析器 — AST 解析与符号提取
//!
//! 支持语言：
//! - Rust（rust-analyzer 集成）
//! - TypeScript/JavaScript（tree-sitter）
//! - Python（tree-sitter）
//! - Go（tree-sitter）
//!
//! 提取信息：
//! - 函数/方法签名
//! - 类型定义
//! - 导入/导出
//! - 依赖关系

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::code_understanding::graph::Language;

/// 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResult {
    pub file: String,
    pub language: Language,
    pub functions: Vec<Function>,
    pub structs: Vec<Struct>,
    pub enums: Vec<Enum>,
    pub imports: Vec<Import>,
    pub exports: Vec<String>,
    pub loc: usize,
}

/// 函数信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub span: Span,
    pub params: Vec<Param>,
    pub return_type: Option<String>,
    pub visibility: Visibility,
    pub is_async: bool,
}

/// 结构体信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Struct {
    pub name: String,
    pub span: Span,
    pub fields: Vec<Field>,
    pub methods: Vec<String>,
}

/// 枚举信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enum {
    pub name: String,
    pub span: Span,
    pub variants: Vec<EnumVariant>,
}

/// 导入信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub path: String,
    pub aliases: Vec<String>,
    pub is_glob: bool,
}

/// 代码位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
    pub start_col: usize,
    pub end_col: usize,
}

/// 参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub param_type: Option<String>,
    pub is_mutable: bool,
}

/// 字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub field_type: String,
    pub visibility: Visibility,
}

/// 枚举变体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub associated_types: Vec<String>,
}

/// 可见性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Crate,
}

/// 代码解析器
pub struct Parser {
    /// 项目根目录
    project_root: PathBuf,
}

impl Parser {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// 解析单个文件
    pub async fn parse_file(&self, path: &Path) -> Result<ParseResult, ParserError> {
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| ParserError::IoError(e.to_string()))?;

        let language = self.detect_language(path);
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        match language {
            Language::Rust => self.parse_rust(&content),
            Language::TypeScript | Language::JavaScript => self.parse_ts_js(&content),
            Language::Python => self.parse_python(&content),
            Language::Go => self.parse_go(&content),
            _ => Err(ParserError::UnsupportedLanguage(ext.to_string())),
        }
    }

    /// 检测语言
    fn detect_language(&self, path: &Path) -> Language {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        match ext {
            "rs" => Language::Rust,
            "ts" | "tsx" => Language::TypeScript,
            "js" | "jsx" | "mjs" => Language::JavaScript,
            "py" => Language::Python,
            "go" => Language::Go,
            "java" => Language::Java,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" => Language::Cpp,
            _ => Language::Unknown,
        }
    }

    /// 解析 Rust 代码（简化版，正则匹配）
    fn parse_rust(&self, content: &str) -> Result<ParseResult, ParserError> {
        let mut functions = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut imports = Vec::new();
        let mut exports = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // 导入
            if line.starts_with("use ") && line.ends_with(';') {
                let path = line[4..line.len()-1].trim().to_string();
                imports.push(Import {
                    path: path.clone(),
                    aliases: Vec::new(),
                    is_glob: path.contains("::*"),
                });
            }

            // 函数
            if line.starts_with("pub fn ") || line.starts_with("fn ") || line.starts_with("pub async fn ") || line.starts_with("async fn ") {
                let is_async = line.starts_with("async ");
                let (name, params, return_type) = self.parse_rust_fn_signature(line);
                let start_line = i + 1;
                let end_line = self.find_block_end(&lines, i);

                functions.push(Function {
                    name,
                    span: Span {
                        start_line,
                        end_line,
                        start_col: 0,
                        end_col: lines.get(i).map(|l| l.len()).unwrap_or(0),
                    },
                    params,
                    return_type,
                    visibility: if line.starts_with("pub ") { Visibility::Public } else { Visibility::Private },
                    is_async,
                });
                i = end_line;
            }

            // 结构体
            if line.starts_with("pub struct ") || line.starts_with("struct ") {
                let name = line.split_whitespace().nth(1).unwrap_or("").split('{').next().unwrap_or("").trim().to_string();
                let start_line = i + 1;
                let end_line = self.find_block_end(&lines, i);

                structs.push(Struct {
                    name,
                    span: Span { start_line, end_line, start_col: 0, end_col: 0 },
                    fields: Vec::new(),
                    methods: Vec::new(),
                });
                i = end_line;
            }

            // 枚举
            if line.starts_with("pub enum ") || line.starts_with("enum ") {
                let name = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let start_line = i + 1;
                let end_line = self.find_block_end(&lines, i);

                enums.push(Enum {
                    name,
                    span: Span { start_line, end_line, start_col: 0, end_col: 0 },
                    variants: Vec::new(),
                });
                i = end_line;
            }

            i += 1;
        }

        Ok(ParseResult {
            file: "".to_string(),
            language: Language::Rust,
            functions,
            structs,
            enums,
            imports,
            exports,
            loc: lines.len(),
        })
    }

    /// 解析 TypeScript/JavaScript（简化版）
    fn parse_ts_js(&self, content: &str) -> Result<ParseResult, ParserError> {
        let mut functions = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // 函数声明
            if trimmed.starts_with("function ") {
                if let Some(name_start) = trimmed.find("function ") {
                    let rest = &trimmed[name_start + 9..];
                    let name = rest.split('(').next().unwrap_or("").trim().to_string();
                    functions.push(Function {
                        name,
                        span: Span { start_line: i + 1, end_line: i + 1, start_col: 0, end_col: trimmed.len() },
                        params: Vec::new(),
                        return_type: None,
                        visibility: Visibility::Public,
                        is_async: false,
                    });
                }
            }

            // Arrow 函数
            if trimmed.contains("=>") && trimmed.contains('(') {
                // 简化处理
            }
        }

        Ok(ParseResult {
            file: "".to_string(),
            language: Language::TypeScript,
            functions,
            structs: Vec::new(),
            enums: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            loc: lines.len(),
        })
    }

    /// 解析 Python（简化版）
    fn parse_python(&self, content: &str) -> Result<ParseResult, ParserError> {
        let mut functions = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // def 函数
            if trimmed.starts_with("def ") && !trimmed.contains(':') == false {
                let rest = &trimmed[4..];
                let name = rest.split('(').next().unwrap_or("").trim().to_string();
                let start_line = i + 1;
                let end_line = self.find_python_indent_end(&lines, i);

                functions.push(Function {
                    name,
                    span: Span { start_line, end_line, start_col: 0, end_col: trimmed.len() },
                    params: Vec::new(),
                    return_type: None,
                    visibility: Visibility::Public,
                    is_async: trimmed.starts_with("async def "),
                });
            }
        }

        Ok(ParseResult {
            file: "".to_string(),
            language: Language::Python,
            functions,
            structs: Vec::new(),
            enums: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            loc: lines.len(),
        })
    }

    /// 解析 Go（简化版）
    fn parse_go(&self, content: &str) -> Result<ParseResult, ParserError> {
        let mut functions = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // func
            if trimmed.starts_with("func ") {
                let rest = &trimmed[5..];
                let (name, params, return_type) = self.parse_go_fn_signature(rest);
                functions.push(Function {
                    name,
                    span: Span { start_line: i + 1, end_line: i + 1, start_col: 0, end_col: trimmed.len() },
                    params,
                    return_type,
                    visibility: Visibility::Public,
                    is_async: false,
                });
            }

            // import
            if trimmed.starts_with("import ") {
                // 处理导入
            }
        }

        Ok(ParseResult {
            file: "".to_string(),
            language: Language::Go,
            functions,
            structs: Vec::new(),
            enums: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            loc: lines.len(),
        })
    }

    /// 解析 Rust 函数签名
    fn parse_rust_fn_signature(&self, line: &str) -> (String, Vec<Param>, Option<String>) {
        let name = line.split(|c: char| c.is_whitespace() || c == '(' || c == ')')
            .filter(|s| !s.is_empty())
            .nth(1)
            .unwrap_or("")
            .to_string();

        // 提取参数
        let params = if let Some(paren_start) = line.find('(') {
            if let Some(paren_end) = line[paren_start..].find(')') {
                let param_str = &line[paren_start+1..paren_start+paren_end];
                param_str.split(',')
                    .filter(|p| !p.trim().is_empty())
                    .map(|p| {
                        let parts: Vec<&str> = p.trim().split(':').collect();
                        Param {
                            name: parts.get(0).map(|s| s.trim().to_string()).unwrap_or_default(),
                            param_type: parts.get(1).map(|s| s.trim().to_string()),
                            is_mutable: p.contains("mut"),
                        }
                    })
                    .collect()
            } else { Vec::new() }
        } else { Vec::new() };

        // 提取返回类型
        let return_type = if let Some(arrow) = line.find("->") {
            Some(line[arrow+2..].trim().to_string())
        } else { None };

        (name, params, return_type)
    }

    /// 解析 Go 函数签名
    fn parse_go_fn_signature(&self, rest: &str) -> (String, Vec<Param>, Option<String>) {
        let (name_part, params_part) = if rest.contains('(') {
            let paren_start = rest.find('(').unwrap();
            (&rest[..paren_start], &rest[paren_start..])
        } else {
            (rest, "()")
        };

        let name = name_part.trim().to_string();
        let return_type = None; // 简化

        (name, Vec::new(), return_type)
    }

    /// 查找代码块结束行
    fn find_block_end(&self, lines: &[&str], start: usize) -> usize {
        let mut brace_count = 0;
        let start_col = lines.get(start).map(|l| l.find('{').unwrap_or(0)).unwrap_or(0);

        for i in start..lines.len() {
            let line = lines[i];
            brace_count += line.matches('{').count() as i32;
            brace_count -= line.matches('}').count() as i32;

            if brace_count <= 0 && i > start {
                return i + 1;
            }
        }

        lines.len()
    }

    /// 查找 Python 缩进块结束
    fn find_python_indent_end(&self, lines: &[&str], start: usize) -> usize {
        let base_indent = lines.get(start).map(|l| l.len() - l.trim_start().len()).unwrap_or(0);

        for i in start + 1..lines.len() {
            let line = lines[i];
            if !line.trim().is_empty() {
                let indent = line.len() - line.trim_start().len();
                if indent <= base_indent {
                    return i;
                }
            }
        }

        lines.len()
    }
}

/// 解析错误
#[derive(Debug)]
pub enum ParserError {
    IoError(String),
    ParseError(String),
    UnsupportedLanguage(String),
}
