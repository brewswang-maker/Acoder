//! # 语言检测器
//!
//! 从自然语言描述中自动检测目标技术栈
//! 借鉴 claw-code 的技术栈推断 + zed 的语言服务器协议

use serde::{Deserialize, Serialize};

/// 检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// 主语言
    pub primary: String,
    /// 框架
    pub framework: Option<String>,
    /// 次要语言
    pub secondary: Vec<String>,
    /// 数据库
    pub database: Option<String>,
    /// 项目类型
    pub project_type: ProjectType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    WebFrontend,
    WebBackend,
    FullStack,
    CLI,
    Mobile,
    Library,
    Desktop,
    Microservice,
    DataPipeline,
    ML,
}

/// 语言检测器
pub struct LanguageDetector;

impl LanguageDetector {
    /// 从自然语言描述检测技术栈
    pub fn detect(description: &str) -> DetectionResult {
        let desc_lower = description.to_lowercase();

        // ── 项目类型检测 ─────────────────────────────────────
        let project_type = Self::detect_project_type(&desc_lower);

        // ── 语言检测（按优先级）─────────────────────────────
        let primary = Self::detect_primary_language(&desc_lower);

        // ── 框架检测 ──────────────────────────────────────
        let framework = Self::detect_framework(&desc_lower, &primary);

        // ── 次要语言 ──────────────────────────────────────
        let secondary = Self::detect_secondary(&desc_lower, &primary, &framework);

        // ── 数据库检测 ──────────────────────────────────────
        let database = Self::detect_database(&desc_lower);

        DetectionResult {
            primary,
            framework,
            secondary,
            database,
            project_type,
        }
    }

    fn detect_project_type(desc: &str) -> ProjectType {
        // 前端关键词
        let frontend_kw = ["前端", "页面", "界面", "ui", "frontend", "web app", "网站", "组件",
            "react", "vue", "angular", "svelte", "next.js", "nuxt", "spa"];

        // 后端关键词
        let backend_kw = ["后端", "api", "服务器", "server", "backend", "微服务", "microservice",
            "rest", "graphql", "rpc"];

        // 全栈关键词
        let fullstack_kw = ["全栈", "fullstack", "全站", "前后端", "完整项目", "web应用"];

        // CLI关键词
        let cli_kw = ["cli", "命令行", "terminal", "工具", "tool"];

        // 移动端关键词
        let mobile_kw = ["移动", "mobile", "app", "ios", "android", "flutter", "react native", "swift", "kotlin"];

        // 桌面关键词
        let desktop_kw = ["桌面", "desktop", "electron", "tauri"];

        // 库关键词
        let lib_kw = ["库", "library", "crate", "包", "package", "sdk", "框架", "framework"];

        // 数据管道
        let data_kw = ["数据管道", "etl", "pipeline", "数据处理"];

        // ML关键词
        let ml_kw = ["机器学习", "ml", "ai", "模型", "训练", "深度学习", "deep learning", "neural"];

        // 计算匹配分数
        let score = |keywords: &[&str]| -> usize {
            keywords.iter().filter(|&kw| desc.contains(kw)).count()
        };

        let scores = [
            (ProjectType::FullStack, score(&fullstack_kw) * 3),
            (ProjectType::WebFrontend, score(&frontend_kw)),
            (ProjectType::WebBackend, score(&backend_kw)),
            (ProjectType::CLI, score(&cli_kw)),
            (ProjectType::Mobile, score(&mobile_kw)),
            (ProjectType::Desktop, score(&desktop_kw)),
            (ProjectType::Library, score(&lib_kw)),
            (ProjectType::Microservice, score(&backend_kw) + score(&["微服务", "microservice"]) * 3),
            (ProjectType::DataPipeline, score(&data_kw)),
            (ProjectType::ML, score(&ml_kw)),
        ];

        scores.into_iter()
            .max_by_key(|(_, s)| *s)
            .map(|(t, _)| t)
            .unwrap_or(ProjectType::FullStack)
    }

    fn detect_primary_language(desc: &str) -> String {
        // 全栈项目：前端关键词优先（避免被 rust 截断）
        if desc.contains("全栈") || desc.contains("fullstack") || desc.contains("前后端") {
            if desc.contains("vue") { return "typescript".to_string(); }
            if desc.contains("react") { return "typescript".to_string(); }
            if desc.contains("angular") { return "typescript".to_string(); }
        }

        // 前端关键词优先检测
        if desc.contains("typescript") || desc.contains("javascript") { return "typescript".to_string(); }
        if desc.contains("react") || desc.contains("vue") || desc.contains("angular") { return "typescript".to_string(); }

        // ML/AI 关键词优先（避免被后端的 rust 截断）
        let ml_kw = ["机器学习", "ml", "ai", "模型训练", "深度学习", "神经网络",
            "pytorch", "tensorflow", "sklearn"];
        for kw in &ml_kw {
            if desc.contains(kw) { return "python".to_string(); }
        }

        // 后端语言检测
        let lang_patterns: &[(&str, &str)] = &[
            ("rust", "rust"),
            ("golang", "golang"),
            ("python", "python"),
            ("java", "java"),
            ("kotlin", "kotlin"),
            ("swift", "swift"),
            ("c++", "cpp"),
            ("cpp", "cpp"),
            ("c#", "csharp"),
            ("ruby", "ruby"),
            ("php", "php"),
        ];

        for (keyword, lang) in lang_patterns {
            if desc.contains(keyword) {
                return lang.to_string();
            }
        }

        // 默认根据项目类型推断
        if desc.contains("前端") || desc.contains("react") || desc.contains("vue") {
            "typescript".to_string()
        } else if desc.contains("后端") || desc.contains("api") {
            "rust".to_string()
        } else if desc.contains("脚本") || desc.contains("自动化") {
            "python".to_string()
        } else {
            "rust".to_string()  // ACoder 默认 Rust
        }
    }

