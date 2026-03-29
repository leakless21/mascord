use crate::llm::client::LlmClient;
use crate::llm::confirm::{confirm_tool_execution, ToolConfirmationContext};
use crate::tools::{Tool, ToolRegistry};
use crate::Data;
use anyhow::Context as _;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage as ReqMsg, ChatCompletionRequestMessage,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageContent,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Agent {
    llm: Arc<LlmClient>,
    tools: Arc<ToolRegistry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolErrorKind {
    Transient,
    Permanent,
}

impl ToolErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::Permanent => "permanent",
        }
    }
}

#[derive(Debug)]
struct ToolRunState {
    per_tool_remaining: HashMap<String, usize>,
    total_remaining: usize,
}

impl ToolRunState {
    fn new() -> Self {
        let mut per_tool_remaining = HashMap::new();
        per_tool_remaining.insert("web_search".to_string(), 3);
        per_tool_remaining.insert("fetch_url".to_string(), 2);
        per_tool_remaining.insert("music".to_string(), 4);
        per_tool_remaining.insert("reminder".to_string(), 3);
        Self {
            per_tool_remaining,
            total_remaining: 12,
        }
    }

    fn consume(&mut self, tool_name: &str) -> Result<(), Value> {
        if self.total_remaining == 0 {
            return Err(serde_json::json!({
                "status": "error",
                "error_type": "budget_exhausted",
                "message": "Tool-call budget exhausted for this request. Summarize best effort and ask for a narrower follow-up."
            }));
        }
        self.total_remaining = self.total_remaining.saturating_sub(1);

        let key = tool_name.to_ascii_lowercase();
        let left = self.per_tool_remaining.entry(key.clone()).or_insert(4);
        if *left == 0 {
            return Err(serde_json::json!({
                "status": "error",
                "error_type": "budget_exhausted",
                "tool": key,
                "message": "This tool has reached its call budget for this request. Use another strategy or finalize."
            }));
        }
        *left = left.saturating_sub(1);
        Ok(())
    }
}

impl Agent {
    fn latest_user_text(messages: &[ChatCompletionRequestMessage]) -> Option<String> {
        messages.iter().rev().find_map(|m| match m {
            ReqMsg::User(u) => match &u.content {
                ChatCompletionRequestUserMessageContent::Text(t) => Some(t.clone()),
                ChatCompletionRequestUserMessageContent::Array(_) => None,
            },
            _ => None,
        })
    }

    /// Tool results after the latest user message mean this user turn already invoked tools;
    /// do not force another required-tool round (avoids loops when the model replies in text).
    fn has_tool_results_since_latest_user(messages: &[ChatCompletionRequestMessage]) -> bool {
        let Some(idx) = messages.iter().rposition(|m| matches!(m, ReqMsg::User(_))) else {
            return false;
        };
        messages[idx + 1..]
            .iter()
            .any(|m| matches!(m, ReqMsg::Tool(_)))
    }

    fn has_action_intent(text: &str) -> bool {
        let t = text.to_lowercase();
        [
            "play ",
            "queue ",
            "put on ",
            "add to queue ",
            "search ",
            "look up ",
            "fetch ",
            "open ",
            "check ",
        ]
        .iter()
        .any(|p| t.contains(p))
    }

    fn has_reminder_intent(text: &str) -> bool {
        let t = text.to_lowercase();
        [
            "remind me",
            "set a reminder",
            "reminder for",
            "reminder to ",
            "ping me in ",
            "remind me in ",
            "remind me at ",
            "schedule a reminder",
        ]
        .iter()
        .any(|p| t.contains(p))
    }

    fn has_music_intent(text: &str) -> bool {
        let t = text.to_lowercase();
        [
            "play ",
            "queue ",
            "put on ",
            "add to queue ",
            "skip",
            "pause",
            "resume",
            "volume",
            "lyrics",
            "shuffle",
            "now playing",
            "join vc",
            "leave vc",
            "song",
            "track",
            "music",
        ]
        .iter()
        .any(|p| t.contains(p))
    }

    fn is_reminder_set_call(name: &str, arguments: &Value) -> bool {
        if !name.eq_ignore_ascii_case("reminder") {
            return false;
        }
        arguments
            .get("action")
            .and_then(|v| v.as_str())
            .is_some_and(|a| a.eq_ignore_ascii_case("set"))
    }

    fn is_music_side_effect_call(name: &str, arguments: &Value) -> bool {
        if !name.eq_ignore_ascii_case("music") {
            return false;
        }
        let Some(action) = arguments.get("action").and_then(|v| v.as_str()) else {
            return false;
        };
        // Read-only-ish actions stay available without strict gating.
        !matches!(
            action.to_ascii_lowercase().as_str(),
            "help" | "queue" | "now_playing" | "lyrics"
        )
    }

