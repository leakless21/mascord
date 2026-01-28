use crate::{Context, Error};
use crate::config::{DISCORD_MESSAGE_LIMIT, DISCORD_EMBED_LIMIT};
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs, 
    ChatCompletionRequestUserMessageArgs, 
    ChatCompletionRequestMessage
};
use poise::serenity_prelude::{CreateEmbed, CreateEmbedFooter};

/// Chat with the all-in-one assistant
#[poise::command(slash_command)]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your message to the assistant"]
    message: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    let config = &ctx.data().config;
    let llm_client = &ctx.data().llm_client;
    
    // Build messages with configurable system prompt
    let messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(config.system_prompt.clone())
            .build()?
            .into(),
        ChatCompletionRequestUserMessageArgs::default()
            .content(message.clone())
            .build()?
            .into(),
    ];

    let response = match llm_client.chat(messages).await {
        Ok(r) => r,
        Err(e) => {
            ctx.say(format!("❌ LLM Error: {}", e)).await?;
            return Ok(());
        }
    };
    
    // Handle long responses with embeds
    send_response(&ctx, &response).await?;
    
    Ok(())
}

/// Send response, using embeds for long messages
async fn send_response(ctx: &Context<'_>, content: &str) -> Result<(), Error> {
    if content.len() <= DISCORD_MESSAGE_LIMIT {
        ctx.say(content).await?;
    } else if content.len() <= DISCORD_EMBED_LIMIT {
        // Use embed for longer content (up to 4096 chars)
        let embed = CreateEmbed::new()
            .title("🤖 Mascord Response")
            .description(content)
            .color(0x5865F2)
            .footer(CreateEmbedFooter::new("Powered by llama.cpp"));
        
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
    } else {
        // Split into multiple embeds if extremely long
        let chunks: Vec<&str> = content
            .as_bytes()
            .chunks(DISCORD_EMBED_LIMIT - 100)
            .map(|c| std::str::from_utf8(c).unwrap_or("..."))
            .collect();
        
        for (i, chunk) in chunks.iter().enumerate() {
            let embed = CreateEmbed::new()
                .title(format!("🤖 Response (Part {}/{})", i + 1, chunks.len()))
                .description(*chunk)
                .color(0x5865F2);
            
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
        }
    }
    Ok(())
}
