//! Webhook 集成 — 外部事件触发 ACoder 任务
//! 支持：GitHub, GitLab, Jira, Slack, 定时任务

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Webhook 事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum WebhookEvent {
    GitHub(GitHubEvent),
    GitLab(GitLabEvent),
    Jira(JiraEvent),
    Slack(SlackEvent),
    Timer(TimerEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubEvent {
    pub action: String,
    pub repository: GitHubRepo,
    #[serde(rename = "pull_request")]
    pub pull_request: Option<GitHubPR>,
    pub push: Option<GitHubPush>,
    pub sender: GitHubUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRepo {
    pub full_name: String,
    pub clone_url: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubPR {
    pub number: usize,
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubPush {
    #[serde(rename = "ref")]
    pub ref_: String,
    pub commits: Vec<GitHubCommit>,
    pub compare: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommit {
    pub id: String,
    pub message: String,
    pub author: GitHubUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    pub avatar_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabEvent {
    pub object_kind: String,
    pub project: GitLabProject,
    pub merge_request: Option<GitLabMR>,
    pub push: Option<GitLabPush>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabProject {
    pub path_with_namespace: String,
    pub git_http_url: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabMR {
    pub iid: usize,
    pub title: String,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabPush {
    #[serde(rename = "ref")]
    pub ref_: String,
    pub commits: Vec<GitLabCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabCommit {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraEvent {
    pub webhook_event: String,
    pub issue: JiraIssue,
    pub user: JiraUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraIssue {
    pub key: String,
    pub fields: JiraFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraFields {
    pub summary: String,
    pub description: String,
    pub status: JiraStatus,
    pub priority: JiraPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraStatus {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraPriority {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraUser {
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackEvent {
    pub event_type: String,
    pub channel: String,
    pub user: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerEvent {
    pub schedule_id: String,
    pub cron_expression: String,
}

/// Webhook 配置
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    pub github_secret: Option<String>,
    pub gitlab_secret: Option<String>,
    pub server_addr: String,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            github_secret: None,
            gitlab_secret: None,
            server_addr: "0.0.0.0:8080".to_string(),
        }
    }
}
