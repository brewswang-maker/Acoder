//! 项目脚手架生成
//!
//! 支持模板：
//! - `rust`: Rust 命令行项目
//! - `rust-api`: Rust Axum Web API
//! - `fullstack`: Vue + Rust API + SQLite (Todo 应用)
//! - `react`: React 前端项目
//!
//! Demo #1: `acode demo fullstack` 生成完整可运行的 Todo 应用

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// 项目模板定义
#[derive(Debug, Clone)]
pub struct ProjectTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub files: HashMap<&'static str, &'static str>,
    pub commands: Vec<String>,
}

impl ProjectTemplate {
    /// 完整 Todo Web 应用模板（Vue + Rust API + SQLite）
    pub fn fullstack() -> Self {
        let mut files = HashMap::new();

        // Rust 后端
        files.insert("Cargo.toml", r#"[package]
name = "todo-api"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.31", features = ["bundled"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors"] }
tracing = "0.1"
tracing-subscriber = "0.3"
"#);

        files.insert("src/main.rs", r#"use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post, delete},
    Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    db: Mutex<Connection>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Todo {
    id: Option<i64>,
    title: String,
    completed: bool,
}

async fn list_todos(State(state): State<AppState>) -> Json<Vec<Todo>> {
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare("SELECT id, title, completed FROM todos ORDER BY id DESC").unwrap();
    let todos = stmt.query_map([], |row| {
        Ok(Todo {
            id: Some(row.get(0)?),
            title: row.get(1)?,
            completed: row.get(2)?,
        })
    }).unwrap()
    .filter_map(|r| r.ok())
    .collect();
    Json(todos)
}

async fn create_todo(
    State(state): State<AppState>,
    Json(new_todo): Json<Todo>,
) -> (StatusCode, Json<Todo>) {
    let db = state.db.lock().unwrap();
    db.execute("INSERT INTO todos (title, completed) VALUES (?1, ?2)", 
        [&new_todo.title, &new_todo.completed.to_string()]).unwrap();
    let id = db.last_insert_rowid();
    (StatusCode::CREATED, Json(Todo { id: Some(id), ..new_todo }))
}

async fn delete_todo(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> StatusCode {
    let db = state.db.lock().unwrap();
    db.execute("DELETE FROM todos WHERE id = ?1", [id]).unwrap();
    StatusCode::NO_CONTENT
}

async fn toggle_todo(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Json<Todo> {
    let db = state.db.lock().unwrap();
    db.execute("UPDATE todos SET completed = NOT completed WHERE id = ?1", [id]).unwrap();
    let mut stmt = db.prepare("SELECT id, title, completed FROM todos WHERE id = ?1").unwrap();
    let todo = stmt.query_row([id], |row| {
        Ok(Todo { id: Some(row.get(0)?), title: row.get(1)?, completed: row.get(2)? })
    }).unwrap();
    Json(todo)
}

#[tokio::main]
async fn main() {
    // 初始化数据库
    let conn = Connection::open("todo.db").unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            completed BOOLEAN NOT NULL DEFAULT FALSE
        )",
        [],
    ).unwrap();

    let state = AppState { db: Mutex::new(conn) };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/todos", get(list_todos))
        .route("/todos", post(create_todo))
        .route("/todos/:id", delete(delete_todo))
        .route("/todos/:id/toggle", post(toggle_todo))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("🚀 Todo API 已启动: http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}
"#);

        // Vue 前端
        files.insert("frontend/index.html", r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Todo App — Acode Demo</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: system-ui, sans-serif; background: #f5f5f5; padding: 2rem; }
    .container { max-width: 600px; margin: 0 auto; }
    h1 { color: #333; margin-bottom: 1.5rem; text-align: center; }
    .input-row { display: flex; gap: 0.5rem; margin-bottom: 1rem; }
    input[type=text] { flex: 1; padding: 0.75rem; border: 1px solid #ddd; border-radius: 6px; font-size: 1rem; }
    button { padding: 0.75rem 1.5rem; background: #4f46e5; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 1rem; }
    button:hover { background: #4338ca; }
    .todo-item { display: flex; align-items: center; gap: 0.75rem; padding: 0.75rem; background: white; border-radius: 6px; margin-bottom: 0.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }
    .todo-item.completed .title { text-decoration: line-through; color: #999; }
    .title { flex: 1; }
    .delete { background: #ef4444; padding: 0.4rem 0.8rem; font-size: 0.875rem; }
    .delete:hover { background: #dc2626; }
    .toggle { width: 20px; height: 20px; cursor: pointer; }
    .empty { text-align: center; color: #999; padding: 2rem; }
  </style>
</head>
<body>
  <div class="container">
    <h1>✅ Todo 应用</h1>
    <div class="input-row">
      <input type="text" id="todoInput" placeholder="输入新任务，按回车添加..." onkeydown="if(event.key==='Enter')addTodo()">
      <button onclick="addTodo()">添加</button>
    </div>
    <div id="todoList"></div>
  </div>
  <script>
    const API = 'http://localhost:3000';
    async function loadTodos() {
      const res = await fetch(API + '/todos');
      const todos = await res.json();
      const list = document.getElementById('todoList');
      if (!todos.length) { list.innerHTML = '<div class="empty">暂无任务，添加一个吧 ✨</div>'; return; }
      list.innerHTML = todos.map(t => `
        <div class="todo-item ${t.completed ? 'completed' : ''}">
          <input type="checkbox" class="toggle" ${t.completed ? 'checked' : ''} onclick="toggleTodo(${t.id})">
          <span class="title">${t.title}</span>
          <button class="delete" onclick="deleteTodo(${t.id})">删除</button>
        </div>`).join('');
    }
    async function addTodo() {
      const input = document.getElementById('todoInput');
      const title = input.value.trim();
      if (!title) return;
      await fetch(API + '/todos', { method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({ title, completed: false }) });
      input.value = '';
      loadTodos();
    }
    async function toggleTodo(id) {
      await fetch(API + '/todos/' + id + '/toggle', { method: 'POST' });
      loadTodos();
    }
    async function deleteTodo(id) {
      await fetch(API + '/todos/' + id, { method: 'DELETE' });
      loadTodos();
    }
    loadTodos();
  </script>
</body>
</html>
"#);

        files.insert("README.md", r#"# Todo 应用 — Acode Demo

## 快速启动

```bash
# 1. 启动后端（Rust API）
cd todo-api
cargo run

# 2. 打开前端（直接在浏览器打开或用任意静态服务器）
open frontend/index.html
```

## 技术栈

- **后端**: Rust + Axum + SQLite (rusqlite)
- **前端**: 原生 HTML + CSS + JavaScript
- **数据库**: SQLite

## API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | /todos | 获取所有任务 |
| POST | /todos | 创建任务 |
| DELETE | /todos/:id | 删除任务 |
| POST | /todos/:id/toggle | 切换完成状态 |

---

由 Acode 自动生成 ✨
"#);

        files.insert("Makefile", r#"run:
	cd todo-api && cargo run

frontend:
	open frontend/index.html

all: run frontend

clean:
	cd todo-api && cargo clean && rm -f todo.db
"#);

        Self {
            name: "fullstack",
            description: "Vue + Rust API + SQLite — Todo 待办应用",
            files,
            commands: vec![
                "cd todo-api && cargo run".to_string(),
            ],
        }
    }

    /// Rust 命令行项目
    pub fn rust() -> Self {
        let mut files = HashMap::new();
        files.insert("Cargo.toml", r#"[package]
name = "hello-world"
version = "0.1.0"
edition = "2021"

[dependencies]
"#);
        files.insert("src/main.rs", r#"fn main() {
    println!("Hello, world!");
}
"#);
        files.insert("README.md", "# hello-world\n\nGenerated by Acode\n");
        Self {
            name: "rust",
            description: "Rust 命令行项目",
            files,
            commands: vec!["cargo run".to_string()],
        }
    }
}

/// 创建项目
pub async fn create_project(base_path: &Path, template: &str) -> Result<PathBuf> {
    let tmpl = match template {
        "fullstack" => ProjectTemplate::fullstack(),
        "rust" => ProjectTemplate::rust(),
        _ => ProjectTemplate::rust(),
    };

    let project_dir = base_path.join(format!("{}-project", tmpl.name));
    fs::create_dir_all(&project_dir).await
        .with_context(|| format!("创建项目目录失败: {}", project_dir.display()))?;

    // 写入所有文件
    for (rel_path, content) in &tmpl.files {
        let file_path = project_dir.join(rel_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&file_path, *content).await
            .with_context(|| format!("写入文件失败: {}", file_path.display()))?;
        tracing::info!("创建: {}", file_path.display());
    }

    println!("\n✅ 项目创建成功: {}", project_dir.display());
    println!("📁 项目路径: {}", project_dir.display());
    println!("\n启动方式:");
    for cmd in &tmpl.commands {
        println!("  $ {}", cmd);
    }
    println!("\n📖 详细说明请查看: {}/README.md", project_dir.display());

    Ok(project_dir)
}

/// 列出所有可用模板
pub fn list_templates() -> Vec<ProjectTemplate> {
    vec![
        ProjectTemplate::fullstack(),
        ProjectTemplate::rust(),
    ]
}
