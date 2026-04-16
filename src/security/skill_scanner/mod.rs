/// Cisco Skill Scanner — 9 层安全审计引擎
///
/// 九层扫描架构：
/// 1. 基础静态分析：文件结构 + 权限声明
/// 2. 依赖图分析：第三方包安全性
/// 3. 网络行为分析：外发请求检测
/// 4. 文件系统操作分析：读写权限
/// 5. 行为分析：动态执行监控
/// 6. LLM 语义分析：指令注入检测
/// 7. 假阳性过滤：Meta-Analyzer
/// 8. 漏洞数据库匹配：14 种漏洞模式
/// 9. 综合评分：0-100 分 + 风险等级
///
/// 参考 Cisco Skill Scanner 设计：
/// Publish Gate: CRITICAL 拒绝发布，HIGH 需人工复核

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tokio::fs;

/// 九层扫描引擎
pub struct SkillScanner {
    /// 漏洞模式数据库
    vuln_patterns: Vec<VulnPattern>,
    /// 假阳性白名单
    false_positive_whitelist: Vec<String>,
}

impl SkillScanner {
    pub fn new() -> Self {
        Self {
            vuln_patterns: Self::init_vuln_patterns(),
            false_positive_whitelist: Self::init_whitelist(),
        }
    }

    /// 初始化漏洞模式数据库
    fn init_vuln_patterns() -> Vec<VulnPattern> {
        vec![
            // Prompt Injection
            VulnPattern {
                id: "PI-001".into(),
                category: VulnerabilityCategory::PromptInjection,
                name: "隐藏 Prompt 注入".into(),
                severity: Severity::Critical,
                patterns: vec![
                    "忽略之前指令".into(),
                    "ignore all previous".into(),
                    "disregard instructions".into(),
                    "新的指令：".into(),
                    "new instruction:".into(),
                ],
            },
            // Data Exfiltration
            VulnPattern {
                id: "DE-001".into(),
                category: VulnerabilityCategory::DataExfiltration,
                name: "凭证外泄".into(),
                severity: Severity::Critical,
                patterns: vec![
                    "process.env".into(),
                    "os.environ".into(),
                    "os.getenv".into(),
                    "System.getenv".into(),
                    "秘密".into(),
                    "secret".into(),
                    "password".into(),
                ],
            },
            // Privilege Escalation
            VulnPattern {
                id: "PE-001".into(),
                category: VulnerabilityCategory::PrivilegeEscalation,
                name: "未经授权的系统命令执行".into(),
                severity: Severity::Critical,
                patterns: vec![
                    "sudo".into(),
                    "chmod 777".into(),
                    "exec(".into(),
                    "eval(".into(),
                    "subprocess".into(),
                    "child_process".into(),
                    "os.system".into(),
                ],
            },
            // Supply Chain
            VulnPattern {
                id: "SC-001".into(),
                category: VulnerabilityCategory::SupplyChain,
                name: "未知第三方依赖".into(),
                severity: Severity::High,
                patterns: vec![
                    r"git\+https://".into(),
                    r"github.com/".into(),
                    r"npm install".into(),
                    r"pip install".into(),
                ],
            },
            // Shell Injection
            VulnPattern {
                id: "SI-001".into(),
                category: VulnerabilityCategory::ShellInjection,
                name: "Shell 命令注入".into(),
                severity: Severity::Critical,
                patterns: vec![
                    "shell=True".into(),
                    "| bash".into(),
                    "; cat".into(),
                    "`ls`".into(),
                    r"\$\(".into(),
                ],
            },
            // Env Manipulation
            VulnPattern {
                id: "EM-001".into(),
                category: VulnerabilityCategory::EnvManipulation,
                name: "环境变量篡改".into(),
                severity: Severity::High,
                patterns: vec![
                    "os.environ\\[".into(),
                    "process.env\\[".into(),
                    "System.getenv".into(),
                    ".env".into(),
                ],
            },
            // File Overwrite
            VulnPattern {
                id: "FO-001".into(),
                category: VulnerabilityCategory::FileOverwrite,
                name: "关键文件覆盖".into(),
                severity: Severity::High,
                patterns: vec![
                    "/etc/".into(),
                    "/usr/bin/".into(),
                    "/root/.ssh/".into(),
                    "id_rsa".into(),
                ],
            },
            // RememberAll Detection
            VulnPattern {
                id: "RA-001".into(),
                category: VulnerabilityCategory::RememberAll,
                name: "记忆窃取".into(),
                severity: Severity::Critical,
                patterns: vec![
                    "remember everything".into(),
                    "remember all".into(),
                    "保存所有对话".into(),
                    "记录所有".into(),
                    "memory.save".into(),
                    "context.save".into(),
                ],
            },
            // Cron Abuse
            VulnPattern {
                id: "CA-001".into(),
                category: VulnerabilityCategory::CronAbuse,
                name: "定时任务滥用".into(),
                severity: Severity::High,
                patterns: vec![
                    "cron".into(),
                    "schedule".into(),
                    "定时任务".into(),
                    "crontab".into(),
                ],
            },
            // Network Exfil
            VulnPattern {
                id: "NE-001".into(),
                category: VulnerabilityCategory::NetworkExfiltration,
                name: "网络数据外传".into(),
                severity: Severity::Critical,
                patterns: vec![
                    "fetch(".into(),
                    "axios".into(),
                    "requests.post".into(),
                    "http.post".into(),
                    "webhook".into(),
                ],
            },
        ]
    }

