use crate::config::DISCORD_EMBED_LIMIT;
use crate::context::ConversationContext;
use crate::llm::confirm::ToolConfirmationContext;
use crate::services::user_memory::UserMemoryService;
use crate::system_prompt;
use crate::{Context, Error};
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs,
};
use poise::serenity_prelude::{CreateEmbed, CreateEmbedFooter};
use tracing::{error, info, warn};

/// Chat with the all-in-one assistant
#[poise::command(slash_command)]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your message to the assistant"] message: String,
) -> Result<(), Error> {
    info!(
        "Chat command received from {} in channel {}: {}",
        ctx.author().name,
        ctx.channel_id(),
        message
    );
    ctx.defer().await?;

    let guild_id = ctx.guild_id().map(|id| id.get());
    let system_prompt = if let Some(gid) = guild_id {
        ctx.data()
            .db
            .run_blocking(move |db| db.get_guild_system_prompt(gid))
            .await?
            .unwrap_or_else(|| ctx.data().config.system_prompt.clone())
    } else {
        ctx.data().config.system_prompt.clone()
    };
    let confirm_timeout_secs = if let Some(gid) = guild_id {
        ctx.data()
            .db
            .run_blocking(move |db| db.get_guild_agent_confirm_timeout(gid))
            .await?
            .unwrap_or(ctx.data().config.agent_confirm_timeout_secs)
    } else {
        ctx.data().config.agent_confirm_timeout_secs
    };

    // Build messages with configurable system prompt
    let mut messages: Vec<ChatCompletionRequestMessage> =
        vec![ChatCompletionRequestSystemMessageArgs::default()
            .content(system_prompt)
            .build()?
            .into()];

    // Inject current date/time context
    if let Ok(datetime_msg) = ChatCompletionRequestSystemMessageArgs::default()
        .content(system_prompt::build_datetime_system_message())
        .build()
    {
        messages.push(datetime_msg.into());
    }

    let memory_service = UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
    let skip_memory = UserMemoryService::should_skip_memory(&message);
    let user_id = ctx.author().id.get();

    if skip_memory {
        if let Ok(msg) = ChatCompletionRequestSystemMessageArgs::default()
            .content("Temporary no-memory request: do not use or update user memory. Do not call get_user_memory.")
            .build()
        {
            messages.push(msg.into());
        }
    }

    if let Ok(meta_msg) = ChatCompletionRequestSystemMessageArgs::default()
        .content(format!(
            "User metadata: id={}, name={}",
            user_id,
            ctx.author().name
        ))
        .build()
    {
        messages.push(meta_msg.into());
    }

    let memory_record = if skip_memory {
        None
    } else {
        memory_service.get_user_memory_record(user_id).await?
    };
    let memory_enabled = memory_record.as_ref().is_some_and(|r| r.enabled);

    if !skip_memory {
        if let Ok(help_msg) = ChatCompletionRequestSystemMessageArgs::default()
            .content(
                "If you need the user's full memory profile, call get_user_memory with user_id.",
            )
            .build()
        {
            messages.push(help_msg.into());
        }
    }

    if let Some(record) = memory_record.as_ref().filter(|r| r.enabled) {
        let snippet = UserMemoryService::format_snippet(&record.summary, 600);
        if !snippet.is_empty() {
            if let Ok(msg) = ChatCompletionRequestUserMessageArgs::default()
                .content(snippet)
                .build()
            {
                messages.push(msg.into());
            }
        }
    }

    // Inject channel context (recent messages)
    let context_messages = ConversationContext::get_context_for_channel_async(
        ctx.data().cache.clone(),
        ctx.data().db.clone(),
        ctx.data().config.clone(),
        ctx.channel_id(),
        ctx.guild_id().map(|id| id.get()),
        Some(ctx.data().bot_id),
        None,
    );
    messages.extend(context_messages.await);

    // Add the current user message
    messages.push(
        ChatCompletionRequestUserMessageArgs::default()
            .content(format!("[{}]: {}", ctx.author().name, message.clone()))
            .build()?
            .into(),
    );

    let query_msg = ctx.say("Thinking...").await?;

    let agent = crate::llm::agent::Agent::new(ctx.data());
    let confirm_ctx = ToolConfirmationContext::new(
        ctx.serenity_context(),
        ctx.channel_id(),
        ctx.author().id,
        std::time::Duration::from_secs(confirm_timeout_secs),
    );
    let response = match agent.run_with_confirmation(confirm_ctx, messages, 10).await {
        Ok(r) => r,
        Err(e) => {
            error!(
                "Assistant error in /chat for channel {}: {}",
                ctx.channel_id(),
                e
            );
            format!("❌ Assistant Error: {}", e)
        }
    };

    // Handle long responses with embeds
    send_response(&ctx, &response).await?;
    info!(
        "Assistant response sent to {} in channel {}",
        ctx.author().name,
        ctx.channel_id()
    );

    // Attempt to delete the "Thinking..." message to clean up
    if let Ok(m) = query_msg.into_message().await {
        let _ = m.delete(ctx).await;
    }

    if !skip_memory && memory_enabled {
        let llm = ctx.data().llm_client.clone();
        let memory_service =
            UserMemoryService::new(ctx.data().db.clone(), ctx.data().cache.clone());
        let user_message = message.clone();
        let assistant_response = response.clone();
        tokio::spawn(async move {
            if let Err(e) = memory_service
                .auto_update_memory(llm, user_id, &user_message, &assistant_response)
                .await
            {
                warn!("Auto-update memory failed for user {}: {}", user_id, e);
            }
        });
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
) -> Result<Vec<poise::serenity_prelude::MessageId>, Error> {
    use poise::serenity_prelude::CreateMessage;

    let mut sent_ids = Vec::new();
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

        let sent = channel_id.send_message(http, message.embed(embed)).await?;
        sent_ids.push(sent.id);
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

            let sent = channel_id
                .send_message(&http, message.clone().embed(embed))
                .await?;
            sent_ids.push(sent.id);
        }
    }
    Ok(sent_ids)
}
