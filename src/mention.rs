use crate::commands::chat::send_embed_reply;
use crate::context::ConversationContext;
use crate::discord_text::{extract_message_text, strip_bot_mentions};
use crate::llm::confirm::ToolConfirmationContext;
use crate::{Data, Error};
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
};
use poise::serenity_prelude as serenity;
use tracing::{error, info};

/// Handle a message where the bot is mentioned/tagged.
pub async fn handle_mention(
    ctx: &serenity::Context,
    new_message: &serenity::Message,
    data: &Data,
) -> Result<(), Error> {
    info!(
        "Handling mention from {} in channel {}: {}",
        new_message.author.name, new_message.channel_id, new_message.content
    );

    let prompt = strip_bot_mentions(&new_message.content, data.bot_id);
    if prompt.trim().is_empty() {
        // Avoid noisy replies when someone only pings the bot.
        return Ok(());
    }

    let guild_id = new_message.guild_id.map(|id| id.get());
    let system_prompt = if let Some(gid) = guild_id {
        data.db
            .get_guild_system_prompt(gid)?
            .unwrap_or_else(|| data.config.system_prompt.clone())
    } else {
        data.config.system_prompt.clone()
    };
    let confirm_timeout_secs = if let Some(gid) = guild_id {
        data.db
            .get_guild_agent_confirm_timeout(gid)?
            .unwrap_or(data.config.agent_confirm_timeout_secs)
    } else {
        data.config.agent_confirm_timeout_secs
    };

    // Build messages with configurable system prompt
    let mut messages: Vec<ChatCompletionRequestMessage> =
        vec![ChatCompletionRequestSystemMessageArgs::default()
            .content(system_prompt)
            .build()?
            .into()];

    // Inject channel context (recent messages), excluding this message (already in cache).
    let context_messages = ConversationContext::get_context_for_channel(
        &data.cache,
        &data.db,
        &data.config,
        new_message.channel_id,
        new_message.guild_id.map(|id| id.get()),
        Some(data.bot_id),
        Some(new_message.id.get()),
    );
    messages.extend(context_messages);

    // If this mention is itself a reply, include the referenced message explicitly.
    if let Some(referenced) = new_message.referenced_message.as_deref() {
        let referenced_text = extract_message_text(referenced);
        if !referenced_text.trim().is_empty() {
            let referenced_msg: ChatCompletionRequestMessage =
                if referenced.author.id.get() == data.bot_id {
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .content(referenced_text)
                        .build()?
                        .into()
                } else {
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(format!("[{}]: {}", referenced.author.name, referenced_text))
                        .build()?
                        .into()
                };
            messages.push(referenced_msg);
        }
    }

    // Add the current user message (with mention stripped)
    messages.push(
        ChatCompletionRequestUserMessageArgs::default()
            .content(format!("[{}]: {}", new_message.author.name, prompt))
            .build()?
            .into(),
    );

    let typing = new_message.channel_id.start_typing(&ctx.http);

    let agent = crate::llm::agent::Agent::new(data);
    let confirm_ctx = ToolConfirmationContext::new(
        ctx,
        new_message.channel_id,
        new_message.author.id,
        std::time::Duration::from_secs(confirm_timeout_secs),
    );

    let response = match agent.run_with_confirmation(confirm_ctx, messages, 10).await {
        Ok(r) => r,
        Err(e) => {
            error!("Agent error handling mention: {}", e);
            format!("❌ Assistant Error: {}", e)
        }
    };

    drop(typing);

    // Reply directly to the mention message
    let sent_ids = send_embed_reply(
        &ctx.http,
        new_message.channel_id,
        &response,
        Some(new_message.id),
    )
    .await?;

    // Cache the assistant response as a synthetic message so it can appear in future context windows.
    if data
        .db
        .is_channel_tracking_enabled(&new_message.channel_id.to_string())?
    {
        let Some(sent_id) = sent_ids.first().copied() else {
            return Ok(());
        };

        let mut synthetic = serenity::Message::default();
        synthetic.id = sent_id;
        synthetic.channel_id = new_message.channel_id;
        synthetic.author = serenity::User::default();
        synthetic.author.id = serenity::UserId::new(data.bot_id);
        synthetic.author.name = "Mascord".to_string();
        synthetic.content = response.clone();
        synthetic.timestamp = serenity::Timestamp::now();
        data.cache.insert(synthetic);
    }

    Ok(())
}
