use async_openai::{
    config::OpenAIConfig,
    types::{CreateChatCompletionRequestArgs, ChatCompletionRequestMessage},
    Client,
};
use crate::config::Config;

pub struct LlmClient {
    chat_client: Client<OpenAIConfig>,
    embedding_client: Client<OpenAIConfig>,
    chat_model: String,
    embedding_model: String,
}

impl LlmClient {
    pub fn new(config: &Config) -> Self {
        let mut chat_config = OpenAIConfig::new()
            .with_api_base(&config.llama_url);
        
        if let Some(key) = &config.llama_api_key {
            chat_config = chat_config.with_api_key(key);
        } else {
            chat_config = chat_config.with_api_key("unused");
        }
            
        let mut embedding_config = OpenAIConfig::new()
            .with_api_base(&config.embedding_url);

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
        }
    }

    pub async fn chat(
        &self, 
        messages: Vec<ChatCompletionRequestMessage>
    ) -> anyhow::Result<String> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.chat_model)
            .messages(messages)
            .build()?;

        let response = self.chat_client.chat().create(request).await?;
        
        let content = response.choices.first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_else(|| "No response from LLM".to_string());

        Ok(content)
    }

    pub async fn get_embeddings(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        use async_openai::types::CreateEmbeddingRequestArgs;

        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.embedding_model)
            .input(text)
            .build()?;

        let response = self.embedding_client.embeddings().create(request).await?;
        let embedding = response.data.first()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))?
            .embedding.clone();

        Ok(embedding)
    }
}
