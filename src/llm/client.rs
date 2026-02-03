use crate::config::Config;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionTool, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, FunctionObject,
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
        use tokio::time::{timeout, Duration};
        let llm_timeout = Duration::from_secs(self.chat_timeout);

        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&self.chat_model).messages(messages);

        if let Some(tools_vec) = tools {
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

            if !openai_tools.is_empty() {
                request_builder.tools(openai_tools);
            }
        }

        let request = request_builder.build()?;

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
            })??;

        let duration = start.elapsed();
        info!(
            "LLM chat request to {} completed in {:?}",
            self.chat_model, duration
        );

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
            .unwrap_or_else(|| "No response from LLM".to_string());

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
        })??;

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