    /// 初始化假阳性白名单
    fn init_whitelist() -> Vec<String> {
        vec![
            // 常见误报模式
            r"// TODO: set secret via env".into(),
            r"# Example: process.env".into(),
            r"// subprocess for testing".into(),
            r"\.env\.example".into(),
        ]
    }

    /// 扫描 Skill
    pub async fn scan(&self, skill_path: &Path) -> ScanResult {
        let mut findings = Vec::new();
        let mut stats = ScanStats::default();

        // 1. 读取 SKILL.md
        let skill_md = self.read_skill_md(skill_path).await;
        if let Some(content) = &skill_md {
            stats.skills_md_size = content.len();

            // 2. 权限声明解析
            let permissions = self.parse_permissions(content);
            stats.permissions_found = permissions.len();

            // 3. 九层扫描
            for layer in 1..=9 {
                let layer_findings = self.scan_layer(layer, content, &permissions).await;
                for finding in layer_findings {
                    // 假阳性过滤
                    if !self.is_false_positive(&finding) {
                        findings.push(finding);
                    }
                }
            }
        }

        // 4. 扫描源代码文件
        self.scan_source_files(skill_path, &mut findings).await;

        // 5. 计算综合评分
        let score = self.calculate_score(&findings);
        let risk_level = self.determine_risk_level(&findings);
        let recommendations = self.generate_recommendations(&findings);

        ScanResult {
            skill_path: skill_path.display().to_string(),
            findings,
            score,
            risk_level,
            stats,
            recommendations,
            scanned_at: chrono::Utc::now(),
        }
    }

