//! Skill 注册表与市场
//!
//! 功能：
//! - 技能注册与管理
//! - 技能发现（按标签/用途搜索）
//! - 依赖解析
//! - 版本控制
//! - 市场同步（从 ClawdHub/GitHub 下载）

use super::SkillInfo;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Skill 注册表
pub struct SkillRegistry {
    /// 已安装的技能
    skills: HashMap<String, SkillMeta>,
    /// 技能目录路径
    skills_dir: PathBuf,
    /// 技能市场缓存
    marketplace_cache: HashMap<String, MarketplaceSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: PathBuf,
    pub tags: Vec<String>,
    pub dependencies: Vec<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    /// 使用次数
    pub use_count: usize,
    /// 成功率（0-1）
    pub success_rate: f64,
    /// 是否启用
    pub enabled: bool,
}

/// 市场技能信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSkill {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub tags: Vec<String>,
    pub author: String,
    pub downloads: usize,
    pub rating: f64,
    pub source: SkillSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillSource {
    Local,
    ClawdHub,
    GitHub { repo: String },
}

impl SkillRegistry {
    pub fn new() -> Result<Self> {
        let skills_dir = std::env::current_dir()?.join("skills");
        std::fs::create_dir_all(&skills_dir)?;
        Ok(Self {
            skills: HashMap::new(),
            skills_dir,
            marketplace_cache: HashMap::new(),
        })
    }

