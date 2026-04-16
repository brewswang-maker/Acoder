//! 会话可视化模块
//!
//! 核心功能：
//! - 时序图展示 Agent 思考链路
//! - 检查点 Resume
//! - 时间线回溯
//! - HTML / Mermaid 导出

use crate::error::{Error, Result};
use crate::memory::{MemoryManager, MemoryItem, MemoryType};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// 数据结构
// ─────────────────────────────────────────────────────────────────────────────

/// Agent 执行轨迹
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrack {
    pub agent_id: String,
    pub agent_name: String,
    pub agent_role: String,
    pub color: String,
    pub spans: Vec<ExecutionSpan>,
}

/// 单个执行时间段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSpan {
    pub span_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub action: SpanAction,
    pub tool_calls: Vec<ToolCallRef>,
    pub result_summary: String,
    pub token_used: TokenUsage,
    pub status: SpanStatus,
}

/// Agent 操作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpanAction {
    Thinking { reasoning: String },
    Planning { plan: String },
    Executing { tool: String },
    Reviewing { result: String },
    WaitingApproval,
    Blocked { reason: String },
    Done,
}

/// 工具调用引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRef {
    pub call_id: String,
    pub tool_name: String,
    pub args_summary: String,
    pub result_summary: String,
}

/// Token 使用统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Span 状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpanStatus {
    Running,
    Completed,
    Failed,
    Blocked,
    WaitingApproval,
}

/// 关键决策点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub decision_id: String,
    pub timestamp: DateTime<Utc>,
    pub agent_id: String,
    pub agent_name: String,
    pub decision_type: DecisionType,
    pub description: String,
    pub reasoning: String,
    pub alternatives_considered: Vec<String>,
    pub chosen_option: String,
    pub outcome: Option<String>,
}

/// 决策类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionType {
    ToolSelection,
    CodeGeneration,
    TaskRouting,
    Approval,
    Rollback,
    StrategyChange,
}

/// 检查点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub checkpoint_id: String,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub context_snapshot: ContextSnapshot,
    pub agent_state: HashMap<String, AgentState>,
    pub memory_state: MemoryState,
    pub event_index: usize,
    pub is_auto_save: bool,
}

/// 上下文快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub task_description: String,
    pub project_root: String,
    pub active_files: Vec<String>,
    pub working_memory_summary: String,
    pub recent_tool_results: Vec<String>,
    pub llm_conversation_history: Vec<ConversationTurn>,
}

/// 对话轮次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub turn_id: usize,
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub token_count: usize,
}

/// Agent 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub agent_id: String,
    pub agent_name: String,
    pub current_task: String,
    pub progress: f32,
    pub status: String,
}

/// 记忆状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryState {
    pub working_memory_size: usize,
    pub session_memory_entries: usize,
    pub longterm_hints: Vec<String>,
}

/// 会话时序图
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTimeline {
    pub session_id: String,
    pub task_name: String,
    pub agent_tracks: Vec<AgentTrack>,
    pub decisions: Vec<Decision>,
    pub checkpoints: Vec<Checkpoint>,
    pub total_duration_secs: u64,
    pub total_tokens: TokenUsage,
    pub status: TimelineStatus,
    pub created_at: DateTime<Utc>,
}

/// 时间线状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimelineStatus {
    Running,
    Completed,
    PartiallyRolledBack,
    Failed,
}

/// Resume 上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeContext {
    pub checkpoint: Checkpoint,
    pub restored_agents: Vec<AgentState>,
    pub restored_context: ContextSnapshot,
    pub continuation_prompt: String,
}

/// 会话事件（内部使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_type: String,
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub payload: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// SessionVisualizer
// ─────────────────────────────────────────────────────────────────────────────

/// 会话可视化器
pub struct SessionVisualizer {
    data_dir: std::path::PathBuf,
}

