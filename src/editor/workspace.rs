//! 工作区 — 多标签 + 文件树
//!
//! 管理打开的缓冲区列表，支持多标签编辑

use std::path::PathBuf;
use std::collections::HashMap;

use crate::editor::buffer::Buffer;

/// 工作区
#[derive(Debug)]
pub struct Workspace {
    /// 所有打开的缓冲区（path → Buffer）
    buffers: HashMap<PathBuf, Buffer>,
    /// 标签页顺序
    tabs: Vec<PathBuf>,
    /// 当前激活的标签
    active_tab: Option<usize>,
    /// 工作目录
    workdir: Option<PathBuf>,
    /// 文件树根节点
    file_tree: FileTree,
    /// 是否显示快速打开面板
    show_quick_open: bool,
    /// 快速打开的过滤词
    quick_open_filter: String,
}

/// 文件树节点
#[derive(Debug)]
pub struct FileTree {
    /// 节点路径
    path: PathBuf,
    /// 是否目录
    is_dir: bool,
    /// 子节点（目录）
    children: Vec<FileTree>,
    /// 是否折叠
    collapsed: bool,
}

impl Workspace {
    pub fn new(workdir: Option<PathBuf>) -> Self {
        let mut ws = Self {
            buffers: HashMap::new(),
            tabs: Vec::new(),
            active_tab: None,
            workdir: workdir.clone(),
            file_tree: FileTree::new(workdir.clone().unwrap_or_default(), true),
            show_quick_open: false,
            quick_open_filter: String::new(),
        };
        if let Some(ref dir) = workdir {
            ws.file_tree = FileTree::from_dir(dir);
        }
        ws
    }

    /// 打开文件
    pub fn open_file(&mut self, path: PathBuf) {
        if self.buffers.contains_key(&path) {
            // 已经打开，切换到该标签
            self.activate_tab(&path);
            return;
        }

        // 加载文件
        let buffer = match Buffer::from_file(path.clone()) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("打开文件失败: {:?}: {}", path, e);
                return;
            }
        };

        self.buffers.insert(path.clone(), buffer);
        self.tabs.push(path.clone());
        self.active_tab = Some(self.tabs.len() - 1);
    }

    /// 新建缓冲区
    pub fn new_buffer(&mut self) {
        let path = PathBuf::from(format!("Untitled-{}", self.tabs.len() + 1));
        let buffer = Buffer::new();
        self.buffers.insert(path.clone(), buffer);
        self.tabs.push(path);
        self.active_tab = Some(self.tabs.len() - 1);
    }

    /// 关闭标签
    pub fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }

        let path = self.tabs.remove(idx);
        self.buffers.remove(&path);

        if let Some(active) = self.active_tab {
            if idx == active {
                self.active_tab = Some(idx.saturating_sub(1).min(self.tabs.len().saturating_sub(1)));
            } else if idx < active {
                self.active_tab = Some(active - 1);
            }
        }
    }

    /// 关闭当前标签
    pub fn close_active_tab(&mut self) {
        if let Some(idx) = self.active_tab {
            self.close_tab(idx);
        }
    }

    /// 激活标签
    pub fn activate_tab(&mut self, path: &PathBuf) {
        if let Some(idx) = self.tabs.iter().position(|p| p == path) {
            self.active_tab = Some(idx);
        }
    }

    /// 激活标签（索引）
    pub fn activate_tab_at(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active_tab = Some(idx);
        }
    }

    /// 下一个标签
    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let active = self.active_tab.unwrap_or(0);
        self.active_tab = Some((active + 1) % self.tabs.len());
    }

    /// 上一个标签
    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let active = self.active_tab.unwrap_or(0);
        self.active_tab = Some((active + self.tabs.len() - 1) % self.tabs.len());
    }

    /// 获取当前激活的缓冲区
    pub fn active_buffer(&self) -> Option<&Buffer> {
        self.active_tab
            .and_then(|i| self.tabs.get(i))
            .and_then(|p| self.buffers.get(p))
    }

    /// 获取当前激活的缓冲区（可变）
    pub fn active_buffer_mut(&mut self) -> Option<&mut Buffer> {
        let active = self.active_tab?;
        let path = self.tabs.get(active)?.clone();
        self.buffers.get_mut(&path)
    }

    /// 插入文本到当前缓冲区
    pub fn insert_text(&mut self, text: &str) {
        if let Some(buf) = self.active_buffer_mut() {
            buf.insert(text);
        }
    }

    /// 保存当前缓冲区
    pub fn save_active(&mut self) {
        if let Some(buf) = self.active_buffer_mut() {
            if let Err(e) = buf.save() {
                tracing::error!("保存失败: {}", e);
            }
        }
    }

    /// 另存当前缓冲区
    pub fn save_as(&mut self, path: PathBuf) {
        if let Some(buf) = self.active_buffer_mut() {
            if let Err(e) = buf.save_to(&path) {
                tracing::error!("另存失败: {}", e);
            }
        }
    }

    /// 显示快速打开
    pub fn quick_open(&mut self) {
        self.show_quick_open = true;
        self.quick_open_filter.clear();
    }

    /// 关闭快速打开
    pub fn close_quick_open(&mut self) {
        self.show_quick_open = false;
        self.quick_open_filter.clear();
    }

    /// 更新快速打开过滤
    pub fn update_quick_open_filter(&mut self, filter: &str) {
        self.quick_open_filter = filter.to_string();
    }

    /// 获取快速打开候选列表
    pub fn quick_open_candidates(&self) -> Vec<PathBuf> {
        let filter = &self.quick_open_filter.to_lowercase();
        self.tabs.iter()
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase().contains(filter))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// 获取标签列表
    pub fn tabs(&self) -> &[PathBuf] {
        &self.tabs
    }

    /// 获取当前激活标签索引
    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_tab
    }

    /// 获取工作目录
    pub fn workdir(&self) -> Option<&PathBuf> {
        self.workdir.as_ref()
    }

    /// 获取文件树
    pub fn file_tree(&self) -> &FileTree {
        &self.file_tree
    }
}

impl FileTree {
    /// 新建节点
    pub fn new(path: PathBuf, is_dir: bool) -> Self {
        Self {
            path,
            is_dir,
            children: Vec::new(),
            collapsed: false,
        }
    }

    /// 从目录加载
    pub fn from_dir(path: &PathBuf) -> Self {
        let mut root = Self::new(path.clone(), true);
        root.load_children();
        root
    }

    /// 加载子节点
    pub fn load_children(&mut self) {
        if !self.is_dir {
            return;
        }

        let entries = match std::fs::read_dir(&self.path) {
            Ok(e) => e,
            Err(_) => return,
        };

        self.children.clear();
        for entry in entries.flatten() {
            let path = entry.path();
            let is_dir = path.is_dir();
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

            // 跳过隐藏文件和常见忽略目录
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "dist" {
                continue;
            }

            let mut node = FileTree::new(path, is_dir);
            if is_dir {
                node.load_children();
            }
            self.children.push(node);
        }

        // 按目录优先、名称排序
        self.children.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.path.file_name().unwrap_or_default().cmp(&b.path.file_name().unwrap_or_default()),
            }
        });
    }

    /// 切换折叠状态
    pub fn toggle_collapse(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// 获取子节点
    pub fn children(&self) -> &[FileTree] {
        &self.children
    }

    /// 获取路径
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// 是否目录
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// 是否折叠
    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }
}
