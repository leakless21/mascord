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
use std::sync::Arc;

pub struct Agent {
    llm: Arc<LlmClient>,
    tools: Arc<ToolRegistry>,
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
        let Some(idx) = messages
            .iter()
            .rposition(|m| matches!(m, ReqMsg::User(_)))
        else {
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

    fn should_retry_required_tool_call(messages: &[ChatCompletionRequestMessage]) -> bool {
        if Self::has_tool_results_since_latest_user(messages) {
            return false;
        }
        let Some(user_text) = Self::latest_user_text(messages) else {
            return false;
        };
        Self::has_action_intent(&user_text) || Self::has_reminder_intent(&user_text)
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
                tracing::info!("LLM requested {} tool calls", tool_calls.len());
                for tool_call in tool_calls {
                    let result = self
                        .execute_tool_call(tool_call, &all_tools, confirmation)
                        .await
                        .with_context(|| {
                            format!("tool `{}` execution failed", tool_call.function.name)
                        })?;

                    messages.push(
                        ChatCompletionRequestToolMessageArgs::default()
                            .tool_call_id(tool_call.id.clone())
                            .content(result.to_string())
                            .build()
                            .context("failed to build tool result message")?
                            .into(),
                    );
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
        available_tools: &[Arc<dyn Tool>],
        confirmation: Option<&ToolConfirmationContext<'_>>,
    ) -> anyhow::Result<Value> {
        let name = &tool_call.function.name;
        let arguments: Value = serde_json::from_str(&tool_call.function.arguments)
            .with_context(|| format!("invalid JSON in tool arguments for `{}`", name))?;

        tracing::info!(
            "Agent executing tool: {} with arguments: {}",
            name,
            arguments
        );

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
