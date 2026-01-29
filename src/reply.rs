use crate::{Data, Error};
use crate::context::ConversationContext;
use crate::commands::chat::send_embed_reply;
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs, 
    ChatCompletionRequestUserMessageArgs, 
    ChatCompletionRequestMessage
};
use poise::serenity_prelude as serenity;
use tracing::{info, error};

/// Handle a message that is a reply to the bot
pub async fn handle_reply(
    ctx: &serenity::Context,
    new_message: &serenity::Message,
    data: &Data,
) -> Result<(), Error> {
    info!("Handling reply from {} in channel {}: {}", new_message.author.name, new_message.channel_id, new_message.content);

    // Build messages with configurable system prompt
    let mut messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(data.config.system_prompt.clone())
            .build()?
            .into(),
    ];
    
    // Inject channel context (recent messages)
    let context_messages = ConversationContext::get_context_for_channel(
        &data.cache,
        &data.db,
        &data.config,
        new_message.channel_id,
        new_message.guild_id.map(|id| id.get()),
        Some(data.bot_id),
    );
    messages.extend(context_messages);
    
    // Add the current user message (the reply)
    messages.push(
        ChatCompletionRequestUserMessageArgs::default()
            .content(format!("[{}]: {}", new_message.author.name, new_message.content.clone()))
            .build()?
            .into(),
    );

    // Send a "Thinking..." message or use typing indicator
    let typing = new_message.channel_id.start_typing(&ctx.http);

    let agent = crate::llm::agent::Agent::new(data);
    let response = match agent.run(messages, 10).await {
        Ok(r) => r,
        Err(e) => {
            error!("Agent error handling reply: {}", e);
            format!("❌ Assistant Error: {}", e)
        }
    };

    // Stop typing indicator (implicit when it goes out of scope, but we can be explicit)
    drop(typing);

    // Handle long responses with embeds and reply to the user's message
    send_embed_reply(
        &ctx.http,
        new_message.channel_id,
        &response,
        Some(new_message.id)
    ).await?;
    
    info!("Assistant reply sent to {} in channel {}", new_message.author.name, new_message.channel_id);
    
    Ok(())
}