impl SessionVisualizer {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self { data_dir }
    }

    /// 从会话事件流生成时序图
    pub async fn generate_timeline(&self, session_id: &str) -> Result<SessionTimeline> {
        let timeline_path = self.data_dir.join("sessions").join(format!("{}.json", session_id));
        
        let events: Vec<Event> = if timeline_path.exists() {
            let content = tokio::fs::read_to_string(&timeline_path).await?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        let agent_tracks = self.build_agent_tracks(&events);
        let decisions = self.extract_decisions(&events);
        let checkpoints = self.load_checkpoints(session_id).await?;
        let (total_tokens, total_duration, status) = self.compute_summary(&events);

        Ok(SessionTimeline {
            session_id: session_id.to_string(),
            task_name: events.first()
                .and_then(|e| e.payload.get("task").and_then(|v| v.as_str()))
                .unwrap_or("Unknown Task")
                .to_string(),
            agent_tracks,
            decisions,
            checkpoints,
            total_duration_secs: total_duration,
            total_tokens,
            status,
            created_at: Utc::now(),
        })
    }

    /// 从任意检查点 Resume
    pub async fn resume_from_checkpoint(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<ResumeContext> {
        let checkpoints = self.load_checkpoints(session_id).await?;
        
        let checkpoint = checkpoints
            .iter()
            .find(|c| c.checkpoint_id == checkpoint_id)
            .ok_or_else(|| anyhow!("checkpoint {} not found", checkpoint_id))?
            .clone();

        let continuation_prompt = format!(
            "[Resume from checkpoint {}] {}",
            checkpoint.timestamp.format("%Y-%m-%d %H:%M:%S"),
            checkpoint.context_snapshot.task_description
        );

        let restored_agents: Vec<AgentState> = checkpoint.agent_state.values().cloned().collect();

        Ok(ResumeContext {
            checkpoint: checkpoint.clone(),
            restored_agents,
            restored_context: checkpoint.context_snapshot.clone(),
            continuation_prompt,
        })
    }

    /// 保存检查点
    pub async fn save_checkpoint(
        &self,
        session_id: &str,
        description: &str,
        context_snapshot: ContextSnapshot,
        agent_state: HashMap<String, AgentState>,
        memory_state: MemoryState,
        event_index: usize,
        is_auto_save: bool,
    ) -> Result<Checkpoint> {
        let checkpoint = Checkpoint {
            checkpoint_id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            timestamp: Utc::now(),
            description: description.to_string(),
            context_snapshot,
            agent_state,
            memory_state,
            event_index,
            is_auto_save,
        };

        let checkpoint_dir = self.data_dir.join("checkpoints").join(session_id);
        tokio::fs::create_dir_all(&checkpoint_dir).await?;

        let path = checkpoint_dir.join(format!("{}.json", checkpoint.checkpoint_id));
        let content = serde_json::to_string_pretty(&checkpoint)?;
        tokio::fs::write(&path, content).await?;

        Ok(checkpoint)
    }

    /// 生成 HTML 可视化
    pub fn generate_html_timeline(&self, timeline: &SessionTimeline) -> String {
        let agents_json = serde_json::to_string(&timeline.agent_tracks).unwrap_or_default();
        let decisions_json = serde_json::to_string(&timeline.decisions).unwrap_or_default();
        let checkpoints_json = serde_json::to_string(&timeline.checkpoints).unwrap_or_default();

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Session Timeline - {title}</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #0f0f1a; color: #e0e0e0; padding: 2rem; }}
  h1 {{ color: #fff; margin-bottom: 1rem; font-size: 1.5rem; }}
  .summary {{ background: #1a1a2e; border-radius: 8px; padding: 1rem; margin-bottom: 1.5rem; display: flex; gap: 2rem; flex-wrap: wrap; }}
  .summary-item {{ display: flex; flex-direction: column; gap: 0.25rem; }}
  .summary-label {{ font-size: 0.75rem; color: #888; text-transform: uppercase; letter-spacing: 0.05em; }}
  .summary-value {{ font-size: 1.25rem; font-weight: 600; color: #7dd3fc; }}
  .tabs {{ display: flex; gap: 0.5rem; margin-bottom: 1rem; }}
  .tab {{ padding: 0.5rem 1rem; background: #1a1a2e; border: none; border-radius: 6px 6px 0 0; color: #888; cursor: pointer; font-size: 0.875rem; }}
  .tab.active {{ background: #1e293b; color: #7dd3fc; }}
  .panel {{ display: none; background: #1e293b; border-radius: 0 8px 8px 8px; padding: 1rem; }}
  .panel.active {{ display: block; }}
  .agent-track {{ margin-bottom: 1.5rem; }}
  .agent-header {{ display: flex; align-items: center; gap: 0.75rem; margin-bottom: 0.75rem; }}
  .agent-color {{ width: 12px; height: 12px; border-radius: 50%; }}
  .agent-name {{ font-weight: 600; color: #fff; }}
  .agent-role {{ font-size: 0.75rem; color: #888; }}
  .spans {{ display: flex; flex-direction: column; gap: 0.5rem; padding-left: 1.5rem; border-left: 2px solid #334155; }}
  .span {{ background: #0f0f1a; border-radius: 6px; padding: 0.75rem; border-left: 3px solid #4ade80; }}
  .span.thinking {{ border-left-color: #facc15; }}
  .span.planning {{ border-left-color: #60a5fa; }}
  .span.executing {{ border-left-color: #4ade80; }}
  .span.reviewing {{ border-left-color: #c084fc; }}
  .span.blocked {{ border-left-color: #f87171; }}
  .span.waiting {{ border-left-color: #fb923c; }}
  .span-header {{ display: flex; justify-content: space-between; margin-bottom: 0.5rem; }}
  .span-action {{ font-weight: 500; font-size: 0.875rem; }}
  .span-time {{ font-size: 0.75rem; color: #888; }}
  .span-detail {{ font-size: 0.8rem; color: #94a3b8; margin-top: 0.25rem; }}
  .span-tokens {{ font-size: 0.7rem; color: #64748b; margin-top: 0.25rem; }}
  .decision {{ background: #0f0f1a; border-radius: 6px; padding: 0.75rem; margin-bottom: 0.5rem; border-left: 3px solid #facc15; }}
  .decision-type {{ font-size: 0.7rem; text-transform: uppercase; color: #facc15; letter-spacing: 0.05em; margin-bottom: 0.25rem; }}
  .decision-desc {{ font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem; }}
  .decision-reasoning {{ font-size: 0.8rem; color: #94a3b8; }}
  .checkpoint {{ background: #0f0f1a; border-radius: 6px; padding: 0.75rem; margin-bottom: 0.5rem; border-left: 3px solid #7dd3fc; cursor: pointer; }}
  .checkpoint:hover {{ background: #1a1a2e; }}
  .checkpoint-desc {{ font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem; }}
  .checkpoint-meta {{ font-size: 0.75rem; color: #888; }}
  .status-badge {{ display: inline-block; padding: 0.2rem 0.5rem; border-radius: 4px; font-size: 0.7rem; font-weight: 600; text-transform: uppercase; }}
  .status-running {{ background: #166534; color: #86efac; }}
  .status-completed {{ background: #1e40af; color: #93c5fd; }}
  .status-failed {{ background: #991b1b; color: #fca5a5; }}
  .mermaid {{ background: #fff; color: #333; border-radius: 8px; padding: 1rem; overflow-x: auto; font-size: 0.8rem; }}
  #data {{ display: none; }}
</style>
</head>
<body>
<h1>🤖 Session Timeline: {title}</h1>

<div class="summary">
  <div class="summary-item">
    <span class="summary-label">Session ID</span>
    <span class="summary-value">{session_id}</span>
  </div>
  <div class="summary-item">
    <span class="summary-label">Status</span>
    <span class="summary-value"><span class="status-badge status-{status_slug}">{status}</span></span>
  </div>
  <div class="summary-item">
    <span class="summary-label">Duration</span>
    <span class="summary-value">{duration}s</span>
  </div>
  <div class="summary-item">
    <span class="summary-label">Total Tokens</span>
    <span class="summary-value">{total_tokens}</span>
  </div>
  <div class="summary-item">
    <span class="summary-label">Agents</span>
    <span class="summary-value">{agent_count}</span>
  </div>
  <div class="summary-item">
    <span class="summary-label">Decisions</span>
    <span class="summary-value">{decision_count}</span>
  </div>
  <div class="summary-item">
    <span class="summary-label">Checkpoints</span>
    <span class="summary-value">{checkpoint_count}</span>
  </div>
</div>

<div class="tabs">
  <button class="tab active" onclick="showTab('tracks')">Agent Tracks</button>
  <button class="tab" onclick="showTab('decisions')">Decisions</button>
  <button class="tab" onclick="showTab('checkpoints')">Checkpoints</button>
  <button class="tab" onclick="showTab('mermaid')">Mermaid Diagram</button>
</div>

<div id="tracks" class="panel active">
  <div id="tracks-content"></div>
</div>
<div id="decisions" class="panel">
  <div id="decisions-content"></div>
</div>
<div id="checkpoints" class="panel">
  <div id="checkpoints-content"></div>
</div>
<div id="mermaid" class="panel">
  <pre id="mermaid-content" class="mermaid"></pre>
</div>

<div id="data"
  data-agents='{agents_json}'
  data-decisions='{decisions_json}'
  data-checkpoints='{checkpoints_json}'>
</div>

<script>
const data = {{
  agents: JSON.parse(document.getElementById('data').dataset.agents),
  decisions: JSON.parse(document.getElementById('data').dataset.decisions),
  checkpoints: JSON.parse(document.getElementById('data').dataset.checkpoints)
}};

function showTab(name) {{
  document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
  document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
  document.querySelector(`[onclick="showTab('{{name}}')"]`).classList.add('active');
  document.getElementById('{{name}}').classList.add('active');
}}

function renderTracks() {{
  const container = document.getElementById('tracks-content');
  container.innerHTML = data.agents.map(agent => `
    <div class="agent-track">
      <div class="agent-header">
        <div class="agent-color" style="background:${{agent.color}}"></div>
        <span class="agent-name">${{agent.agent_name}}</span>
        <span class="agent-role">(${{agent.agent_role}})</span>
      </div>
      <div class="spans">
        ${{agent.spans.map(span => `
          <div class="span ${{spanActionClass(span)}}">
            <div class="span-header">
              <span class="span-action">${{spanActionLabel(span)}}</span>
              <span class="span-time">${{formatTime(span)}}</span>
            </div>
            <div class="span-detail">${{span.result_summary || ''}}</div>
            ${{span.token_used && span.token_used.total_tokens > 0 ? `<div class="span-tokens">Tokens: ${{span.token_used.total_tokens}}</div>` : ''}}
          </div>
        `).join('')}}
      </div>
    </div>
  `).join('');
}}

function spanActionClass(span) {{
  const map = {{
    'Thinking': 'thinking', 'Planning': 'planning', 'Executing': 'executing',
    'Reviewing': 'reviewing', 'Blocked': 'blocked', 'WaitingApproval': 'waiting'
  }};
  return map[span.action] || '';
}}

function spanActionLabel(span) {{
  if (span.action.Thinking) return '🤔 Thinking: ' + span.action.Thinking.reasoning.substring(0, 60) + '...';
  if (span.action.Planning) return '📋 Planning: ' + span.action.Planning.plan.substring(0, 60) + '...';
  if (span.action.Executing) return '⚙️  Executing: ' + span.action.Executing.tool;
  if (span.action.Reviewing) return '🔍 Reviewing: ' + span.action.Reviewing.result.substring(0, 60) + '...';
  if (span.action.Blocked) return '🚫 Blocked: ' + span.action.Blocked.reason;
  if (span.action.WaitingApproval) return '⏳ Waiting Approval';
  if (span.action.Done) return '✅ Done';
  return 'Unknown';
}}

function formatTime(span) {{
  if (!span.start_time) return '';
  const d = new Date(span.start_time);
  return d.toLocaleTimeString();
}}

function renderDecisions() {{
  const container = document.getElementById('decisions-content');
  container.innerHTML = data.decisions.map(d => `
    <div class="decision">
      <div class="decision-type">${{d.decision_type}}</div>
      <div class="decision-desc">${{d.description}}</div>
      <div class="decision-reasoning">${{d.reasoning}}</div>
    </div>
  `).join('') || '<p style="color:#888">No decisions recorded</p>';
}}

function renderCheckpoints() {{
  const container = document.getElementById('checkpoints-content');
  container.innerHTML = data.checkpoints.map(c => `
    <div class="checkpoint" onclick="resumeFrom('${{c.checkpoint_id}}')">
      <div class="checkpoint-desc">${{c.description}}</div>
      <div class="checkpoint-meta">
        ${{new Date(c.timestamp).toLocaleString()}} · ${{c.is_auto_save ? 'Auto' : 'Manual'}}
      </div>
    </div>
  `).join('') || '<p style="color:#888">No checkpoints saved</p>';
}}

function resumeFrom(id) {{
  alert('Resume from checkpoint: ' + id + '\\n(In production, this would trigger resume_from_checkpoint)');
}}

function renderMermaid() {{
  const lines = ['sequenceDiagram'];
  data.agents.forEach(a => {{
    lines.push(`    participant ${{a.agent_id}} as ${{a.agent_name}}`);
  }});
  data.decisions.forEach(d => {{
    lines.push(`    Note over ${{d.agent_id}}: ${{d.decision_type}}: ${{d.description}}`);
  }});
  document.getElementById('mermaid-content').textContent = lines.join('\\n');
}}

renderTracks();
renderDecisions();
renderCheckpoints();
renderMermaid();
</script>
</body>
</html>"#,
            title = timeline.task_name,
            session_id = timeline.session_id,
            status = format!("{:?}", timeline.status),
            status_slug = format!("{:?}", timeline.status).to_lowercase(),
            duration = timeline.total_duration_secs,
            total_tokens = timeline.total_tokens.total_tokens,
            agent_count = timeline.agent_tracks.len(),
            decision_count = timeline.decisions.len(),
            checkpoint_count = timeline.checkpoints.len(),
            agents_json = agents_json,
            decisions_json = decisions_json,
            checkpoints_json = checkpoints_json,
        )
    }

    /// 提取关键决策
    pub fn extract_decisions(&self, events: &[Event]) -> Vec<Decision> {
        events
            .iter()
            .filter(|e| e.event_type == "decision")
            .map(|e| {
                let timestamp = e.timestamp;
                let agent_id = e.agent_id.clone();
                let agent_name = e.payload.get("agent_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let decision_type = e.payload.get("decision_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ToolSelection");
                let decision_type = match decision_type {
                    "ToolSelection" => DecisionType::ToolSelection,
                    "CodeGeneration" => DecisionType::CodeGeneration,
                    "TaskRouting" => DecisionType::TaskRouting,
                    "Approval" => DecisionType::Approval,
                    "Rollback" => DecisionType::Rollback,
                    "StrategyChange" => DecisionType::StrategyChange,
                    _ => DecisionType::ToolSelection,
                };
                Decision {
                    decision_id: Uuid::new_v4().to_string(),
                    timestamp,
                    agent_id,
                    agent_name,
                    decision_type,
                    description: e.payload.get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    reasoning: e.payload.get("reasoning")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    alternatives_considered: e.payload
                        .get("alternatives")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    chosen_option: e.payload.get("chosen")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    outcome: e.payload.get("outcome").and_then(|v| v.as_str()).map(String::from),
                }
            })
            .collect()
    }

    /// 生成 Mermaid 时序图
    pub fn generate_mermaid_sequence(&self, timeline: &SessionTimeline) -> String {
        let mut lines = vec!["sequenceDiagram".to_string()];

        // Agent participants
        for track in &timeline.agent_tracks {
            lines.push(format!(
                "    participant {} as {}",
                track.agent_id, track.agent_name
            ));
        }

        // Decision notes
        for decision in &timeline.decisions {
            lines.push(format!(
                "    Note over {}: {}: {}",
                decision.agent_id,
                format!("{:?}", decision.decision_type),
                decision.description.chars().take(50).collect::<String>()
            ));
        }

        // Checkpoint markers
        for cp in &timeline.checkpoints {
            lines.push(format!(
                "    Note over {}: 📌 Checkpoint: {}",
                timeline
                    .agent_tracks
                    .first()
                    .map(|a| a.agent_id.as_str())
                    .unwrap_or("Agent"),
                cp.description.chars().take(40).collect::<String>()
            ));
        }

        lines.join("\n")
    }

    // ── 内部辅助方法 ──────────────────────────────────────────────

    fn build_agent_tracks(&self, events: &[Event]) -> Vec<AgentTrack> {
        let mut tracks: HashMap<String, AgentTrack> = HashMap::new();

        for event in events {
            let agent_id = &event.agent_id;
            let agent_name = event
                .payload
                .get("agent_name")
                .and_then(|v| v.as_str())
                .unwrap_or(agent_id)
                .to_string();

            let track = tracks
                .entry(agent_id.clone())
                .or_insert_with(|| AgentTrack {
                    agent_id: agent_id.clone(),
                    agent_name: agent_name.clone(),
                    agent_role: event
                        .payload
                        .get("agent_role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("agent")
                        .to_string(),
                    color: Self::agent_color(&agent_id),
                    spans: Vec::new(),
                });

            if event.event_type == "span" {
                if let Some(span) = self.build_span(event) {
                    track.spans.push(span);
                }
            }
        }

        tracks.into_values().collect()
    }

    fn build_span(&self, event: &Event) -> Option<ExecutionSpan> {
        let action_str = event.payload.get("action")?.as_str()?;
        let span_status_str = event.payload.get("status").and_then(|v| v.as_str()).unwrap_or("Completed");

        let action = match action_str {
            "thinking" => SpanAction::Thinking {
                reasoning: event.payload.get("reasoning").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            },
            "planning" => SpanAction::Planning {
                plan: event.payload.get("plan").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            },
            "executing" => SpanAction::Executing {
                tool: event.payload.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            },
            "reviewing" => SpanAction::Reviewing {
                result: event.payload.get("result").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            },
            "waiting_approval" => SpanAction::WaitingApproval,
            "blocked" => SpanAction::Blocked {
                reason: event.payload.get("reason").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            },
            "done" => SpanAction::Done,
            _ => SpanAction::Done,
        };

        let status = match span_status_str {
            "running" => SpanStatus::Running,
            "completed" => SpanStatus::Completed,
            "failed" => SpanStatus::Failed,
            "blocked" => SpanStatus::Blocked,
            "waiting_approval" => SpanStatus::WaitingApproval,
            _ => SpanStatus::Completed,
        };

        let token_used = event.payload.get("tokens").and_then(|v| {
            serde_json::from_value(v.clone()).ok()
        }).unwrap_or_default();

        Some(ExecutionSpan {
            span_id: Uuid::new_v4().to_string(),
            start_time: event.timestamp,
            end_time: event.payload.get("end_time").and_then(|v| {
                serde_json::from_value(v.clone()).ok()
            }),
            action,
            tool_calls: event.payload.get("tool_calls")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            result_summary: event.payload.get("result_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            token_used,
            status,
        })
    }

    async fn load_checkpoints(&self, session_id: &str) -> Result<Vec<Checkpoint>> {
        let dir = self.data_dir.join("checkpoints").join(session_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                    if let Ok(cp) = serde_json::from_str::<Checkpoint>(&content) {
                        checkpoints.push(cp);
                    }
                }
            }
        }
        checkpoints.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(checkpoints)
    }

    fn compute_summary(&self, events: &[Event]) -> (TokenUsage, u64, TimelineStatus) {
        let mut total_tokens = TokenUsage::default();
        let start = events.first().map(|e| e.timestamp);
        let end = events.last().map(|e| e.timestamp);

        let duration = match (start, end) {
            (Some(s), Some(e)) => (e - s).num_seconds() as u64,
            _ => 0,
        };

        for event in events {
            if let Some(tokens) = event.payload.get("tokens") {
                if let Ok(t) = serde_json::from_value::<TokenUsage>(tokens.clone()) {
                    total_tokens.prompt_tokens += t.prompt_tokens;
                    total_tokens.completion_tokens += t.completion_tokens;
                    total_tokens.total_tokens += t.total_tokens;
                }
            }
        }

        let status = if events.iter().any(|e| e.event_type == "failure") {
            TimelineStatus::Failed
        } else if events.iter().any(|e| e.event_type == "rollback") {
            TimelineStatus::PartiallyRolledBack
        } else if events.iter().any(|e| e.event_type == "complete") {
            TimelineStatus::Completed
        } else {
            TimelineStatus::Running
        };

        (total_tokens, duration, status)
    }

    fn agent_color(agent_id: &str) -> String {
        let colors = ["#4ade80", "#60a5fa", "#facc15", "#c084fc", "#f87171", "#fb923c", "#2dd4bf"];
        let hash: usize = agent_id.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
        colors[hash % colors.len()].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_color_deterministic() {
        let c1 = SessionVisualizer::agent_color("agent-1");
        let c2 = SessionVisualizer::agent_color("agent-1");
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_token_usage_default() {
        let t = TokenUsage::default();
        assert_eq!(t.total_tokens, 0);
    }
}
