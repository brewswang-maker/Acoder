//! Webhook 路由器 — 事件匹配与规则触发

use super::WebhookEvent;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Webhook 路由器
pub struct WebhookRouter {
    rules: Vec<WebhookRule>,
}

#[derive(Debug, Clone)]
pub struct WebhookRule {
    pub id: String,
    pub name: String,
    pub trigger: Trigger,
    pub filter: Filter,
    pub action: Action,
    pub callback: Option<Callback>,
}

#[derive(Debug, Clone)]
pub enum Trigger {
    GitHubPush { branches: Vec<String> },
    GitHubPR { actions: Vec<String> },
    GitLabPush { branches: Vec<String> },
    GitLabMR { actions: Vec<String> },
    JiraIssue { statuses: Vec<String> },
    SlackMessage { keywords: Vec<String> },
    Timer { cron: String },
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub branch_pattern: Option<String>,
    pub file_pattern: Option<String>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum Action {
    RunAgent { agent_type: String, prompt_template: String },
    CreateTask { task_template: String },
    SendNotification { message: String },
}

#[derive(Debug, Clone)]
pub struct Callback {
    pub url: String,
    pub events: Vec<CallbackEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallbackEvent {
    Success,
    Failure,
    Always,
}

impl WebhookRouter {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add_rule(&mut self, rule: WebhookRule) {
        self.rules.push(rule);
    }

    /// 匹配事件 → 触发规则
    pub fn match_event(&self, event: &WebhookEvent) -> Vec<&WebhookRule> {
        self.rules
            .iter()
            .filter(|rule| self.matches_rule(rule, event))
            .collect()
    }

    fn matches_rule(&self, rule: &WebhookRule, event: &WebhookEvent) -> bool {
        match (&rule.trigger, event) {
            (Trigger::GitHubPush { branches }, WebhookEvent::GitHub(gh))
                if gh.push.is_some() =>
            {
                let push = gh.push.as_ref().unwrap();
                if branches.is_empty() {
                    return true;
                }
                branches.iter().any(|b| push.ref_.contains(b))
            }
            (Trigger::GitHubPR { actions }, WebhookEvent::GitHub(gh))
                if gh.pull_request.is_some() =>
            {
                if actions.is_empty() {
                    return true;
                }
                actions.iter().any(|a| gh.action == *a)
            }
            (Trigger::GitLabPush { branches }, WebhookEvent::GitLab(gl))
                if gl.push.is_some() =>
            {
                let push = gl.push.as_ref().unwrap();
                if branches.is_empty() {
                    return true;
                }
                branches.iter().any(|b| push.ref_.contains(b))
            }
            (Trigger::GitLabMR { actions }, WebhookEvent::GitLab(gl))
                if gl.merge_request.is_some() =>
            {
                if actions.is_empty() {
                    return true;
                }
                let mr = gl.merge_request.as_ref().unwrap();
                // GitLab MR actions like "open", "merge", "close"
                actions.iter().any(|a| gl.object_kind == *a)
            }
            (Trigger::JiraIssue { statuses }, WebhookEvent::Jira(jira)) => {
                if statuses.is_empty() {
                    return true;
                }
                statuses
                    .iter()
                    .any(|s| jira.issue.fields.status.name == *s)
            }
            (Trigger::SlackMessage { keywords }, WebhookEvent::Slack(slack)) => {
                if keywords.is_empty() {
                    return true;
                }
                keywords
                    .iter()
                    .any(|k| slack.text.to_lowercase().contains(&k.to_lowercase()))
            }
            (Trigger::Timer { cron: _ }, WebhookEvent::Timer(_)) => true,
            _ => false,
        }
    }

    /// 从 WebhookEvent 构建触发 Prompt
    pub fn build_prompt(&self, rule: &WebhookRule, event: &WebhookEvent) -> String {
        match (&rule.action, event) {
            (
                Action::RunAgent {
                    agent_type: _,
                    prompt_template,
                },
                WebhookEvent::GitHub(gh),
            ) => {
                let mut prompt = prompt_template.clone();
                if let Some(pr) = &gh.pull_request {
                    prompt = prompt
                        .replace("{title}", &pr.title)
                        .replace("{body}", &pr.body)
                        .replace("{head}", &pr.head)
                        .replace("{base}", &pr.base);
                }
                if let Some(push) = &gh.push {
                    prompt = prompt.replace("{ref}", &push.ref_);
                    if let Some(c) = push.commits.first() {
                        prompt = prompt.replace("{commit_msg}", &c.message);
                    }
                }
                prompt = prompt.replace("{repo}", &gh.repository.full_name);
                prompt = prompt.replace("{action}", &gh.action);
                prompt
            }
            (Action::RunAgent { prompt_template, .. }, WebhookEvent::GitLab(gl)) => {
                let mut prompt = prompt_template.clone();
                if let Some(mr) = &gl.merge_request {
                    prompt = prompt
                        .replace("{title}", &mr.title)
                        .replace("{source_branch}", &mr.source_branch)
                        .replace("{target_branch}", &mr.target_branch);
                }
                if let Some(push) = &gl.push {
                    prompt = prompt.replace("{ref}", &push.ref_);
                }
                prompt = prompt.replace("{project}", &gl.project.path_with_namespace);
                prompt = prompt.replace("{object_kind}", &gl.object_kind);
                prompt
            }
            (Action::RunAgent { prompt_template, .. }, WebhookEvent::Jira(jira)) => {
                let mut prompt = prompt_template.clone();
                prompt = prompt.replace("{issue_key}", &jira.issue.key);
                prompt = prompt.replace("{summary}", &jira.issue.fields.summary);
                prompt = prompt.replace("{status}", &jira.issue.fields.status.name);
                prompt = prompt.replace("{priority}", &jira.issue.fields.priority.name);
                prompt = prompt.replace("{description}", &jira.issue.fields.description);
                prompt
            }
            (Action::RunAgent { prompt_template, .. }, WebhookEvent::Slack(slack)) => {
                let mut prompt = prompt_template.clone();
                prompt = prompt.replace("{channel}", &slack.channel);
                prompt = prompt.replace("{user}", &slack.user);
                prompt = prompt.replace("{text}", &slack.text);
                prompt
            }
            (Action::RunAgent { prompt_template, .. }, WebhookEvent::Timer(timer)) => {
                let mut prompt = prompt_template.clone();
                prompt = prompt.replace("{schedule_id}", &timer.schedule_id);
                prompt = prompt.replace("{cron}", &timer.cron_expression);
                prompt
            }
            (Action::CreateTask { task_template }, _) => {
                // Generate structured task from event
                let event_json = serde_json::to_string(event).unwrap_or_default();
                task_template.replace("{event}", &event_json)
            }
            (Action::SendNotification { message }, _) => {
                let event_json = serde_json::to_string(event).unwrap_or_default();
                message.replace("{event}", &event_json)
            }
        }
    }
}

impl Default for WebhookRouter {
    fn default() -> Self {
        Self::new()
    }
}