    /// 从目录加载所有已安装技能
    pub async fn load_installed(&mut self) -> Result<Vec<SkillMeta>> {
        let mut loaded = Vec::new();
        
        for entry in std::fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(meta) = self.load_skill_meta(&path).await? {
                    self.skills.insert(meta.id.clone(), meta.clone());
                    loaded.push(meta);
                }
            }
        }
        
        tracing::info!("已加载 {} 个技能", loaded.len());
        Ok(loaded)
    }

    /// 从目录加载技能元数据
    async fn load_skill_meta(&self, path: &PathBuf) -> Result<Option<SkillMeta>> {
        let meta_path = path.join("skill.json");
        if !meta_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&meta_path).await?;
        let meta: SkillMeta = serde_json::from_str(&content)?;
        
        Ok(Some(meta))
    }

    /// 获取技能
    pub async fn get(&self, id: &str) -> Result<SkillMeta> {
        self.skills.get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Skill 未找到: {}", id))
    }

    /// 列出所有已安装技能
    pub async fn list(&self) -> Result<Vec<SkillInfo>> {
        Ok(self.skills.values().map(|m| SkillInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            success_rate: m.success_rate,
            utility_score: m.use_count as f64,
        }).collect())
    }

    /// 按标签搜索技能
    pub async fn search_by_tag(&self, tag: &str) -> Vec<SkillMeta> {
        self.skills.values()
            .filter(|m| m.tags.iter().any(|t| t.to_lowercase().contains(&tag.to_lowercase())))
            .cloned()
            .collect()
    }

    /// 按关键词搜索
    pub async fn search(&self, query: &str) -> Vec<SkillMeta> {
        let q = query.to_lowercase();
        self.skills.values()
            .filter(|m| {
                m.name.to_lowercase().contains(&q) ||
                m.description.to_lowercase().contains(&q) ||
                m.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .cloned()
            .collect()
    }

    /// 注册新技能
    pub fn register(&mut self, meta: SkillMeta) -> Result<()> {
        // 检查依赖
        for dep in &meta.dependencies {
            if !self.skills.contains_key(dep) {
                return Err(anyhow::anyhow!("缺少依赖技能: {}", dep));
            }
        }
        
        self.skills.insert(meta.id.clone(), meta);
        Ok(())
    }

    /// 安装技能（从市场或本地）
    pub async fn install(&mut self, id: &str) -> Result<()> {
        // 检查是否已安装
        if self.skills.contains_key(id) {
            return Ok(());
        }

        // 从市场缓存查找
        if let Some(market_skill) = self.marketplace_cache.get(id).cloned() {
            self.download_and_install(&market_skill).await?;
            return Ok(());
        }

        Err(anyhow::anyhow!("未找到技能: {}", id))
    }

    /// 下载并安装技能
    async fn download_and_install(&mut self, skill: &MarketplaceSkill) -> Result<()> {
        let skill_dir = self.skills_dir.join(&skill.id);
        tokio::fs::create_dir_all(&skill_dir).await?;

        // 创建元数据文件
        let meta = SkillMeta {
            id: skill.id.clone(),
            name: skill.name.clone(),
            version: skill.version.clone(),
            description: skill.description.clone(),
            path: skill_dir.clone(),
            tags: skill.tags.clone(),
            dependencies: Vec::new(),
            author: Some(skill.author.clone()),
            homepage: None,
            use_count: 0,
            success_rate: skill.rating / 5.0, // 归一化到 0-1
            enabled: true,
        };

        let meta_path = skill_dir.join("skill.json");
        let content = serde_json::to_string_pretty(&meta)?;
        tokio::fs::write(&meta_path, &content).await?;

        // 写入基础 SKILL.md 模板
        let skill_md = format!(
            r#"# {}

{}

## 使用方法

```yaml
# 示例配置
```

## 作者

{}
"#,
            skill.name, skill.description, skill.author
        );
        tokio::fs::write(skill_dir.join("SKILL.md"), &skill_md).await?;

        self.skills.insert(meta.id.clone(), meta);
        tracing::info!("已安装技能: {} ({})", skill.name, skill.id);
        Ok(())
    }

    /// 卸载技能
    pub async fn uninstall(&mut self, id: &str) -> Result<()> {
        if let Some(meta) = self.skills.remove(id) {
            tokio::fs::remove_dir_all(&meta.path).await?;
            tracing::info!("已卸载技能: {}", id);
        }
        Ok(())
    }

    /// 启用/禁用技能
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        if let Some(meta) = self.skills.get_mut(id) {
            meta.enabled = enabled;
        }
        Ok(())
    }

    /// 记录技能使用
    pub fn record_usage(&mut self, id: &str, success: bool) {
        if let Some(meta) = self.skills.get_mut(id) {
            meta.use_count += 1;
            // 更新成功率（滑动平均）
            let alpha = 0.1;
            meta.success_rate = meta.success_rate * (1.0 - alpha) + if success { alpha } else { 0.0 };
        }
    }

    /// 获取推荐技能（按使用频率和成功率排序）
    pub fn get_recommended(&self, limit: usize) -> Vec<&SkillMeta> {
        let mut skills: Vec<_> = self.skills.values()
            .filter(|m| m.enabled)
            .collect();
        
        skills.sort_by(|a, b| {
            let score_a = a.success_rate * (1.0 + a.use_count as f64 / 10.0);
            let score_b = b.success_rate * (1.0 + b.use_count as f64 / 10.0);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        skills.truncate(limit);
        skills
    }

    /// 同步市场（从远程获取最新技能列表）
    pub async fn sync_marketplace(&mut self) -> Result<usize> {
        // 模拟市场数据（实际应从 ClawdHub API 获取）
        let mock_skills = vec![
            MarketplaceSkill {
                id: "openai-whisper".to_string(),
                name: "OpenAI Whisper".to_string(),
                version: "1.0.0".to_string(),
                description: "语音转文字".to_string(),
                tags: vec!["audio".to_string(), "transcription".to_string()],
                author: "ACoder".to_string(),
                downloads: 1200,
                rating: 4.5,
                source: SkillSource::ClawdHub,
            },
            MarketplaceSkill {
                id: "web-scraper".to_string(),
                name: "Web Scraper".to_string(),
                version: "2.1.0".to_string(),
                description: "网页抓取与解析".to_string(),
                tags: vec!["web".to_string(), "scraping".to_string()],
                author: "Community".to_string(),
                downloads: 3500,
                rating: 4.2,
                source: SkillSource::GitHub { repo: "acoder/web-scraper".to_string() },
            },
            MarketplaceSkill {
                id: "code-review".to_string(),
                name: "Code Review".to_string(),
                version: "1.5.0".to_string(),
                description: "自动代码审查".to_string(),
                tags: vec!["code".to_string(), "review".to_string(), "quality".to_string()],
                author: "ACoder".to_string(),
                downloads: 8900,
                rating: 4.8,
                source: SkillSource::ClawdHub,
            },
        ];

        let count = mock_skills.len();
        for skill in mock_skills {
            self.marketplace_cache.insert(skill.id.clone(), skill);
        }

        tracing::info!("市场同步完成，可用技能: {}", count);
        Ok(count)
    }

    /// 列出市场可用技能
    pub fn list_marketplace(&self) -> Vec<&MarketplaceSkill> {
        self.marketplace_cache.values().collect()
    }

    /// 技能目录路径
    pub fn skills_dir(&self) -> &PathBuf {
        &self.skills_dir
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new().unwrap()
    }
}
