use crate::config::Config;
use anyhow::Context as _;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionTool, ChatCompletionToolChoiceOption,
        ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionObject,
    },
    Client,
};
use serde_json::Value;
use std::time::Instant;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct LlmClient {
    chat_client: Client<OpenAIConfig>,
    embedding_client: Client<OpenAIConfig>,
    chat_model: String,
    embedding_model: String,
    chat_timeout: u64,
    embedding_timeout: u64,
}

impl LlmClient {
    pub fn new(config: &Config) -> Self {
        let mut chat_config = OpenAIConfig::new().with_api_base(&config.llama_url);

        if let Some(key) = &config.llama_api_key {
            chat_config = chat_config.with_api_key(key);
        } else {
            chat_config = chat_config.with_api_key("unused");
        }

        let mut embedding_config = OpenAIConfig::new().with_api_base(&config.embedding_url);

        if let Some(key) = &config.embedding_api_key {
            embedding_config = embedding_config.with_api_key(key);
        } else {
            embedding_config = embedding_config.with_api_key("unused");
        }

        Self {
            chat_client: Client::with_config(chat_config),
            embedding_client: Client::with_config(embedding_config),
            chat_model: config.llama_model.clone(),
            embedding_model: config.embedding_model.clone(),
            chat_timeout: config.llm_timeout_secs,
            embedding_timeout: config.embedding_timeout_secs,
        }
    }

    pub async fn chat_with_tools(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<Value>>,
    ) -> anyhow::Result<async_openai::types::CreateChatCompletionResponse> {
        self.chat_with_tools_mode(messages, tools, ChatCompletionToolChoiceOption::Auto)
            .await
    }

    pub async fn chat_with_tools_required(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<Value>>,
    ) -> anyhow::Result<async_openai::types::CreateChatCompletionResponse> {
        self.chat_with_tools_mode(messages, tools, ChatCompletionToolChoiceOption::Required)
            .await
    }

    async fn chat_with_tools_mode(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<Value>>,
        tool_mode: ChatCompletionToolChoiceOption,
    ) -> anyhow::Result<async_openai::types::CreateChatCompletionResponse> {
        use tokio::time::{timeout, Duration};
        let llm_timeout = Duration::from_secs(self.chat_timeout);

        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&self.chat_model).messages(messages);

        if let Some(tools_vec) = tools {
            let n_tools_in = tools_vec.len();
            let openai_tools: Vec<ChatCompletionTool> = tools_vec
                .into_iter()
                .filter_map(|t| {
                    let func =
                        serde_json::from_value::<FunctionObject>(t["function"].clone()).ok()?;
                    Some(ChatCompletionTool {
                        r#type: ChatCompletionToolType::Function,
                        function: func,
                    })
                })
                .collect();

            if n_tools_in > 0 && openai_tools.is_empty() {
                error!(
                    "All {} tool definition(s) failed JSON parse; fail-fast (check ToolRegistry schemas)",
                    n_tools_in
                );
                return Err(anyhow::anyhow!(
                    "Internal error: no valid tool definitions ({} entries failed to parse)",
                    n_tools_in
                ));
            }

            if !openai_tools.is_empty() {
                request_builder.tools(openai_tools);
                // Be explicit for providers that are strict about tool selection semantics.
                request_builder.tool_choice(tool_mode);
            }
        }

        let request = request_builder
            .build()
            .context("failed to build chat completion request (check model / messages)")?;

        debug!(
            "Sending chat request to {} (timeout: {}s)...",
            self.chat_model, self.chat_timeout
        );
        let start = Instant::now();
        let response = timeout(llm_timeout, self.chat_client.chat().create(request))
            .await
            .map_err(|_| {
                error!("LLM request timed out after {}s", llm_timeout.as_secs());
                anyhow::anyhow!("LLM request timed out after {}s", llm_timeout.as_secs())
            })?
            .map_err(|e| {
                error!(error = %e, model = %self.chat_model, "LLM chat API error");
                anyhow::anyhow!("LLM API error: {} (model: {})", e, self.chat_model)
            })?;

        let duration = start.elapsed();
        info!(
            "LLM chat request to {} completed in {:?}",
            self.chat_model, duration
        );

        if response.choices.is_empty() {
            error!("LLM returned zero choices (model: {})", self.chat_model);
            return Err(anyhow::anyhow!(
                "LLM returned an empty response (no choices)"
            ));
        }

        Ok(response)
    }

    pub async fn chat(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> anyhow::Result<String> {
        let response = self.chat_with_tools(messages, None).await?;

        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "LLM returned no message content (model: {})",
                    self.chat_model
                )
            })?;

        Ok(content)
    }

    /// Simple string completion for internal tasks (summarization, etc)
    pub async fn completion(&self, prompt: &str) -> anyhow::Result<String> {
        use async_openai::types::ChatCompletionRequestUserMessageArgs;

        let message = ChatCompletionRequestUserMessageArgs::default()
            .content(prompt)
            .build()?
            .into();

        self.chat(vec![message]).await
    }

    pub async fn get_embeddings(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        use async_openai::types::CreateEmbeddingRequestArgs;
        use tokio::time::{timeout, Duration};

        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.embedding_model)
            .input(text)
            .build()?;

        debug!("Sending embedding request to {}...", self.embedding_model);
        let start = Instant::now();
        let response = timeout(
            Duration::from_secs(self.embedding_timeout),
            self.embedding_client.embeddings().create(request),
        )
        .await
        .map_err(|_| {
            error!(
                "Embedding request timed out after {}s",
                self.embedding_timeout
            );
            anyhow::anyhow!(
                "Embedding request timed out after {}s",
                self.embedding_timeout
            )
        })?
        .map_err(|e| {
            error!(error = %e, model = %self.embedding_model, "Embedding API error");
            anyhow::anyhow!(
                "Embedding API error: {} (model: {})",
                e,
                self.embedding_model
            )
        })?;

        let duration = start.elapsed();
        info!(
            "Embedding request to {} completed in {:?}",
            self.embedding_model, duration
        );

        let embedding = response
            .data
            .first()
            .ok_or_else(|| {
                error!("No embedding returned from API");
                anyhow::anyhow!("No embedding returned")
            })?
            .embedding
            .clone();

        Ok(embedding)
    }
}
