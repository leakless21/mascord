use crate::commands::chat::send_embed_reply;
use crate::context::ConversationContext;
use crate::discord_text::{extract_message_text, strip_bot_mentions};
use crate::llm::confirm::ToolConfirmationContext;
use crate::services::user_memory::UserMemoryService;
use crate::system_prompt;
use crate::{Data, Error};
use async_openai::types::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
};
use poise::serenity_prelude as serenity;
use tracing::{error, info, warn};

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
            .run_blocking(move |db| db.get_guild_system_prompt(gid))
            .await?
            .unwrap_or_else(|| data.config.system_prompt.clone())
    } else {
        data.config.system_prompt.clone()
    };
    let confirm_timeout_secs = if let Some(gid) = guild_id {
        data.db
            .run_blocking(move |db| db.get_guild_agent_confirm_timeout(gid))
            .await?
            .unwrap_or(data.config.agent_confirm_timeout_secs)
    } else {
        data.config.agent_confirm_timeout_secs
    };

    let skip_memory = UserMemoryService::should_skip_memory(&prompt);
    let user_id = new_message.author.id.get();

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

    if skip_memory {
        if let Ok(msg) = ChatCompletionRequestSystemMessageArgs::default()
            .content("Temporary no-memory request: do not use or update user memory. Do not call get_user_memory.")
            .build()
        {
            messages.push(msg.into());
        }
    }

    let memory_service = UserMemoryService::new(data.db.clone(), data.cache.clone());
    if let Ok(meta_msg) = ChatCompletionRequestSystemMessageArgs::default()
        .content(format!(
            "User metadata: id={}, name={}",
            user_id, new_message.author.name
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

    // Inject channel context (recent messages), excluding this message (already in cache).
    let context_messages = ConversationContext::get_context_for_channel_async(
        data.cache.clone(),
        data.db.clone(),
        data.config.clone(),
        new_message.channel_id,
        guild_id,
        Some(data.bot_id),
        Some(new_message.id.get()),
    );
    messages.extend(context_messages.await);

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

    if !skip_memory && memory_enabled {
        let llm = data.llm_client.clone();
        let memory_service = UserMemoryService::new(data.db.clone(), data.cache.clone());
        let user_message = prompt.clone();
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

    // Cache the assistant response as a synthetic message so it can appear in future context windows.
    let channel_id_str = new_message.channel_id.to_string();
    let tracking_enabled = data
        .db
        .run_blocking(move |db| db.is_channel_tracking_enabled(&channel_id_str))
        .await?;
    if tracking_enabled {
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
