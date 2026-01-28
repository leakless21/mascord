use crate::Data;
use crate::llm::client::LlmClient;
use crate::tools::{ToolRegistry, Tool};
use async_openai::types::{
    ChatCompletionRequestMessage,
    ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestToolMessageArgs,
    ChatCompletionMessageToolCall,
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
        mut messages: Vec<ChatCompletionRequestMessage>,
        max_iterations: usize,
    ) -> anyhow::Result<String> {
        for _ in 0..max_iterations {
            // Get all available tools (built-in + MCP)
            let mut all_tools = self.tools.list_tools();
            let mcp_tools = self.mcp_manager.list_all_tools().await;
            all_tools.extend(mcp_tools);
            
            // Build tool definitions for OpenAI
            let tool_definitions: Vec<Value> = all_tools.iter().map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.parameters_schema()
                    }
                })
            }).collect();

            let response = self.llm.chat_with_tools(messages.clone(), Some(tool_definitions)).await?;
            let choice = response.choices.first().ok_or_else(|| anyhow::anyhow!("No response from LLM"))?;
            
            let assistant_message = &choice.message;
            
            // Convert assistant response to request message for history
            let request_assistant_message = if let Some(tool_calls) = &assistant_message.tool_calls {
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
                for tool_call in tool_calls {
                    let result = self.execute_tool_call(tool_call, &all_tools).await?;
                    
                    messages.push(ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(tool_call.id.clone())
                        .content(result.to_string())
                        .build()?
                        .into());
                }
                // Continue the loop to let the LLM see the results
            } else {
                // No more tool calls, return final content
                return Ok(assistant_message.content.clone().unwrap_or_else(|| "...".to_string()));
            }
        }

        Err(anyhow::anyhow!("Agent exceeded max iterations"))
    }

    async fn execute_tool_call(
        &self,
        tool_call: &ChatCompletionMessageToolCall,
        available_tools: &[Arc<dyn Tool>],
    ) -> anyhow::Result<Value> {
        let name = &tool_call.function.name;
        let arguments: Value = serde_json::from_str(&tool_call.function.arguments)?;
        
        let tool = available_tools.iter().find(|t| t.name() == name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;
            
        tool.execute(arguments).await
    }
}
