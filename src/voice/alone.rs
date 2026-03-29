//! Leave voice when no human listeners remain in the bot's channel (`VoiceStateUpdate`).

use crate::Data;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::{Cache, ChannelId, Context};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub(crate) fn serenity_channel_from_songbird(id: songbird::id::ChannelId) -> ChannelId {
    ChannelId::new(id.0.get())
}

/// Count non-bot users connected to `channel_id` in `guild` (uses cache for users missing from `guild.members`).
pub fn count_humans_in_voice_channel(
    cache: &Cache,
    guild: &serenity::model::guild::Guild,
    channel_id: ChannelId,
) -> usize {
    let mut count = 0usize;
    for (uid, vs) in guild.voice_states.iter() {
        if vs.channel_id != Some(channel_id) {
            continue;
        }
        let is_bot = guild
            .members
            .get(uid)
            .map(|m| m.user.bot)
            .or_else(|| cache.user(*uid).map(|u| u.bot))
            .unwrap_or(false);
        if !is_bot {
            count += 1;
        }
    }
    count
}

/// After `VoiceStateUpdate`, disconnect if the bot is in voice and no humans remain in that channel.
pub async fn handle_voice_alone_disconnect(
    ctx: &Context,
    data: &Data,
    new_state: &serenity::model::voice::VoiceState,
) {
    let Some(guild_id) = new_state.guild_id else {
        return;
    };
    let guild_id_u64 = guild_id.get();

    let Some(manager) = songbird::get(ctx).await else {
        return;
    };
    if manager.get(guild_id).is_none() {
        return;
    }

    let timeout_secs = match data
        .db
        .run_blocking(move |db| db.get_guild_voice_alone_timeout(guild_id_u64))
        .await
    {
        Ok(v) => v.unwrap_or(data.config.voice_alone_timeout_secs),
        Err(e) => {
            warn!("voice alone: failed to read guild override: {}", e);
            data.config.voice_alone_timeout_secs
        }
    };
    if timeout_secs == 0 {
        return;
    }

    let Some(handler_lock) = manager.get(guild_id) else {
        return;
    };
    let bot_channel = {
        let h = handler_lock.lock().await;
        h.current_channel()
    };
    let Some(bot_channel) = bot_channel else {
        return;
    };
    let bot_channel = serenity_channel_from_songbird(bot_channel);

    let Some(guild) = ctx.cache.guild(guild_id) else {
        return;
    };

    let humans = count_humans_in_voice_channel(&ctx.cache, &guild, bot_channel);
    if humans > 0 {
        data.music.cancel_alone_leave_task(guild_id_u64);
        return;
    }

    data.music.cancel_alone_leave_task(guild_id_u64);

    let cache = ctx.cache.clone();
    let manager2 = manager.clone();
    let music = Arc::clone(&data.music);
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(timeout_secs)).await;

        let Some(handler_lock) = manager2.get(guild_id) else {
            music.take_alone_leave_task(guild_id_u64);
            return;
        };
        let bot_channel = {
            let h = handler_lock.lock().await;
            h.current_channel()
        };
        let Some(bot_channel) = bot_channel else {
            music.take_alone_leave_task(guild_id_u64);
            return;
        };
        let bot_channel = serenity_channel_from_songbird(bot_channel);
        let humans = {
            let Some(g) = cache.guild(guild_id) else {
                music.take_alone_leave_task(guild_id_u64);
                return;
            };
            count_humans_in_voice_channel(&cache, &g, bot_channel)
        };
        if humans > 0 {
            info!(
                "Alone-leave timer aborted in guild {} — listeners present.",
                guild_id
            );
            music.take_alone_leave_task(guild_id_u64);
            return;
        }

        info!(
            "No listeners left in voice channel in guild {} — leaving.",
            guild_id
        );
        music.clear_voice_hooks(guild_id_u64);
        let _ = manager2.remove(guild_id).await;
        music.take_alone_leave_task(guild_id_u64);
    });

    data.music.replace_alone_leave_task(guild_id_u64, handle);
}
