//! 文本缓冲区 — 基于 rope 数据结构
//!
//! Rope 特点：
//! - O(log n) 插入/删除（适合大文件）
//! - 支持协同编辑（CRDT 基础）
//! - 内存效率高（不需要复制整个文件）

use ropey::Rope;
use std::path::PathBuf;

/// 单个缓冲区的内容
#[derive(Debug, Clone)]
pub struct Buffer {
    /// 底层 Rope 文本
    rope: Rope,
    /// 文件路径（如果是从文件加载的）
    path: Option<PathBuf>,
    /// 是否已修改
    modified: bool,
    /// 光标位置（字节偏移）
    cursor: usize,
    /// 选择范围（开始, 结束）
    selection: Option<(usize, usize)>,
    /// 文件编码
    encoding: &'static str,
}

impl Buffer {
    /// 新建空缓冲区
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            modified: false,
            cursor: 0,
            selection: None,
            encoding: "utf-8",
        }
    }

    /// 从文件加载
    pub fn from_file(path: PathBuf) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(&path)?;
        let rope = Rope::from_str(&content);
        Ok(Self {
            rope,
            path: Some(path),
            modified: false,
            cursor: 0,
            selection: None,
            encoding: "utf-8",
        })
    }

/// 保存到文件
    pub fn save(&mut self) -> std::io::Result<()> {
        if let Some(path) = self.path.clone() {
            self.save_to(&path)?;
        }
        Ok(())
    }

    /// 另存为
    pub fn save_to(&mut self, path: &PathBuf) -> std::io::Result<()> {
        std::fs::write(path, self.rope.to_string())?;
        self.path = Some(path.clone());
        self.modified = false;
        Ok(())
    }

    /// 插入文本（光标位置）
    pub fn insert(&mut self, text: &str) {
        self.rope.insert(self.cursor, text);
        self.modified = true;
        self.cursor += text.len();
    }

    /// 删除选中文本
    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection.take() {
            self.rope.remove(start..end);
            self.cursor = start;
            self.modified = true;
        }
    }

    /// 在指定位置插入
    pub fn insert_at(&mut self, pos: usize, text: &str) {
        self.rope.insert(pos, text);
        self.modified = true;
    }

    /// 删除指定范围
    pub fn remove_range(&mut self, range: std::ops::Range<usize>) {
        self.rope.remove(range);
        self.modified = true;
    }

    /// 获取行数
    pub fn num_lines(&self) -> usize {
        self.rope.lines().count()
    }

    /// 获取总字节数
    pub fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// 获取文本内容
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// 获取行（0-indexed）
    pub fn line(&self, idx: usize) -> String {
        self.rope.line(idx).to_string()
    }

    /// 获取行范围（返回起始字节偏移, 结束字节偏移）
    pub fn line_range(&self, idx: usize) -> std::ops::Range<usize> {
        self.rope.line_to_char(idx)..self.rope.line_to_char(idx + 1)
    }

    /// 移动光标
    pub fn move_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.rope.len_bytes());
        self.selection = None;
    }

    /// 移动光标（行/列）
    pub fn move_to(&mut self, line: usize, col: usize) {
        let char_offset = self.rope.line_to_char(line) + col.min(self.rope.line(line).len_chars());
        self.move_cursor(char_offset);
    }

    /// 获取光标位置
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// 设置选择
    pub fn set_selection(&mut self, start: usize, end: usize) {
        self.selection = Some((start.min(end), end.max(start)));
    }

    /// 获取选择范围
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.selection
    }

    /// 清空选择
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// 获取当前行号（0-indexed）
    pub fn current_line(&self) -> usize {
        self.rope.char_to_line(self.cursor)
    }

    /// 获取当前列号（0-indexed）
    pub fn current_col(&self) -> usize {
        let line_start = self.rope.line_to_char(self.current_line());
        self.cursor - line_start
    }

    /// 向上移动光标一行
    pub fn cursor_up(&mut self) {
        let line = self.current_line();
        if line > 0 {
            let col = self.current_col();
            let new_line = line - 1;
            let new_col = col.min(self.rope.line(new_line).len_chars());
            self.move_to(new_line, new_col);
        }
    }

    /// 向下移动光标一行
    pub fn cursor_down(&mut self) {
        let line = self.current_line();
        let num_lines = self.num_lines();
        if line < num_lines - 1 {
            let col = self.current_col();
            let new_line = line + 1;
            let new_col = col.min(self.rope.line(new_line).len_chars());
            self.move_to(new_line, new_col);
        }
    }

    /// 向左移动光标一列
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.selection = None;
        }
    }

    /// 向右移动光标一列
    pub fn cursor_right(&mut self) {
        if self.cursor < self.rope.len_bytes() {
            self.cursor += 1;
            self.selection = None;
        }
    }

    /// 跳到行首
    pub fn cursor_line_start(&mut self) {
        let line = self.current_line();
        self.cursor = self.rope.line_to_char(line);
    }

    /// 跳到行尾
    pub fn cursor_line_end(&mut self) {
        let line = self.current_line();
        self.cursor = self.rope.line_to_char(line + 1).saturating_sub(1);
    }

    /// 获取文件路径
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// 是否已修改
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// 获取文件名前缀（用于标签显示）
    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// 搜索文本
    pub fn find(&self, query: &str, from: usize) -> Option<usize> {
        let text = self.rope.to_string();
        text[from..].find(query).map(|i| from + i)
    }

    /// 替换文本
    pub fn replace(&mut self, range: std::ops::Range<usize>, replacement: &str) {
        self.rope.remove(range.clone());
        self.rope.insert(range.start, replacement);
        self.modified = true;
    }

    /// 全量替换
    pub fn replace_all(&mut self, from: &str, to: &str) {
        let text = self.rope.to_string();
        let new_text = text.replace(from, to);
        if text != new_text {
            self.rope = Rope::from_str(&new_text);
            self.modified = true;
        }
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}
