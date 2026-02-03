use crate::llm::client::LlmClient;
use crate::llm::confirm::{confirm_tool_execution, ToolConfirmationContext};
use crate::tools::{Tool, ToolRegistry};
use crate::Data;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs,
};
use serde_json::Value;
use std::sync::Arc;

pub struct Agent {
    llm: Arc<LlmClient>,
    tools: Arc<ToolRegistry>,
    mcp_manager: Arc<crate::mcp::client::McpClientManager>,
}

impl Agent {
    pub fn new(data: &Data) -> Self {
        Self {
            llm: Arc::new(crate::llm::LlmClient::new(&data.config)),
            tools: data.tools.clone(),
            mcp_manager: data.mcp_manager.clone(),
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
            // Get all available tools (built-in + MCP)
            let mut all_tools = self.tools.list_tools();
            let builtin_count = all_tools.len();
            let mcp_tools = self.mcp_manager.list_all_tools().await;
            let mcp_count = mcp_tools.len();
            all_tools.extend(mcp_tools);
            tracing::debug!(
                "Agent tools available: builtin={}, mcp={}, total={}",
                builtin_count,
                mcp_count,
                all_tools.len()
            );

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

            let response = self
                .llm
                .chat_with_tools(messages.clone(), Some(tool_definitions))
                .await?;
            let choice = response
                .choices
                .first()
                .ok_or_else(|| anyhow::anyhow!("No response from LLM"))?;

            let assistant_message = &choice.message;

            // Convert assistant response to request message for history
            let request_assistant_message = if let Some(tool_calls) = &assistant_message.tool_calls
            {
                ChatCompletionRequestAssistantMessageArgs::default()
                    .tool_calls(tool_calls.clone())
                    .build()?
            } else {
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(assistant_message.content.clone().unwrap_or_default())
                    .build()?
            };

            messages.push(request_assistant_message.into());

            if let Some(tool_calls) = &assistant_message.tool_calls {
                tracing::info!("LLM requested {} tool calls", tool_calls.len());
                for tool_call in tool_calls {
                    let result = self
                        .execute_tool_call(tool_call, &all_tools, confirmation)
                        .await?;

                    messages.push(
                        ChatCompletionRequestToolMessageArgs::default()
                            .tool_call_id(tool_call.id.clone())
                            .content(result.to_string())
                            .build()?
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
        let arguments: Value = serde_json::from_str(&tool_call.function.arguments)?;

        tracing::info!(
            "Agent executing tool: {} with arguments: {}",
            name,
            arguments
        );

        let tool = available_tools
            .iter()
            .find(|t| t.name() == name)
            .ok_or_else(|| {
                tracing::error!("Tool not found: {}", name);
                anyhow::anyhow!("Tool not found: {}", name)
            })?;

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

        let result = tool.execute(arguments).await;
        match &result {
            Ok(v) => tracing::debug!("Tool {} returned: {}", name, v),
            Err(e) => tracing::error!("Tool {} failed: {}", name, e),
        }
        result
    }
}