    fn should_retry_required_tool_call(messages: &[ChatCompletionRequestMessage]) -> bool {
        if Self::has_tool_results_since_latest_user(messages) {
            return false;
        }
        let Some(user_text) = Self::latest_user_text(messages) else {
            return false;
        };
        Self::has_action_intent(&user_text) || Self::has_reminder_intent(&user_text)
    }

    fn classify_tool_error(message: &str) -> ToolErrorKind {
        let m = message.to_ascii_lowercase();
        if m.contains("timed out")
            || m.contains("timeout")
            || m.contains("429")
            || m.contains("too many requests")
            || m.contains("temporar")
            || m.contains("connection reset")
            || m.contains("service unavailable")
        {
            return ToolErrorKind::Transient;
        }
        ToolErrorKind::Permanent
    }

    fn is_task_complete_from_tool(name: &str, result: &Value) -> bool {
        if !name.eq_ignore_ascii_case("music") {
            return false;
        }
        let status_ok = result
            .get("status")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("ok"));
        let is_play = result
            .get("action")
            .and_then(|v| v.as_str())
            .is_some_and(|a| a.eq_ignore_ascii_case("play"));
        status_ok && is_play
    }

    pub fn new(data: &Data) -> Self {
        Self {
            llm: Arc::new(crate::llm::LlmClient::new(&data.config)),
            tools: data.tools.clone(),
        }
    }

    pub async fn run(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        max_iterations: usize,
    ) -> anyhow::Result<String> {
        self.run_inner(None, messages, max_iterations).await
    }

    pub async fn run_with_confirmation<'a>(
        &self,
        confirmation: ToolConfirmationContext<'a>,
        messages: Vec<ChatCompletionRequestMessage>,
        max_iterations: usize,
    ) -> anyhow::Result<String> {
        self.run_inner(Some(&confirmation), messages, max_iterations)
            .await
    }

    async fn run_inner<'a>(
        &self,
        confirmation: Option<&ToolConfirmationContext<'a>>,
        mut messages: Vec<ChatCompletionRequestMessage>,
        max_iterations: usize,
    ) -> anyhow::Result<String> {
        let mut tool_state = ToolRunState::new();
        let mut task_completed = false;

        for i in 0..max_iterations {
            tracing::info!("Agent iteration {}/{}", i + 1, max_iterations);
            let all_tools = self.tools.list_tools();
            tracing::debug!("Agent tools available: {}", all_tools.len());

            // Build tool definitions for OpenAI
            let tool_definitions: Vec<Value> = all_tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name(),
                            "description": t.description(),
                            "parameters": t.parameters_schema()
                        }
                    })
                })
                .collect();

            let mut response = self
                .llm
                .chat_with_tools(messages.clone(), Some(tool_definitions))
                .await?;
            let mut choice = response
                .choices
                .first()
                .ok_or_else(|| anyhow::anyhow!("No response from LLM"))?;

            if choice.message.tool_calls.is_none()
                && Self::should_retry_required_tool_call(&messages)
            {
                tracing::warn!(
                    "No tool call on explicit action request; retrying with required tool mode"
                );
                response = self
                    .llm
                    .chat_with_tools_required(
                        messages.clone(),
                        Some(
                            all_tools
                                .iter()
                                .map(|t| {
                                    serde_json::json!({
                                        "type": "function",
                                        "function": {
                                            "name": t.name(),
                                            "description": t.description(),
                                            "parameters": t.parameters_schema()
                                        }
                                    })
                                })
                                .collect(),
                        ),
                    )
                    .await?;
                choice = response.choices.first().ok_or_else(|| {
                    anyhow::anyhow!("No response from LLM after required-tool retry")
                })?;
            }

            if choice
                .message
                .tool_calls
                .as_ref()
                .map(|c| c.is_empty())
                .unwrap_or(false)
            {
                return Err(anyhow::anyhow!(
                    "Model returned empty tool_calls; refusing to continue"
                ));
            }

            if choice.message.tool_calls.is_none()
                && Self::should_retry_required_tool_call(&messages)
            {
                return Err(anyhow::anyhow!(
                    "The model did not invoke a tool for an explicit action request (even after required-tool retry). For music try /play, the `music` tool (action play/skip/volume/…), or a direct \"play …\" phrase; for reminders use /reminder or the `reminder` tool (action set/list/cancel). Also check LLM_URL / model availability."
                ));
            }

            let assistant_message = &choice.message;

            // Convert assistant response to request message for history
            let request_assistant_message = if let Some(tool_calls) = &assistant_message.tool_calls
            {
                ChatCompletionRequestAssistantMessageArgs::default()
                    .tool_calls(tool_calls.clone())
                    .build()
                    .context("failed to serialize assistant tool_calls message")?
            } else {
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(assistant_message.content.clone().unwrap_or_default())
                    .build()
                    .context("failed to build assistant message (no tools)")?
            };

            messages.push(request_assistant_message.into());

            if let Some(tool_calls) = &assistant_message.tool_calls {
                if task_completed {
                    return Ok(
                        "Done. I found a playable track and queued it. Tell me if you want another one."
                            .to_string(),
                    );
                }
                tracing::info!("LLM requested {} tool calls", tool_calls.len());
                for tool_call in tool_calls {
                    let result = match tool_state.consume(&tool_call.function.name) {
                        Ok(()) => match self
                            .execute_tool_call(tool_call, &messages, &all_tools, confirmation)
                            .await
                        {
                            Ok(v) => v,
                            Err(e) => {
                                let msg = format!("{:#}", e);
                                let kind = Self::classify_tool_error(&msg);
                                serde_json::json!({
                                    "status": "error",
                                    "tool": tool_call.function.name,
                                    "error_type": kind.as_str(),
                                    "retryable": kind == ToolErrorKind::Transient,
                                    "message": msg
                                })
                            }
                        },
                        Err(v) => v,
                    };

                    messages.push(
                        ChatCompletionRequestToolMessageArgs::default()
                            .tool_call_id(tool_call.id.clone())
                            .content(result.to_string())
                            .build()
                            .context("failed to build tool result message")?
                            .into(),
                    );

                    if Self::is_task_complete_from_tool(&tool_call.function.name, &result) {
                        task_completed = true;
                    }
                }
                // Continue the loop to let the LLM see the results
            } else {
                // No more tool calls, return final content
                tracing::info!("Agent task completed after {} iterations", i + 1);
                return Ok(assistant_message
                    .content
                    .clone()
                    .unwrap_or_else(|| "...".to_string()));
            }
        }

        tracing::warn!(
            "Agent exceeded max iterations ({}) - potential runaway loop or recursive tool calls",
            max_iterations
        );
        Err(anyhow::anyhow!("I've reached my reasoning limit for this task ({} steps). To improve results, try breaking your request into smaller, more specific steps.", max_iterations))
    }

    async fn execute_tool_call(
        &self,
        tool_call: &ChatCompletionMessageToolCall,
        messages: &[ChatCompletionRequestMessage],
        available_tools: &[Arc<dyn Tool>],
        confirmation: Option<&ToolConfirmationContext<'_>>,
    ) -> anyhow::Result<Value> {
        let name = &tool_call.function.name;
        let arguments: Value = serde_json::from_str(&tool_call.function.arguments)
            .with_context(|| format!("invalid JSON in tool arguments for `{}`", name))?;

        if Self::is_reminder_set_call(name, &arguments)
            && !Self::latest_user_text(messages)
                .as_deref()
                .is_some_and(Self::has_reminder_intent)
        {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "Blocked reminder set: latest user message did not request a reminder."
            }));
        }

        if Self::is_music_side_effect_call(name, &arguments)
            && !Self::latest_user_text(messages)
                .as_deref()
                .is_some_and(Self::has_music_intent)
        {
            return Ok(serde_json::json!({
                "status": "error",
                "message": "Blocked music action: latest user message did not clearly request music control."
            }));
        }

        if self.llm.log_llm_tool_args() {
            tracing::debug!(
                "Agent executing tool: {} with arguments: {}",
                name,
                arguments
            );
        } else {
            tracing::info!("Agent executing tool: {}", name);
        }

        let tool = available_tools
            .iter()
            .find(|t| t.name() == name)
            .or_else(|| {
                available_tools
                    .iter()
                    .find(|t| t.name().eq_ignore_ascii_case(name))
            })
            .ok_or_else(|| {
                tracing::error!("Tool not found: {}", name);
                anyhow::anyhow!("Tool not found: {} (not registered)", name)
            })?;
        if name != tool.name() {
            tracing::warn!(
                "Tool name casing mismatch: model sent `{}`, using registered `{}`",
                name,
                tool.name()
            );
        }

        if tool.requires_confirmation() {
            let Some(confirm_ctx) = confirmation else {
                return Err(anyhow::anyhow!(
                    "Tool '{}' requires confirmation, but this conversation does not support interactive confirmation.",
                    name
                ));
            };

            let confirmed = confirm_tool_execution(confirm_ctx, name, &arguments).await?;
            if !confirmed {
                return Err(anyhow::anyhow!("Tool execution cancelled."));
            }
        }

        let result = if let Some(c) = confirmation {
            let dc = crate::llm::confirm::DiscordToolContext {
                serenity_ctx: c.serenity_ctx,
                guild_id: c.guild_id,
                channel_id: c.channel_id,
                user_id: c.user_id,
                data: c.data,
            };
            tool.execute_with_discord(arguments, Some(&dc)).await
        } else {
            tool.execute(arguments).await
        };
        match &result {
            Ok(v) => tracing::debug!("Tool {} returned: {}", name, v),
            Err(e) => tracing::error!("Tool {} failed: {}", name, e),
        }
        result
    }
}