    fn detect_framework(desc: &str, language: &str) -> Option<String> {
        let patterns: &[(&str, &str, &str)] = &[
            // Web Frontend
            ("react", "typescript", "react"),
            ("next.js", "typescript", "nextjs"),
            ("nextjs", "typescript", "nextjs"),
            ("vue", "typescript", "vue"),
            ("nuxt", "typescript", "nuxt"),
            ("angular", "typescript", "angular"),
            ("svelte", "typescript", "svelte"),
            // Web Backend
            ("axum", "rust", "axum"),
            ("actix", "rust", "actix-web"),
            ("gin", "golang", "gin"),
            ("express", "javascript", "express"),
            ("fastapi", "python", "fastapi"),
            ("django", "python", "django"),
            ("flask", "python", "flask"),
            ("spring", "java", "spring-boot"),
            // Mobile
            ("flutter", "dart", "flutter"),
            ("react native", "typescript", "react-native"),
            ("swiftui", "swift", "swiftui"),
            // Desktop
            ("electron", "typescript", "electron"),
            ("tauri", "rust", "tauri"),
        ];

        for (fw_keyword, lang, fw) in patterns {
            if desc.contains(fw_keyword) && language == *lang {
                return Some(fw.to_string());
            }
        }

        None
    }

    fn detect_secondary(desc: &str, primary: &str, framework: &Option<String>) -> Vec<String> {
        let mut secondary = Vec::new();

        // 全栈项目自动添加前端/后端
        if desc.contains("全栈") || desc.contains("前后端") {
            if primary == "rust" || primary == "golang" {
                secondary.push("typescript".to_string());
            } else if primary == "typescript" {
                secondary.push("rust".to_string());
            }
        }

        // 添加通用配置语言
        secondary.push("toml".to_string());
        if !primary.contains("javascript") && !primary.contains("typescript") {
            secondary.push("json".to_string());
        }

        // 框架相关
        if let Some(fw) = framework {
            match fw.as_str() {
                "react" | "vue" | "svelte" | "nextjs" | "nuxt" => {
                    secondary.push("css".to_string());
                    secondary.push("html".to_string());
                }
                "fastapi" | "django" | "flask" => {
                    secondary.push("sql".to_string());
                }
                _ => {}
            }
        }

        secondary.dedup();
        secondary
    }

    fn detect_database(desc: &str) -> Option<String> {
        let db_patterns: &[(&str, &str)] = &[
            ("postgresql", "postgresql"),
            ("postgres", "postgresql"),
            ("mysql", "mysql"),
            ("sqlite", "sqlite"),
            ("mongodb", "mongodb"),
            ("mongo", "mongodb"),
            ("redis", "redis"),
            ("dynamodb", "dynamodb"),
        ];

        for (keyword, db) in db_patterns {
            if desc.contains(keyword) {
                return Some(db.to_string());
            }
        }

        // 默认根据项目类型
        if desc.contains("全栈") || desc.contains("api") {
            Some("sqlite".to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_rust_api() {
        let result = LanguageDetector::detect("用 Rust 写一个 REST API 服务");
        assert_eq!(result.primary, "rust");
        assert!(matches!(result.project_type, ProjectType::WebBackend | ProjectType::Microservice));
    }

    #[test]
    fn test_detect_react_frontend() {
        let result = LanguageDetector::detect("创建一个 React 前端页面");
        assert_eq!(result.primary, "typescript");
        assert_eq!(result.framework, Some("react".to_string()));
        assert_eq!(result.project_type, ProjectType::WebFrontend);
    }

    #[test]
    fn test_detect_fullstack() {
        let result = LanguageDetector::detect("全栈项目 Vue 前端 + Rust 后端");
        assert_eq!(result.primary, "typescript");
        assert_eq!(result.framework, Some("vue".to_string()));
        assert_eq!(result.project_type, ProjectType::FullStack);
    }

    #[test]
    fn test_detect_python_ml() {
        let result = LanguageDetector::detect("机器学习模型训练管道");
        assert_eq!(result.primary, "python");
        assert_eq!(result.project_type, ProjectType::ML);
    }

    #[test]
    fn test_detect_default() {
        let result = LanguageDetector::detect("帮我写一个工具");
        assert_eq!(result.primary, "rust");
    }
}