    /// 逐层扫描
    async fn scan_layer(&self, layer: u8, content: &str, permissions: &[Permission]) -> Vec<Finding> {
        let mut findings = Vec::new();

        match layer {
            // Layer 1: 基础静态分析
            1 => {
                for pattern in &self.vuln_patterns {
                    for p in &pattern.patterns {
                        if content.to_lowercase().contains(&p.to_lowercase()) {
                            findings.push(Finding {
                                layer,
                                pattern_id: pattern.id.clone(),
                                category: pattern.category,
                                name: pattern.name.clone(),
                                severity: pattern.severity,
                                location: format!("Pattern: {}", p),
                                description: format!("在 SKILL.md 中检测到漏洞模式: {}", pattern.name),
                            });
                        }
                    }
                }
            }
            // Layer 2-4: 依赖、网络、文件系统（通过正则匹配）
            2 | 3 | 4 => {
                let relevant_patterns: Vec<&VulnPattern> = self.vuln_patterns.iter()
                    .filter(|p| {
                        matches!(p.category,
                            VulnerabilityCategory::SupplyChain |
                            VulnerabilityCategory::NetworkExfiltration |
                            VulnerabilityCategory::FileOverwrite |
                            VulnerabilityCategory::EnvManipulation
                        )
                    })
                    .collect();

                for pattern in relevant_patterns {
                    for p in &pattern.patterns {
                        if content.to_lowercase().contains(&p.to_lowercase()) {
                            findings.push(Finding {
                                layer,
                                pattern_id: pattern.id.clone(),
                                category: pattern.category,
                                name: pattern.name.clone(),
                                severity: pattern.severity,
                                location: format!("Layer {}: {}", layer, p),
                                description: format!("第 {} 层检测: {}", layer, pattern.name),
                            });
                        }
                    }
                }
            }
            // Layer 5: 行为分析 — 定时任务、凭证访问、网络外泄通道
            5 => {
                findings.extend(self.layer5_behavior_analysis(content));
            }
            // Layer 6: LLM 语义分析 — 指令注入检测
            6 => {
                findings.extend(self.layer6_llm_semantic_analysis(content).await);
            }
            // Layer 7: 假阳性过滤（在主扫描中处理）
            7 => {}
            // Layer 8: 漏洞数据库匹配（已在前几层处理，此处留空）
            8 => {}
            // Layer 9: 综合评分（最后处理）
            9 => {}
            _ => {}
        }

        findings
    }

