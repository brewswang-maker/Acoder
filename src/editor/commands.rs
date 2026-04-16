//! 命令面板
//!
//! Ctrl+Shift+P 呼出，支持模糊搜索

use std::collections::HashMap;

/// 命令定义
#[derive(Debug, Clone)]
pub struct Command {
    /// 命令 ID
    pub id: String,
    /// 显示名称
    pub label: String,
    /// 快捷键（可选）
    pub shortcut: Option<String>,
    /// 分类
    pub category: &'static str,
    /// 是否需要保存状态
    pub dirty_only: bool,
}

/// 命令面板状态
#[derive(Debug)]
pub struct CommandPalette {
    /// 是否打开
    open: bool,
    /// 过滤查询
    query: String,
    /// 候选命令列表
    candidates: Vec<Command>,
    /// 选中索引
    selected: usize,
    /// 注册的命令
    registry: HashMap<String, Command>,
}

impl CommandPalette {
    pub fn new() -> Self {
        let mut palette = Self {
            open: false,
            query: String::new(),
            candidates: Vec::new(),
            selected: 0,
            registry: HashMap::new(),
        };
        palette.register_default_commands();
        palette
    }

    /// 注册默认命令
    fn register_default_commands(&mut self) {
        let commands = vec![
            // 文件
            Command { id: "file.new".into(), label: "新建文件".into(), shortcut: Some("Ctrl+N".into()), category: "文件", dirty_only: false },
            Command { id: "file.open".into(), label: "打开文件".into(), shortcut: Some("Ctrl+O".into()), category: "文件", dirty_only: false },
            Command { id: "file.save".into(), label: "保存".into(), shortcut: Some("Ctrl+S".into()), category: "文件", dirty_only: true },
            Command { id: "file.save_as".into(), label: "另存为".into(), shortcut: Some("Ctrl+Shift+S".into()), category: "文件", dirty_only: false },
            Command { id: "file.close".into(), label: "关闭标签".into(), shortcut: Some("Ctrl+W".into()), category: "文件", dirty_only: false },
            Command { id: "file.quit".into(), label: "退出".into(), shortcut: Some("Ctrl+Q".into()), category: "文件", dirty_only: false },

            // 编辑
            Command { id: "edit.undo".into(), label: "撤销".into(), shortcut: Some("Ctrl+Z".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.redo".into(), label: "重做".into(), shortcut: Some("Ctrl+Shift+Z".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.cut".into(), label: "剪切".into(), shortcut: Some("Ctrl+X".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.copy".into(), label: "复制".into(), shortcut: Some("Ctrl+C".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.paste".into(), label: "粘贴".into(), shortcut: Some("Ctrl+V".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.select_all".into(), label: "全选".into(), shortcut: Some("Ctrl+A".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.find".into(), label: "查找".into(), shortcut: Some("Ctrl+F".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.replace".into(), label: "替换".into(), shortcut: Some("Ctrl+H".into()), category: "编辑", dirty_only: false },
            Command { id: "edit.find_in_files".into(), label: "在文件中查找".into(), shortcut: Some("Ctrl+Shift+F".into()), category: "编辑", dirty_only: false },

            // 视图
            Command { id: "view.toggle_sidebar".into(), label: "切换侧边栏".into(), shortcut: Some("Ctrl+B".into()), category: "视图", dirty_only: false },
            Command { id: "view.toggle_terminal".into(), label: "切换终端".into(), shortcut: Some("Ctrl+`".into()), category: "视图", dirty_only: false },
            Command { id: "view.zoom_in".into(), label: "放大".into(), shortcut: Some("Ctrl+=".into()), category: "视图", dirty_only: false },
            Command { id: "view.zoom_out".into(), label: "缩小".into(), shortcut: Some("Ctrl+-".into()), category: "视图", dirty_only: false },
            Command { id: "view.reset_zoom".into(), label: "重置缩放".into(), shortcut: Some("Ctrl+0".into()), category: "视图", dirty_only: false },
            Command { id: "view.toggle_fullscreen".into(), label: "切换全屏".into(), shortcut: Some("F11".into()), category: "视图", dirty_only: false },

            // AI
            Command { id: "ai.complete".into(), label: "AI 补全".into(), shortcut: Some("Tab".into()), category: "AI", dirty_only: false },
            Command { id: "ai.chat".into(), label: "AI 对话".into(), shortcut: Some("Ctrl+Shift+L".into()), category: "AI", dirty_only: false },
            Command { id: "ai.explain".into(), label: "解释代码".into(), shortcut: Some("Ctrl+Shift+E".into()), category: "AI", dirty_only: false },
            Command { id: "ai.refactor".into(), label: "AI 重构".into(), shortcut: Some("Ctrl+Shift+R".into()), category: "AI", dirty_only: false },
            Command { id: "ai.fix".into(), label: "修复错误".into(), shortcut: Some("Ctrl+Shift+F".into()), category: "AI", dirty_only: false },
            Command { id: "ai.gen_test".into(), label: "生成测试".into(), shortcut: Some("Ctrl+Shift+T".into()), category: "AI", dirty_only: false },
            Command { id: "ai.gen_docs".into(), label: "生成文档".into(), shortcut: None, category: "AI", dirty_only: false },

            // 标签
            Command { id: "tab.next".into(), label: "下一个标签".into(), shortcut: Some("Ctrl+Tab".into()), category: "标签", dirty_only: false },
            Command { id: "tab.prev".into(), label: "上一个标签".into(), shortcut: Some("Ctrl+Shift+Tab".into()), category: "标签", dirty_only: false },
            Command { id: "tab.close".into(), label: "关闭标签".into(), shortcut: Some("Ctrl+W".into()), category: "标签", dirty_only: false },
            Command { id: "tab.close_others".into(), label: "关闭其他标签".into(), shortcut: None, category: "标签", dirty_only: false },
        ];

        for cmd in commands {
            self.registry.insert(cmd.id.clone(), cmd);
        }
    }

    /// 打开面板
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.update_candidates();
    }

    /// 关闭面板
    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
    }

    /// 切换面板
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    /// 是否打开
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 更新过滤查询
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.selected = 0;
        self.update_candidates();
    }

    /// 追加查询字符
    pub fn append_query(&mut self, ch: char) {
        self.query.push(ch);
        self.selected = 0;
        self.update_candidates();
    }

    /// 回退查询
    pub fn backspace_query(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.update_candidates();
    }

    /// 上移选择
    pub fn select_up(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }

    /// 下移选择
    pub fn select_down(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.candidates.len() - 1);
    }

    /// 获取当前选中命令
    pub fn selected_command(&self) -> Option<&Command> {
        self.candidates.get(self.selected)
    }

    /// 执行选中命令
    pub fn execute_selected(&mut self) -> Option<String> {
        self.selected_command().map(|c| c.id.clone())
    }

    /// 获取候选命令
    pub fn candidates(&self) -> &[Command] {
        &self.candidates
    }

    /// 获取选中索引
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// 注册命令
    pub fn register(&mut self, command: Command) {
        self.registry.insert(command.id.clone(), command);
    }

    /// 更新候选列表
    fn update_candidates(&mut self) {
        let query_lower = self.query.to_lowercase();
        let mut matches: Vec<&Command> = self.registry.values()
            .filter(|cmd| {
                if query_lower.is_empty() {
                    return true;
                }
                cmd.label.to_lowercase().contains(&query_lower)
                    || cmd.id.contains(&query_lower)
            })
            .collect();

        // 模糊排序
        matches.sort_by(|a, b| {
            let a_score = fuzzy_score(&a.label, &query_lower);
            let b_score = fuzzy_score(&b.label, &query_lower);
            b_score.cmp(&a_score)
        });

        self.candidates = matches.into_iter().take(20).cloned().collect();
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// 简单模糊匹配评分
fn fuzzy_score(text: &str, query: &str) -> usize {
    let text_lower = text.to_lowercase();
    let query_chars: Vec<char> = query.chars().collect();
    let text_chars: Vec<char> = text_lower.chars().collect();

    let mut score = 0;
    let mut qi = 0;
    let mut prev_match = false;

    for tc in &text_chars {
        if qi < query_chars.len() && *tc == query_chars[qi] {
            score += if prev_match { 2 } else { 1 }; // 连续匹配得 2 分
            qi += 1;
            prev_match = true;
        } else {
            prev_match = false;
        }
    }

    // 全部匹配加 10 分
    if qi == query_chars.len() {
        score += 10;
    }

    score
}
