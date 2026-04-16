//! Editor 模块 — 独立 AI 代码编辑器
//!
//! 架构参考：Zed（Rust GPU渲染）+ Claw Code
//!
//! 核心组件：
//! - Buffer: 文本缓冲（rope 数据结构，支持大文件）
//! - Renderer: 终端渲染（开发阶段）
//! - Workspace: 工作区（多标签 + 文件树）
//! - CommandPalette: 命令面板
//! - AICompletor: AI 内联补全

pub mod buffer;
pub mod renderer;
pub mod workspace;
pub mod commands;
pub mod ai;

pub use buffer::Buffer;
pub use renderer::Renderer;
pub use workspace::Workspace;
pub use commands::CommandPalette;
pub use ai::AICompletor;

/// 编辑器应用入口
use std::path::PathBuf;
use std::sync::Mutex;

/// 编辑器主应用
pub struct Editor {
    /// 渲染器
    renderer: Mutex<Renderer>,
    /// 工作区
    workspace: Workspace,
    /// 命令面板
    command_palette: CommandPalette,
    /// AI 补全器
    ai: AICompletor,
    /// 事件控制标志
    running: bool,
}

impl Editor {
    pub fn new(workdir: Option<PathBuf>) -> Self {
        Self {
            renderer: Mutex::new(Renderer::new()),
            workspace: Workspace::new(workdir),
            command_palette: CommandPalette::new(),
            ai: AICompletor::new(),
            running: true,
        }
    }

    /// 渲染当前帧
    pub fn render(&self) {
        if let Ok(mut renderer) = self.renderer.lock() {
            renderer.render(&self.workspace, &self.command_palette);
        }
    }

    /// 处理键盘事件（简化版，终端输入）
    pub fn handle_input(&mut self, input: &str) {
        match input {
            "q" | "quit" => {
                self.running = false;
            }
            "tab_next" => {
                self.workspace.next_tab();
            }
            "tab_prev" => {
                self.workspace.prev_tab();
            }
            "cmd" => {
                self.command_palette.toggle();
            }
            "save" => {
                self.workspace.save_active();
            }
            "new" => {
                self.workspace.new_buffer();
            }
            "open" => {
                // TODO: 文件选择
            }
            _ => {
                // 普通输入
                if !self.command_palette.is_open() {
                    self.workspace.insert_text(input);
                }
            }
        }
    }

    /// 处理文件拖放
    pub fn handle_drop(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            self.workspace.open_file(path);
        }
    }

    /// 是否运行中
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 停止编辑器
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// 获取工作区
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// 获取命令面板
    pub fn command_palette(&self) -> &CommandPalette {
        &self.command_palette
    }

    /// 获取 AI 补全器
    pub fn ai(&self) -> &AICompletor {
        &self.ai
    }
}

/// 启动编辑器（开发阶段：终端模式）
pub fn run(workdir: Option<PathBuf>) {
    let mut editor = Editor::new(workdir);
    editor.render();

    println!();
    println!("\x1b[36m=== Acode Editor 终端模式 ===\x1b[0m");
    println!("快捷键:");
    println!("  tab_next / tab_prev  — 切换标签");
    println!("  cmd                  — 切换命令面板");
    println!("  save                 — 保存当前文件");
    println!("  new                  — 新建缓冲区");
    println!("  q / quit             — 退出");
    println!("  直接输入              — 插入文本");
    println!();

    // 简单的终端输入循环
    use std::io::{self, Write};
    loop {
        print!("\x1b[90macode> \x1b[0m");
        io::stdout().flush().ok();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() {
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        editor.handle_input(input);
        editor.render();

        if !editor.is_running() {
            break;
        }
    }
}

/// 启动 GUI 编辑器（需要 winit/wgpu）
#[cfg(feature = "gui")]
pub fn run_gui(workdir: Option<PathBuf>) {
    // TODO: 实现 GUI 模式
    run(workdir);
}
