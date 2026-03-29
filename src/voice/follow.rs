//! Move the voice connection when the last human listener leaves for another channel in the guild.

use crate::voice::alone::{count_humans_in_voice_channel, serenity_channel_from_songbird};
use crate::Data;
use poise::serenity_prelude::Context;
use poise::serenity_prelude as serenity;
use tracing::info;

/// If enabled, move the bot to `new.channel_id` when a user was the last human in the bot’s
/// previous channel (common “follow me” behavior).
pub async fn handle_voice_follow_move(
    ctx: &Context,
    data: &Data,
    old: Option<&serenity::model::voice::VoiceState>,
    new: &serenity::model::voice::VoiceState,
) {
    if !data.config.voice_follow_user_move {
        return;
    }

    let Some(guild_id) = new.guild_id else {
        return;
    };
    let guild_id_u64 = guild_id.get();

    let Some(manager) = songbird::get(ctx).await else {
        return;
    };
    let Some(handler_lock) = manager.get(guild_id) else {
        return;
    };

    let bot_channel_sb = {
        let h = handler_lock.lock().await;
        h.current_channel()
    };
    let Some(bot_channel_sb) = bot_channel_sb else {
        return;
    };
    let bot_channel = serenity_channel_from_songbird(bot_channel_sb);

    let Some(old_vs) = old else {
        return;
    };
    let Some(old_ch) = old_vs.channel_id else {
        return;
    };
    let Some(new_ch) = new.channel_id else {
        return;
    };
    if old_ch == new_ch || old_ch != bot_channel {
        return;
    }

    let is_bot = new
        .member
        .as_ref()
        .map(|m| m.user.bot)
        .or_else(|| ctx.cache.user(new.user_id).map(|u| u.bot))
        .unwrap_or(false);
    if is_bot {
        return;
    }

    let should_follow = {
        let Some(guild) = ctx.cache.guild(guild_id) else {
            return;
        };
        count_humans_in_voice_channel(&ctx.cache, &guild, old_ch) == 0
    };
    if !should_follow {
        return;
    }

    data.music.cancel_alone_leave_task(guild_id_u64);
    match manager.join(guild_id, new_ch).await {
        Ok(_) => info!(
            "Followed listener to voice channel {} in guild {}",
            new_ch, guild_id
        ),
        Err(e) => tracing::warn!(
            "Failed to follow user to voice channel {} in guild {}: {}",
            new_ch,
            guild_id,
            e
        ),
    }
}