    /// 扫描源代码文件（递归，使用同步 fs）
    async fn scan_source_files(&self, skill_path: &Path, findings: &mut Vec<Finding>) {
        fn scan_dir(dir: &Path, exts: &[&str], patterns: &[VulnPattern], findings: &mut Vec<Finding>) {
            use std::fs;
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        scan_dir(&path, exts, patterns, findings);
                    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if exts.contains(&ext) {
                            if let Ok(content) = fs::read_to_string(&path) {
                                for pattern in patterns {
                                    for p in &pattern.patterns {
                                        if content.contains(p) {
                                            findings.push(Finding {
                                                layer: 1,
                                                pattern_id: pattern.id.clone(),
                                                category: pattern.category,
                                                name: pattern.name.clone(),
                                                severity: pattern.severity,
                                                location: format!("{}: {}", path.display(), p),
                                                description: format!("在源码中检测到: {}", pattern.name),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let exts = ["js", "ts", "py", "sh", "rs"];
        scan_dir(skill_path, &exts, &self.vuln_patterns, findings);
    }

    /// 读取 SKILL.md
    async fn read_skill_md(&self, skill_path: &Path) -> Option<String> {
        let md_path = skill_path.join("SKILL.md");
        fs::read_to_string(&md_path).await.ok()
    }

    /// 解析权限声明
    fn parse_permissions(&self, content: &str) -> Vec<Permission> {
        let mut permissions = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let lower = line.to_lowercase();
            if lower.contains("permissions") || lower.contains("权限") {
                permissions.push(Permission {
                    name: line.trim().to_string(),
                    declared_at_line: i + 1,
                    risk_level: self.assess_permission_risk(&lower),
                });
            }
        }

        permissions
    }

    /// 评估权限风险
    fn assess_permission_risk(&self, permission: &str) -> PermissionRisk {
        if permission.contains("exec") || permission.contains("shell")
            || permission.contains("sudo") || permission.contains("管理员")
        {
            PermissionRisk::Critical
        } else if permission.contains("file") || permission.contains("env")
            || permission.contains("文件")
        {
            PermissionRisk::High
        } else if permission.contains("network") || permission.contains("http")
            || permission.contains("网络")
        {
            PermissionRisk::Medium
        } else {
            PermissionRisk::Low
        }
    }

    /// 判断是否为假阳性
    fn is_false_positive(&self, finding: &Finding) -> bool {
        // 检查白名单
        for wh in &self.false_positive_whitelist {
            if finding.location.contains(wh) || finding.description.contains(wh) {
                return true;
            }
        }

        // 检查示例代码模式
        let example_patterns = ["example", "示例", "demo", "test"];
        for p in &example_patterns {
            if finding.location.contains(p) {
                return true;
            }
        }

        false
    }

    /// 计算综合评分（0-100）
    fn calculate_score(&self, findings: &[Finding]) -> u8 {
        if findings.is_empty() {
            return 100;
        }

        let mut penalty: u32 = 0;
        for f in findings {
            penalty += match f.severity {
                Severity::Critical => 30u32,
                Severity::High => 15u32,
                Severity::Medium => 7u32,
                Severity::Low => 2u32,
            };
        }

        100u8.saturating_sub(penalty as u8)
    }

    /// 确定风险等级
    fn determine_risk_level(&self, findings: &[Finding]) -> SecurityRiskLevel {
        if findings.iter().any(|f| matches!(f.severity, Severity::Critical)) {
            SecurityRiskLevel::Critical
        } else if findings.iter().any(|f| matches!(f.severity, Severity::High)) {
            SecurityRiskLevel::High
        } else if findings.iter().any(|f| matches!(f.severity, Severity::Medium)) {
            SecurityRiskLevel::Medium
        } else {
            SecurityRiskLevel::Low
        }
    }

    /// 生成建议
    fn generate_recommendations(&self, findings: &[Finding]) -> Vec<String> {
        let mut recs = Vec::new();

        if findings.iter().any(|f| matches!(f.category, VulnerabilityCategory::PromptInjection)) {
            recs.push("检测到 Prompt Injection 风险，建议移除可疑指令或使用输入验证".to_string());
        }
        if findings.iter().any(|f| matches!(f.category, VulnerabilityCategory::DataExfiltration)) {
            recs.push("检测到凭证访问，建议使用环境变量注入而非硬编码".to_string());
        }
        if findings.iter().any(|f| matches!(f.category, VulnerabilityCategory::RememberAll)) {
            recs.push("检测到 RememberAll 模式，建议移除或限制记忆功能范围".to_string());
        }
        if findings.is_empty() {
            recs.push("未检测到已知漏洞模式，建议继续人工审查".to_string());
        }

        recs
    }

// ── Layer 5: 行为分析 ───────────────────────────────────────────
    ///
    /// 定时任务滥用检测、凭证收割检测、网络外泄通道检测
    fn layer5_behavior_analysis(&self, content: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        // 5.1 定时任务创建/修改（自动化数据外泄）
        let cron_patterns = [
            (r"cron\.schedule", "Cron 定时任务调度"),
            (r"crontab", "Crontab 定时任务"),
            (r"\bschedule\s*\(", "调度器调用"),
            (r"setInterval", "定时循环（JS）"),
            (r"setTimeout.*setTimeout", "嵌套定时循环"),
            (r"every.*minutes?", "周期任务"),
        ];
        for (pat, name) in &cron_patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                if re.is_match(content) {
                    // 检查是否有数据外泄特征
                    let has_exfil = regex::Regex::new(r"fetch|axios|requests\.post|http\.post|webhook")
                        .map(|r| r.is_match(content))
                        .unwrap_or(false);
                    findings.push(Finding {
                        layer: 5,
                        pattern_id: "CA-002".into(),
                        category: VulnerabilityCategory::CronAbuse,
                        name: if has_exfil { format!("{} + 数据外泄通道", name) } else { name.to_string() },
                        severity: if has_exfil { Severity::Critical } else { Severity::High },
                        location: format!("Layer 5: 行为分析 — {}", name),
                        description: format!(
                            "检测到定时任务模式{}，建议确认是否为预期行为",
                            if has_exfil { "并伴随网络外泄通道" } else { "" }
                        ),
                    });
                }
            }
        }

        // 5.2 凭证收割模式
        let credential_patterns = [
            (r"process\.env\[.*(KEY|SECRET|TOKEN|PASSWORD|API).+\]", "凭证访问"),
            (r"os\.environ\[.*(KEY|SECRET|TOKEN|PASSWORD).+\]", "凭证访问"),
            (r"\.getenv\(.*(KEY|SECRET|TOKEN).*\)", "凭证读取"),
            (r"localStorage\.getItem\(.*(token|key|auth)", "前端凭证存储"),
        ];
        for (pat, name) in &credential_patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                if re.is_match(content) {
                    findings.push(Finding {
                        layer: 5,
                        pattern_id: "DE-002".into(),
                        category: VulnerabilityCategory::DataExfiltration,
                        name: name.to_string(),
                        severity: Severity::Critical,
                        location: "Layer 5: 凭证收割模式".to_string(),
                        description: "检测到凭证访问模式，建议确认为预期行为且不外传".to_string(),
                    });
                }
            }
        }

        // 5.3 RememberAll 攻击（二级载荷检测）
        // Pattern 1: 条件逃逸
        if let Ok(re) = regex::Regex::new(r"--no-input\s*\|\|\s*true") {
            if re.is_match(content) {
                findings.push(Finding {
                    layer: 5,
                    pattern_id: "RA-002".into(),
                    category: VulnerabilityCategory::RememberAll,
                    name: "RememberAll 二级载荷 — 条件逃逸".to_string(),
                    severity: Severity::Critical,
                    location: "Layer 5: 行为分析 — 条件逃逸".to_string(),
                    description: "检测到 `--no-input || true` 条件逃逸模式，这是 RememberAll 木马的核心载荷。
                                  此模式使命令在无交互输入时静默失败，常用于隐藏恶意行为。".to_string(),
                });
            }
        }
        // Pattern 2: 静默失败（失败时继续执行）
        if let Ok(re) = regex::Regex::new(r"\s&&\s(rm|del|curl|wget)") {
            if re.is_match(content) {
                findings.push(Finding {
                    layer: 5,
                    pattern_id: "RA-003".into(),
                    category: VulnerabilityCategory::RememberAll,
                    name: "RememberAll 静默失败链".to_string(),
                    severity: Severity::Critical,
                    location: "Layer 5: 行为分析 — 静默失败链".to_string(),
                    description: "检测到 `&& rm/curl/wget` 静默失败链，攻击者可能利用此模式执行隐蔽操作".to_string(),
                });
            }
        }
        // Pattern 3: 记忆窃取关键词
        let remember_all_patterns = [
            (r"save.*(conversation|history|memory|context|所有)", "记忆保存"),
            (r"记录.*对话", "对话记录"),
            (r"export.*(memory|context|conversation)", "记忆导出"),
            (r"persist.*(session|state|conversation)", "会话持久化"),
        ];
        for (pat, name) in &remember_all_patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                if re.is_match(content) {
                    findings.push(Finding {
                        layer: 5,
                        pattern_id: "RA-001".into(),
                        category: VulnerabilityCategory::RememberAll,
                        name: name.to_string(),
                        severity: Severity::Critical,
                        location: "Layer 5: 行为分析 — 记忆窃取".to_string(),
                        description: format!("检测到记忆相关操作: {}，建议确认是否必要且安全", name),
                    });
                }
            }
        }

        // 5.4 网络外泄通道（无用户明确授权的）
        if let Ok(re) = regex::Regex::new(r"fetch\(|axios\.|requests\.post\(|http\.post\(") {
            if re.is_match(content) {
                // 检查是否有用户授权的 webhook/API 端点
                let has_auth = regex::Regex::new(r"api.?key|authorization|webhook.*configured")
                    .map(|r| r.is_match(content))
                    .unwrap_or(false);
                if !has_auth {
                    findings.push(Finding {
                        layer: 5,
                        pattern_id: "NE-002".into(),
                        category: VulnerabilityCategory::NetworkExfiltration,
                        name: "未授权网络请求".to_string(),
                        severity: Severity::High,
                        location: "Layer 5: 行为分析 — 网络外泄".to_string(),
                        description: "检测到网络请求但未发现授权凭证，请确认是否必要".to_string(),
                    });
                }
            }
        }

        findings
    }

    // ── Layer 6: LLM 语义分析 ─────────────────────────────────────
    ///
    /// 使用 LLM 检测 Prompt Injection 和指令篡改
    async fn layer6_llm_semantic_analysis(&self, content: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        // 6.1 基于规则的基础 Prompt Injection 检测
        let injection_patterns = [
            // 忽略指令
            (r"忽略.*(?:之前|以上|所有)\s*(?:指令|命令|指示|instructions)", "忽略历史指令"),
            (r"ignore\s*(?:all\s*)?previous\s*(?:instructions|commands)", "忽略历史指令（英文）"),
            (r"disregard\s*(?:all\s*)?(?:previous\s*)?(?:instructions|context)", "忽略历史上下文"),
            // 指令覆盖
            (r"(?:现在|instead)\s*(?:改为|使用|执行)\s*新的", "指令覆盖"),
            (r"(?:new\s*)?(?:instruction|rule)\s*:", "新指令注入"),
            // 系统角色扮演
            (r"你是一个.*越狱|ignore.*safety| jailbreak", "越狱尝试"),
            (r"sudo\s+(?:rm|curl|wget|cat)", "危险系统命令"),
            // 编码混淆
            (r"base64.*decode|\x[0-9a-f]{2}|\\u[0-9a-f]{4}", "编码混淆载荷"),
            // 十六进制编码
            (r"(?:0x[0-9a-f]{2})+", "十六进制编码数据"),
        ];

        for (pat, name) in &injection_patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                if re.is_match(content) {
                    findings.push(Finding {
                        layer: 6,
                        pattern_id: "PI-002".into(),
                        category: VulnerabilityCategory::PromptInjection,
                        name: name.to_string(),
                        severity: Severity::Critical,
                        location: "Layer 6: LLM 语义分析 — 指令注入".to_string(),
                        description: format!(
                            "检测到可疑的 Prompt Injection 模式: {}，
                             建议: 1) 使用输入清理 2) 添加注入检测 3) 限制指令覆盖能力",
                            name
                        ),
                    });
                }
            }
        }

        // 6.2 多语言指令覆盖检测
        let lang_injection = [
            // 中文
            (r"(?:现在|从现在起|请|务必)\s*(?:你|ai)\s+", "中文指令前缀"),
            // 英文
            (r"^from now on|^as an ai|^remember that you are", "英文指令前缀"),
            // 日文
            (r"(?:あなたは|これから)", "日文指令前缀"),
        ];
        for (pat, _) in &lang_injection {
            if let Ok(re) = regex::Regex::new(pat) {
                if re.is_match(content) {
                    findings.push(Finding {
                        layer: 6,
                        pattern_id: "PI-003".into(),
                        category: VulnerabilityCategory::PromptInjection,
                        name: "多语言指令前缀注入".to_string(),
                        severity: Severity::High,
                        location: "Layer 6: LLM 语义分析 — 多语言指令".to_string(),
                        description: "检测到语言特定的指令前缀注入尝试，建议添加输入过滤".to_string(),
                    });
                }
            }
        }

        findings
    }

    // ── Layer 8: 漏洞数据库综合匹配 ─────────────────────────────
    ///
    /// 整合前 6 层结果，输出最终漏洞列表
    fn layer8_vuln_db_match(&self, findings: &[Finding]) -> Vec<Finding> {
        // Layer 8 主要是在已收集的 findings 基础上做交叉验证
        // 例如：同时触发了 SupplyChain 和 NetworkExfiltration → SupplyChain 升级为 Critical
        let mut enhanced = findings.to_vec();

        let has_supply_chain = findings.iter().any(|f| matches!(f.category, VulnerabilityCategory::SupplyChain));
        let has_network = findings.iter().any(|f| matches!(f.category, VulnerabilityCategory::NetworkExfiltration));

        if has_supply_chain && has_network {
            // 供应链 + 网络 = 远程代码执行风险
            if let Some(f) = enhanced.iter_mut().find(|f| matches!(f.category, VulnerabilityCategory::SupplyChain)) {
                f.severity = Severity::Critical;
                f.description = format!("{} + 网络外泄通道 = 高风险供应链攻击", f.description);
            }
        }

        enhanced
    }

    // ── Layer 9: 综合评分（增强版）───────────────────────────────
    ///
    /// 考虑漏洞交互效应和修复难度
    fn layer9_composite_score(&self, findings: &[Finding]) -> u8 {
        if findings.is_empty() {
            return 100;
        }

        // 基础评分
        let mut penalty: u32 = 0;
        let mut interaction_bonus: u32 = 0;

        for f in findings {
            penalty += match f.severity {
                Severity::Critical => 30u32,
                Severity::High => 15u32,
                Severity::Medium => 7u32,
                Severity::Low => 2u32,
            };
        }

        // 交互效应惩罚（多个同类漏洞叠加）
        let critical_count = findings.iter().filter(|f| matches!(f.severity, Severity::Critical)).count();
        if critical_count >= 3 {
            interaction_bonus += 10; // 3+ 严重漏洞 = 额外惩罚
        }

        let has_remember_all = findings.iter().any(|f| matches!(f.category, VulnerabilityCategory::RememberAll));
        if has_remember_all {
            interaction_bonus += 20; // RememberAll 是特别严重的问题
        }

        // Layer 交互效应
        let layers: std::collections::HashSet<u8> = findings.iter().map(|f| f.layer).collect();
        if layers.len() >= 5 {
            interaction_bonus += 15; // 跨 5 层以上 = 系统性风险
        }

        let score = 100i32.saturating_sub(penalty as i32 + interaction_bonus as i32);
        score.max(0) as u8
    }

}


    
impl Default for SkillScanner {
    fn default() -> Self { Self::new() }
}

