//! 工具注册表 — 内置工具定义与执行

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use serde_json::Value;
use tokio::task::JoinSet;
use crate::error::{Error, Result};
use crate::code_understanding::KnowledgeGraph;

#[derive(Debug, Clone)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static str,
    pub examples: Vec<(&'static str, &'static str)>,
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<HashMap<String, Tool>>,
    knowledge_graph: Option<Arc<KnowledgeGraph>>,
}

impl Default for ToolRegistry { fn default() -> Self { Self::new() } }

impl ToolRegistry {
    pub fn new() -> Self {
        let mut tools = HashMap::new();
        tools.insert("read_file".into(), Tool { name: "read_file", description: "读取文件内容", parameters: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("write_file".into(), Tool { name: "write_file", description: "创建或覆盖文件", parameters: r#"{"type":"object","required":["path","content"],"properties":{"path":{"type":"string"},"content":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("search_files".into(), Tool { name: "search_files", description: "在项目中搜索文本", parameters: r#"{"type":"object","properties":{"query":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("list_directory".into(), Tool { name: "list_directory", description: "列出目录内容", parameters: r#"{"type":"object","properties":{"path":{"type":"string"},"recursive":{"type":"boolean"}}}"#, examples: vec![] });
        tools.insert("run_command".into(), Tool { name: "run_command", description: "执行命令行命令", parameters: r#"{"type":"object","required":["command"],"properties":{"command":{"type":"string"},"timeout":{"type":"integer"}}}"#, examples: vec![] });
        tools.insert("git_status".into(), Tool { name: "git_status", description: "查看 Git 状态", parameters: r#"{"type":"object"}"#, examples: vec![] });
        tools.insert("git_diff".into(), Tool { name: "git_diff", description: "查看未提交的变更", parameters: r#"{"type":"object"}"#, examples: vec![] });
        tools.insert("git_log".into(), Tool { name: "git_log", description: "查看提交历史", parameters: r#"{"type":"object"}"#, examples: vec![] });
        tools.insert("grep".into(), Tool { name: "grep", description: "搜索文件内容", parameters: r#"{"type":"object","properties":{"pattern":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("web_search".into(), Tool { name: "web_search", description: "联网搜索", parameters: r#"{"type":"object","properties":{"query":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("query_keyword".into(), Tool { name: "query_keyword", description: "根据关键词查询知识图谱", parameters: r#"{"type":"object","required":["keyword"],"properties":{"keyword":{"type":"string"},"min_confidence":{"type":"number"}}}"#, examples: vec![] });
        tools.insert("query_file".into(), Tool { name: "query_file", description: "查询文件相关的所有代码节点", parameters: r#"{"type":"object","required":["file_path"],"properties":{"file_path":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("query_function".into(), Tool { name: "query_function", description: "查询函数的定义和调用关系", parameters: r#"{"type":"object","required":["function_name"],"properties":{"function_name":{"type":"string"},"include_callers":{"type":"boolean"}}}"#, examples: vec![] });
        tools.insert("query_type".into(), Tool { name: "query_type", description: "查询类型的定义位置和实现", parameters: r#"{"type":"object","required":["type_name"],"properties":{"type_name":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("query_architecture".into(), Tool { name: "query_architecture", description: "查询项目整体架构视图", parameters: r#"{"type":"object","properties":{"depth":{"type":"integer"},"focus_module":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("trace_impact".into(), Tool { name: "trace_impact", description: "追踪变更的影响范围", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"},"max_depth":{"type":"integer"}}}"#, examples: vec![] });
        tools.insert("trace_dependencies".into(), Tool { name: "trace_dependencies", description: "追踪代码依赖关系", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("trace_call_chain".into(), Tool { name: "trace_call_chain", description: "追踪函数调用链", parameters: r#"{"type":"object","required":["from","to"],"properties":{"from":{"type":"string"},"to":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("trace_data_flow".into(), Tool { name: "trace_data_flow", description: "追踪数据流", parameters: r#"{"type":"object","required":["variable","start"],"properties":{"variable":{"type":"string"},"start":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("assess_risk".into(), Tool { name: "assess_risk", description: "评估变更风险等级", parameters: r#"{"type":"object","required":["target","change_type"],"properties":{"target":{"type":"string"},"change_type":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("assess_change_complexity".into(), Tool { name: "assess_change_complexity", description: "评估变更复杂度", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("assess_architectural_fitness".into(), Tool { name: "assess_architectural_fitness", description: "评估变更是否符合架构规范", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("find_usages".into(), Tool { name: "find_usages", description: "查找符号的所有使用位置", parameters: r#"{"type":"object","required":["symbol"],"properties":{"symbol":{"type":"string"},"include_tests":{"type":"boolean"}}}"#, examples: vec![] });
        tools.insert("find_definitions".into(), Tool { name: "find_definitions", description: "查找符号的定义位置", parameters: r#"{"type":"object","required":["symbol"],"properties":{"symbol":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("find_implementations".into(), Tool { name: "find_implementations", description: "查找 trait 的所有实现", parameters: r#"{"type":"object","required":["trait_name"],"properties":{"trait_name":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("analyze_layer".into(), Tool { name: "analyze_layer", description: "分析代码的架构层次", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("analyze_pattern".into(), Tool { name: "analyze_pattern", description: "检测设计模式", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("search_similar_code".into(), Tool { name: "search_similar_code", description: "语义搜索相似代码", parameters: r#"{"type":"object","required":["semantic_query"],"properties":{"semantic_query":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("search_by_semantics".into(), Tool { name: "search_by_semantics", description: "基于语义的代码搜索", parameters: r#"{"type":"object","required":["query"],"properties":{"query":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("search_api_surface".into(), Tool { name: "search_api_surface", description: "搜索模块的公开 API", parameters: r#"{"type":"object","required":["module"],"properties":{"module":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("find_test_coverage".into(), Tool { name: "find_test_coverage", description: "查找测试覆盖情况", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"}}}"#, examples: vec![] });
        tools.insert("find_related_refactors".into(), Tool { name: "find_related_refactors", description: "查找相关重构机会", parameters: r#"{"type":"object","required":["target"],"properties":{"target":{"type":"string"}}}"#, examples: vec![] });
        Self { tools: Arc::new(tools), knowledge_graph: None }
    }

    pub fn with_knowledge_graph(kg: KnowledgeGraph) -> Self {
        let mut this = Self::new();
        this.knowledge_graph = Some(Arc::new(kg));
        this
    }

    pub fn available_tools(&self) -> Vec<crate::llm::LlmTool> {
        self.tools.values().map(|t| crate::llm::LlmTool { name: t.name.to_string(), description: t.description.to_string(), parameters: serde_json::from_str(t.parameters).unwrap_or_else(|_| serde_json::json!({})), }).collect()
    }

    pub async fn execute(&self, name: &str, arguments_json: &str, workdir: &PathBuf) -> Result<String> {
        let start = std::time::Instant::now();
        let result = self.execute_one(name, arguments_json, workdir).await;
        match result {
            Ok(content) => { tracing::debug!("tool {} done {}ms", name, start.elapsed().as_millis()); Ok(content) }
            Err(e) => { tracing::warn!("tool {} failed: {}", name, e); Err(e) }
        }
    }

    pub async fn execute_concurrent(&self, calls: Vec<(String, String)>, workdir: &PathBuf) -> (Vec<ToolResult>, Vec<ToolResult>) {
        let tools = self.tools.clone();
        let mut join_set = JoinSet::new();
        for (name, args) in calls {
            let tools = Arc::clone(&tools);
            let workdir = workdir.clone();
            join_set.spawn(async move {
                let registry = ToolRegistry { tools, knowledge_graph: None };
                let start = std::time::Instant::now();
                let result = registry.execute_one(&name, &args, &workdir).await;
                (name, result, start.elapsed().as_millis() as u64)
            });
        }
        let mut successes = Vec::new();
        let mut failures = Vec::new();
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok((name, Ok(content), ms)) => successes.push(ToolResult { name, result: Ok(content), elapsed_ms: ms }),
                Ok((name, Err(e), ms)) => failures.push(ToolResult { name, result: Err(e), elapsed_ms: ms }),
                Err(e) => tracing::error!("task failed: {}", e),
            }
        }
        (successes, failures)
    }

    async fn execute_one(&self, name: &str, arguments_json: &str, workdir: &PathBuf) -> Result<String> {
        let args: Value = serde_json::from_str(arguments_json).map_err(|e| Error::ToolCallFailed { tool_name: name.into(), reason: format!("parse failed: {}", e) })?;
        match name {
            "read_file" => self.read_file(&args, workdir).await,
            "write_file" => self.write_file(&args, workdir).await,
            "search_files" | "grep" => self.search_files(&args, workdir).await,
            "list_directory" => self.list_directory(&args, workdir).await,
            "run_command" => self.run_command(&args, workdir).await,
            "git_status" => self.git_status(&args, workdir).await,
            "git_diff" => self.git_diff(&args, workdir).await,
            "git_log" => self.git_log(&args, workdir).await,
            "web_search" => self.web_search(&args).await,
            "query_keyword" => self.query_keyword(&args, workdir).await,
            "query_file" => self.query_file(&args, workdir).await,
            "query_function" => self.query_function(&args, workdir).await,
            "query_type" => self.query_type(&args, workdir).await,
            "query_architecture" => self.query_architecture(&args, workdir).await,
            "trace_impact" => self.trace_impact(&args, workdir).await,
            "trace_dependencies" => self.trace_dependencies(&args, workdir).await,
            "trace_call_chain" => self.trace_call_chain(&args, workdir).await,
            "trace_data_flow" => self.trace_data_flow(&args, workdir).await,
            "assess_risk" => self.assess_risk(&args, workdir).await,
            "assess_change_complexity" => self.assess_change_complexity(&args, workdir).await,
            "assess_architectural_fitness" => self.assess_architectural_fitness(&args, workdir).await,
            "find_usages" => self.find_usages(&args, workdir).await,
            "find_definitions" => self.find_definitions(&args, workdir).await,
            "find_implementations" => self.find_implementations(&args, workdir).await,
            "analyze_layer" => self.analyze_layer(&args, workdir).await,
            "analyze_pattern" => self.analyze_pattern(&args, workdir).await,
            "search_similar_code" => self.search_similar_code(&args, workdir).await,
            "search_by_semantics" => self.search_by_semantics(&args, workdir).await,
            "search_api_surface" => self.search_api_surface(&args, workdir).await,
            "find_test_coverage" => self.find_test_coverage(&args, workdir).await,
            "find_related_refactors" => self.find_related_refactors(&args, workdir).await,
            _ => Err(Error::ToolNotFound { tool_name: name.into() }),
        }
    }

    fn resolve_path(path_str: &str, workdir: &PathBuf) -> PathBuf {
        let p = PathBuf::from(path_str); if p.is_absolute() { p } else { workdir.join(p) }
    }

    async fn read_file(&self, args: &Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| Error::ToolCallFailed { tool_name: "read_file".into(), reason: "missing path".into() })?;
        let path = Self::resolve_path(path_str, workdir);
        let content = tokio::fs::read_to_string(&path).await.map_err(|_| Error::FileNotFound { path: path.clone() })?;
        let lines: Vec<&str> = content.lines().collect();
        Ok(format!("[{} lines]\n\n{}", lines.len(), if lines.len() > 100 { lines[..100].join("\n") } else { content }))
    }

    async fn write_file(&self, args: &Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| Error::ToolCallFailed { tool_name: "write_file".into(), reason: "missing path".into() })?;
        let content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| Error::ToolCallFailed { tool_name: "write_file".into(), reason: "missing content".into() })?;
        let path = Self::resolve_path(path_str, workdir);
        if let Some(parent) = path.parent() { tokio::fs::create_dir_all(parent).await.map_err(|e| Error::ToolCallFailed { tool_name: "write_file".into(), reason: format!("mkdir failed: {}", e) })?; }
        tokio::fs::write(&path, content).await?;
        Ok(format!("written: {}", path.display()))
    }

    async fn search_files(&self, args: &Value, workdir: &PathBuf) -> Result<String> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| Error::ToolCallFailed { tool_name: "search_files".into(), reason: "missing query".into() })?;
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let path = Self::resolve_path(path_str, workdir);
        let pattern = regex::Regex::new(&format!("(?i){}", regex::escape(query))).map_err(|e| Error::ToolCallFailed { tool_name: "search_files".into(), reason: format!("bad regex: {}", e) })?;
        let mut results = Vec::new();
        self.search_dir_recursive(&path, &pattern, &mut results, 0, 20)?;
        if results.is_empty() { return Ok(format!("not found: \'{}\'", query)); }
        Ok(format!("{} matches:\n\n{}", results.len(), results.join("\n")))
    }

    fn search_dir_recursive(&self, dir: &Path, re: &regex::Regex, results: &mut Vec<String>, depth: usize, max_depth: usize) -> Result<()> {
        if depth > max_depth { return Ok(()); }
        let mut entries = std::fs::read_dir(dir).map_err(|e| Error::ContextLoadFailed(e.to_string()))?;
        while let Some(entry) = entries.next() {
            if let Ok(entry) = entry {
                let name: String = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || name == "node_modules" || name == "target" || name == ".git" { continue; }
                let path = entry.path();
                if path.is_dir() { self.search_dir_recursive(&path, re, results, depth + 1, max_depth)?; }
                else if let Ok(content) = std::fs::read_to_string(&path) {
                    for (i, line) in content.lines().enumerate() {
                        if re.is_match(line) { results.push(format!("{}:{}: {}", path.strip_prefix(std::env::current_dir().unwrap_or_default()).unwrap_or(&path).display(), i + 1, line.trim())); if results.len() >= 50 { return Ok(()); } }
                    }
                }
            }
        }
        Ok(())
    }

    async fn list_directory(&self, args: &Value, workdir: &PathBuf) -> Result<String> {
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let path = Self::resolve_path(path_str, workdir);
        let mut entries: Vec<_> = std::fs::read_dir(&path).map_err(|e| Error::ContextLoadFailed(e.to_string()))?.filter_map(|e| e.ok()).map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().is_dir())).collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(entries.iter().map(|(n, d)| format!("{} {}/", if *d { "dir" } else { "file" }, n)).collect::<Vec<_>>().join("\n"))
    }

    async fn run_command(&self, args: &Value, _workdir: &PathBuf) -> Result<String> {
        let command = args.get("command").and_then(|v| v.as_str()).ok_or_else(|| Error::ToolCallFailed { tool_name: "run_command".into(), reason: "missing command".into() })?;
        let output = tokio::process::Command::new("sh").arg("-c").arg(command).output().await.map_err(|e| Error::ToolCallFailed { tool_name: "run_command".into(), reason: format!("exec failed: {}", e) })?;
        if !output.status.success() { return Err(Error::ToolCallFailed { tool_name: "run_command".into(), reason: format!("failed {}: {}{}", output.status, String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)) }); }
        Ok(format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)))
    }

    async fn git_status(&self, _args: &Value, workdir: &PathBuf) -> Result<String> {
        let output = tokio::process::Command::new("git").args(["status", "--porcelain"]).current_dir(workdir).output().await.map_err(|e| Error::ToolCallFailed { tool_name: "git_status".into(), reason: format!("git failed: {}", e) })?;
        let s = String::from_utf8_lossy(&output.stdout);
        if s.trim().is_empty() { return Ok("clean".into()); }
        Ok(s.to_string())
    }

    async fn git_diff(&self, _args: &Value, workdir: &PathBuf) -> Result<String> {
        let output = tokio::process::Command::new("git").args(["diff", "--stat"]).current_dir(workdir).output().await.map_err(|e| Error::ToolCallFailed { tool_name: "git_diff".into(), reason: format!("git failed: {}", e) })?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn git_log(&self, _args: &Value, workdir: &PathBuf) -> Result<String> {
        let output = tokio::process::Command::new("git").args(["log", "--oneline", "-10"]).current_dir(workdir).output().await.map_err(|e| Error::ToolCallFailed { tool_name: "git_log".into(), reason: format!("git failed: {}", e) })?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn web_search(&self, args: &Value) -> Result<String> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| Error::ToolCallFailed { tool_name: "web_search".into(), reason: "missing query".into() })?;
        let url = format!("https://duckduckgo.com/html/?q={}", urlencoding::encode(query));
        let resp = reqwest::get(&url).await.map_err(|e| Error::ToolCallFailed { tool_name: "web_search".into(), reason: format!("request failed: {}", e) })?;
        let body = resp.text().await.map_err(|e| Error::ToolCallFailed { tool_name: "web_search".into(), reason: format!("read failed: {}", e) })?;
        let results = "search results unavailable (enable scraper feature)";
        Ok(format!("search: {}\n\n{}", query, if results.is_empty() { "no results".to_string() } else { results.to_string() }))
    }
    async fn query_keyword(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("query_keyword", 0.3).await; Ok(format!("query_keyword: {} nodes found", r.len())) } else { Ok("query_keyword not initialized".into()) } }
    async fn query_file(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("query_file", 0.3).await; Ok(format!("query_file: {} nodes found", r.len())) } else { Ok("query_file not initialized".into()) } }
    async fn query_function(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("query_function", 0.3).await; Ok(format!("query_function: {} nodes found", r.len())) } else { Ok("query_function not initialized".into()) } }
    async fn query_type(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("query_type", 0.3).await; Ok(format!("query_type: {} nodes found", r.len())) } else { Ok("query_type not initialized".into()) } }
    async fn query_architecture(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("query_architecture", 0.3).await; Ok(format!("query_architecture: {} nodes found", r.len())) } else { Ok("query_architecture not initialized".into()) } }
    async fn trace_impact(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("trace_impact", 0.3).await; Ok(format!("trace_impact: {} nodes found", r.len())) } else { Ok("trace_impact not initialized".into()) } }
    async fn trace_dependencies(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("trace_dependencies", 0.3).await; Ok(format!("trace_dependencies: {} nodes found", r.len())) } else { Ok("trace_dependencies not initialized".into()) } }
    async fn trace_call_chain(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("trace_call_chain", 0.3).await; Ok(format!("trace_call_chain: {} nodes found", r.len())) } else { Ok("trace_call_chain not initialized".into()) } }
    async fn trace_data_flow(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("trace_data_flow", 0.3).await; Ok(format!("trace_data_flow: {} nodes found", r.len())) } else { Ok("trace_data_flow not initialized".into()) } }
    async fn assess_risk(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("assess_risk", 0.3).await; Ok(format!("assess_risk: {} nodes found", r.len())) } else { Ok("assess_risk not initialized".into()) } }
    async fn assess_change_complexity(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("assess_change_complexity", 0.3).await; Ok(format!("assess_change_complexity: {} nodes found", r.len())) } else { Ok("assess_change_complexity not initialized".into()) } }
    async fn assess_architectural_fitness(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("assess_architectural_fitness", 0.3).await; Ok(format!("assess_architectural_fitness: {} nodes found", r.len())) } else { Ok("assess_architectural_fitness not initialized".into()) } }
    async fn find_usages(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("find_usages", 0.3).await; Ok(format!("find_usages: {} nodes found", r.len())) } else { Ok("find_usages not initialized".into()) } }
    async fn find_definitions(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("find_definitions", 0.3).await; Ok(format!("find_definitions: {} nodes found", r.len())) } else { Ok("find_definitions not initialized".into()) } }
    async fn find_implementations(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("find_implementations", 0.3).await; Ok(format!("find_implementations: {} nodes found", r.len())) } else { Ok("find_implementations not initialized".into()) } }
    async fn analyze_layer(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("analyze_layer", 0.3).await; Ok(format!("analyze_layer: {} nodes found", r.len())) } else { Ok("analyze_layer not initialized".into()) } }
    async fn analyze_pattern(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("analyze_pattern", 0.3).await; Ok(format!("analyze_pattern: {} nodes found", r.len())) } else { Ok("analyze_pattern not initialized".into()) } }
    async fn search_similar_code(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("search_similar_code", 0.3).await; Ok(format!("search_similar_code: {} nodes found", r.len())) } else { Ok("search_similar_code not initialized".into()) } }
    async fn search_by_semantics(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("search_by_semantics", 0.3).await; Ok(format!("search_by_semantics: {} nodes found", r.len())) } else { Ok("search_by_semantics not initialized".into()) } }
    async fn search_api_surface(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("search_api_surface", 0.3).await; Ok(format!("search_api_surface: {} nodes found", r.len())) } else { Ok("search_api_surface not initialized".into()) } }
    async fn find_test_coverage(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("find_test_coverage", 0.3).await; Ok(format!("find_test_coverage: {} nodes found", r.len())) } else { Ok("find_test_coverage not initialized".into()) } }
    async fn find_related_refactors(&self, args: &Value, _workdir: &PathBuf) -> Result<String> { if let Some(ref kg) = self.knowledge_graph { let r = kg.query("find_related_refactors", 0.3).await; Ok(format!("find_related_refactors: {} nodes found", r.len())) } else { Ok("find_related_refactors not initialized".into()) } }
}

/// Tool execution result
#[derive(Debug)]
pub struct ToolResult { pub name: String, pub result: Result<String>, pub elapsed_ms: u64 }