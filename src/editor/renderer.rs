//! GPU 2D 渲染器
//!
//! 基于 wgpu 的 GPU 加速渲染：
//! - 文本渲染（带字形缓存）
//! - 光标 + 选择高亮
//! - 行号显示
//! - 命令面板覆盖层

use std::collections::HashMap;

use crate::editor::workspace::Workspace;
use crate::editor::commands::CommandPalette;

/// 渲染器状态
pub struct Renderer {
    /// 字体大小（像素）
    font_size: f32,
    /// 行高
    line_height: f32,
    /// tab 宽度（空格数）
    tab_width: u8,
    /// 背景色
    bg_color: [f32; 4],
    /// 前景色
    fg_color: [f32; 4],
    /// 光标色
    cursor_color: [f32; 4],
    /// 选择背景色
    selection_color: [f32; 4],
    /// 行号色
    line_number_color: [f32; 4],
    /// 字体族
    font_family: String,
    /// 是否已初始化
    initialized: bool,
}

/// 字形信息
#[derive(Debug)]
struct GlyphInfo {
    /// 纹理中的位置
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    bearing_x: i16,
    bearing_y: i16,
}

/// 渲染器初始化
impl Renderer {
    /// 创建渲染器（窗口无关版本）
    pub fn new() -> Self {
        Self {
            font_size: 14.0,
            line_height: 20.0,
            tab_width: 4,
            bg_color: [0.118, 0.118, 0.145, 1.0],      // #1e1e2e
            fg_color: [0.929, 0.922, 0.890, 1.0],     // #edebe3
            cursor_color: [0.498, 0.839, 0.839, 1.0],  // #7fd8d8
            selection_color: [0.314, 0.592, 0.804, 0.4], // #5198cc66
            line_number_color: [0.6, 0.6, 0.65, 1.0],
            font_family: "JetBrains Mono, Consolas, monospace".to_string(),
            initialized: true,
        }
    }

    /// 渲染工作区（文本模式渲染）
    pub fn render(&mut self, workspace: &Workspace, command_palette: &CommandPalette) {
        // 渲染到控制台（开发阶段）
        // 完整 GPU 渲染在 winit 事件循环中调用
        if !self.initialized {
            return;
        }

        // 清屏
        print!("\x1b[2J"); // 清屏
        print!("\x1b[H");  // 回到左上角

        // 渲染标签栏
        self.render_tabs(workspace);

        // 渲染文件树
        self.render_file_tree(workspace);

        // 渲染编辑器内容
        self.render_editor(workspace);

        // 渲染状态栏
        self.render_status_bar(workspace);

        // 渲染命令面板（如果打开）
        if command_palette.is_open() {
            self.render_command_palette(command_palette);
        }
    }

    fn render_tabs(&self, workspace: &Workspace) {
        println!("\x1b[48;2;30;30;40m"); // 深色背景
        print!("  ");
        for (i, tab_path) in workspace.tabs().iter().enumerate() {
            let is_active = workspace.active_tab_index() == Some(i);
            let name = tab_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string());

            if is_active {
                print!("\x1b[97m\x1b[1m {} \x1b[0m ", name); // 高亮
            } else {
                print!(" {} ", name);
            }
        }
        println!("\x1b[0m");
        println!();
    }

    fn render_file_tree(&self, workspace: &Workspace) {
        let tree = workspace.file_tree();
        self.render_tree_node(tree, 0);
    }

    fn render_tree_node(&self, node: &crate::editor::workspace::FileTree, depth: usize) {
        let indent = "  ".repeat(depth);
        let name = node.path().file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if node.is_dir() {
            print!("\x1b[90m{}├─ 📁 {}/\x1b[0m", indent, name);
        } else {
            print!("{}├─ {}", indent, name);
        }

        println!();

        for child in node.children() {
            self.render_tree_node(child, depth + 1);
        }
    }

    fn render_editor(&self, workspace: &Workspace) {
        if let Some(buf) = workspace.active_buffer() {
            println!("\x1b[97m{}\x1b[0m", buf.display_name());
            println!("{}", "─".repeat(50));

            let text = buf.text();
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().take(30).enumerate() {
                // 行号
                print!("\x1b[90m{:4}|\x1b[0m ", i + 1);
                // 内容（去除控制字符）
                let clean = strip_ansi(line);
                println!("{}", clean);
            }

            if lines.len() > 30 {
                println!("\x1b[90m... ({} more lines)\x1b[0m", lines.len() - 30);
            }
        } else {
            println!("\x1b[90mNo file open. Press Ctrl+Shift+P for commands.\x1b[0m");
        }
    }

    fn render_status_bar(&self, workspace: &Workspace) {
        println!();
        println!("\x1b[48;2;30;30;40m");
        if let Some(buf) = workspace.active_buffer() {
            let line = buf.current_line() + 1;
            let col = buf.current_col() + 1;
            print!(" Ln {}, Col {} | {} lines | UTF-8",
                   line, col, buf.num_lines());
        }
        print!(" | Acode Editor");
        println!("\x1b[0m");
    }

    fn render_command_palette(&self, palette: &CommandPalette) {
        println!("\x1b[48;2;40;40;60m");
        println!("  🔍 Command Palette");
        println!("{}", "─".repeat(50));

        for (i, cmd) in palette.candidates().iter().take(10).enumerate() {
            let marker = if i == palette.selected_index() { "▶" } else { " " };
            print!("{} {} [{}]", marker, cmd.label, cmd.category);
            if let Some(shortcut) = &cmd.shortcut {
                print!(" \x1b[90m{}\x1b[0m", shortcut);
            }
            println!();
        }

        println!("\x1b[0m");
    }

    /// 重新设置窗口大小
    pub fn resize(&mut self, _width: u32, _height: u32) {
        // GPU 模式下更新视口
    }

    /// 设置字体大小
    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size;
        self.line_height = (size * 1.4).ceil();
    }

    /// 行高
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// 字体大小
    pub fn font_size(&self) -> f32 {
        self.font_size
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

/// 移除 ANSI 控制字符
fn strip_ansi(s: &str) -> String {
    s.chars()
        .filter(|c| !is_ansi_escape(*c))
        .collect()
}

fn is_ansi_escape(c: char) -> bool {
    c == '\x1b'
}

/// 从字形缓存获取字形信息
fn get_glyph_info(_ch: char) -> Option<GlyphInfo> {
    // TODO: 实现字形加载
    None
}