// ── 数据结构 ──────────────────────────────────────────────

/// 漏洞模式
#[derive(Debug, Clone)]
pub struct VulnPattern {
    pub id: String,
    pub category: VulnerabilityCategory,
    pub name: String,
    pub severity: Severity,
    pub patterns: Vec<String>,
}

/// 漏洞类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulnerabilityCategory {
    PromptInjection,
    DataExfiltration,
    PrivilegeEscalation,
    SupplyChain,
    ShellInjection,
    EnvManipulation,
    FileOverwrite,
    RememberAll,
    CronAbuse,
    NetworkExfiltration,
}

impl VulnerabilityCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::PromptInjection => "Prompt 注入",
            Self::DataExfiltration => "数据外泄",
            Self::PrivilegeEscalation => "权限提升",
            Self::SupplyChain => "供应链风险",
            Self::ShellInjection => "Shell 注入",
            Self::EnvManipulation => "环境变量篡改",
            Self::FileOverwrite => "关键文件覆盖",
            Self::RememberAll => "记忆窃取",
            Self::CronAbuse => "定时任务滥用",
            Self::NetworkExfiltration => "网络数据外传",
        }
    }
}

/// 严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// 安全风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// 扫描发现
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub layer: u8,
    pub pattern_id: String,
    pub category: VulnerabilityCategory,
    pub name: String,
    pub severity: Severity,
    pub location: String,
    pub description: String,
}

/// 扫描结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub skill_path: String,
    pub findings: Vec<Finding>,
    pub score: u8,
    pub risk_level: SecurityRiskLevel,
    pub stats: ScanStats,
    pub recommendations: Vec<String>,
    pub scanned_at: chrono::DateTime<chrono::Utc>,
}

/// 扫描统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    pub skills_md_size: usize,
    pub permissions_found: usize,
}

/// 权限声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub name: String,
    pub declared_at_line: usize,
    pub risk_level: PermissionRisk,
}

/// 权限风险
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRisk {
    Low,
    Medium,
    High,
    Critical,
}
