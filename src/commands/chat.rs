use crate::{Context, Error};
use crate::config::DISCORD_EMBED_LIMIT;
use crate::context::ConversationContext;
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs, 
    ChatCompletionRequestUserMessageArgs, 
    ChatCompletionRequestMessage
};
use poise::serenity_prelude::{CreateEmbed, CreateEmbedFooter};
use tracing::{info, error};

/// Chat with the all-in-one assistant
#[poise::command(slash_command)]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your message to the assistant"]
    message: String,
) -> Result<(), Error> {
    info!("Chat command received from {} in channel {}: {}", ctx.author().name, ctx.channel_id(), message);
    ctx.defer().await?;

    // Build messages with configurable system prompt
    let mut messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(ctx.data().config.system_prompt.clone())
            .build()?
            .into(),
    ];
    
    // Inject channel context (recent messages)
    let context_messages = ConversationContext::get_context_for_channel(
        &ctx.data().cache,
        &ctx.data().db,
        &ctx.data().config,
        ctx.channel_id(),
        ctx.guild_id().map(|id| id.get()),
        Some(ctx.data().bot_id),
    );
    messages.extend(context_messages);
    
    // Add the current user message
    messages.push(
        ChatCompletionRequestUserMessageArgs::default()
            .content(format!("[{}]: {}", ctx.author().name, message.clone()))
            .build()?
            .into(),
    );

    let query_msg = ctx.say("Thinking...").await?;

    let agent = crate::llm::agent::Agent::new(ctx.data());
    let response = match agent.run(messages, 10).await {
        Ok(r) => r,
        Err(e) => {
            error!("Assistant error in /chat for channel {}: {}", ctx.channel_id(), e);
            format!("❌ Assistant Error: {}", e)
        }
    };

    // Handle long responses with embeds
    send_response(&ctx, &response).await?;
    info!("Assistant response sent to {} in channel {}", ctx.author().name, ctx.channel_id());
    
    // Attempt to delete the "Thinking..." message to clean up
    if let Ok(m) = query_msg.into_message().await {
        let _ = m.delete(ctx).await;
    }

    Ok(())
}

/// Send response, always using embeds to avoid plain text limits
pub async fn send_response(ctx: &Context<'_>, content: &str) -> Result<(), Error> {
    if content.len() <= DISCORD_EMBED_LIMIT {
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

/// Generic helper to send an embed response to a specific channel
pub async fn send_embed_reply(
    http: impl poise::serenity_prelude::CacheHttp,
    channel_id: poise::serenity_prelude::ChannelId,
    content: &str,
    reply_to: Option<poise::serenity_prelude::MessageId>,
) -> Result<(), Error> {
    use poise::serenity_prelude::CreateMessage;

    let mut message = CreateMessage::new();
    if let Some(id) = reply_to {
        message = message.reference_message((channel_id, id));
    }

    if content.len() <= DISCORD_EMBED_LIMIT {
        let embed = CreateEmbed::new()
            .title("🤖 Mascord Response")
            .description(content)
            .color(0x5865F2)
            .footer(CreateEmbedFooter::new("Powered by llama.cpp"));
        
        channel_id.send_message(http, message.embed(embed)).await?;
    } else {
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
            
            channel_id.send_message(&http, message.clone().embed(embed)).await?;
        }
    }
    Ok(())
}
