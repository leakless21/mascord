//! Voice music commands (yt-dlp / YouTube and other yt-dlp sources).

pub(crate) mod format;
mod handlers;
pub mod playback;
pub mod lavalink;
pub(crate) mod queue_ops;
pub mod state;

use format::format_hms;

pub use state::{LoopMode, MusicState};

use crate::services::music_ops::{
    build_queue_page, build_queue_page_lavalink, lavalink_queue_is_empty, music_clear, music_join,
    music_leave, music_loop, music_lyrics, music_move_track, music_now_playing, music_pause,
    music_play, music_remove, music_resume, music_shuffle, music_skip, music_skip_button,
    music_stop_and_leave, music_toggle_pause, music_volume, now_playing_embed,
    play_controls_refresh_embed, play_embed, LoopModeArg,
};
use crate::{Context, Error};
use poise::serenity_prelude::{
    ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter,
    CreateInteractionResponse, EditInteractionResponse, EditMessage,
};

/// Wall-clock budget for `/play` and `/queue` component sessions (avoids indefinite extension
/// when interactions keep arriving before each per-wait timeout).
const MUSIC_COMPONENT_SESSION: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Join a voice channel
#[poise::command(
    slash_command,
    guild_only,
    required_bot_permissions = "CONNECT | SPEAK"
)]
pub async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let op = music_join(
        ctx.serenity_context(),
        ctx.data(),
        guild_id,
        ctx.author().id,
    )
    .await
    .map_err(|e| -> Error { e.into() })?;
    ctx.say(op.discord_message()).await?;
    Ok(())
}

/// Play a URL or search query (Lavalink: use `ytmsearch:` / `spsearch:` or `LAVALINK_SEARCH_PREFIX`).
#[poise::command(
    slash_command,
    guild_only,
    required_bot_permissions = "CONNECT | SPEAK"
)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL, search query, or prefixed query (e.g. ytmsearch:artist song)"] query: String,
    #[description = "Expand YouTube playlists (URLs only, max 50 tracks)"] playlist: Option<bool>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let expand = playlist.unwrap_or(false);
    let op = music_play(
        ctx.serenity_context(),
        ctx.data(),
        guild_id,
        ctx.author().id,
        query,
        expand,
    )
    .await
    .map_err(|e| -> Error { e.into() })?;
    let embed = play_embed(&op).ok_or_else(|| -> Error { "internal: play embed".into() })?;

    let row = play_message_control_row();

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(embed)
                .components(vec![row.clone()]),
        )
        .await?;
    let mut message = reply.into_message().await?;
    let deadline = std::time::Instant::now() + MUSIC_COMPONENT_SESSION;

    loop {
        let now = std::time::Instant::now();
        if now >= deadline {
            let _ = message
                .edit(
                    ctx.serenity_context(),
                    EditMessage::new().components(Vec::new()),
                )
                .await;
            return Ok(());
        }
        let wait = deadline.saturating_duration_since(now);

        let Some(interaction) = message.await_component_interaction(ctx).timeout(wait).await else {
            let _ = message
                .edit(
                    ctx.serenity_context(),
                    EditMessage::new().components(Vec::new()),
                )
                .await;
            return Ok(());
        };
        let _ = interaction
            .create_response(ctx.serenity_context(), CreateInteractionResponse::Acknowledge)
            .await;

        let id = interaction.data.custom_id.as_str();
        if id == "stop" {
            let _ = music_stop_and_leave(ctx.serenity_context(), ctx.data(), guild_id).await;
            let _ = interaction
                .edit_response(
                    ctx.serenity_context(),
                    EditInteractionResponse::new()
                        .content("Stopped playback and left channel.")
                        .components(vec![])
                        .embeds(vec![]),
                )
                .await;
            return Ok(());
        }
        if id == "pause" {
            let _ = music_toggle_pause(ctx.serenity_context(), ctx.data(), guild_id).await;
        } else if id == "skip" {
            let _ = music_skip_button(ctx.serenity_context(), ctx.data(), guild_id).await;
        } else {
            continue;
        }

        if let Some(new_embed) =
            play_controls_refresh_embed(ctx.serenity_context(), ctx.data(), guild_id).await
        {
            let _ = interaction
                .edit_response(
                    ctx.serenity_context(),
                    EditInteractionResponse::new()
                        .embed(new_embed)
                        .components(vec![row.clone()]),
                )
                .await;
        } else {
            let _ = interaction
                .edit_response(
                    ctx.serenity_context(),
                    EditInteractionResponse::new()
                        .content("📭 Queue is empty.")
                        .components(vec![])
                        .embeds(vec![]),
                )
                .await;
            return Ok(());
        }
    }
}

fn play_message_control_row() -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new("pause")
            .emoji('⏯')
            .label("Pause")
            .style(ButtonStyle::Primary),
        CreateButton::new("skip")
            .emoji('⏭')
            .label("Skip")
            .style(ButtonStyle::Success),
        CreateButton::new("stop")
            .emoji('⏹')
            .label("Stop")
            .style(ButtonStyle::Danger),
    ])
}

/// Skip the current song
#[poise::command(slash_command, guild_only)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    match music_skip(ctx.serenity_context(), ctx.data(), guild_id).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Stop playback and leave
#[poise::command(slash_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    match music_leave(ctx.serenity_context(), ctx.data(), guild_id).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Pause playback
#[poise::command(slash_command, guild_only)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_pause(ctx.serenity_context(), ctx.data(), guild_id).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Resume playback
#[poise::command(slash_command, guild_only)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_resume(ctx.serenity_context(), ctx.data(), guild_id).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Set playback volume (0–200, applied to current and future tracks in this session)
#[poise::command(slash_command, guild_only)]
pub async fn volume(
    ctx: Context<'_>,
    #[description = "Volume 0–200 (100 = default)"] percent: u8,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_volume(ctx.serenity_context(), ctx.data(), guild_id, percent).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Show what is playing
#[poise::command(slash_command, guild_only, rename = "nowplaying")]
pub async fn now_playing_cmd(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_now_playing(ctx.serenity_context(), ctx.data(), guild_id).await {
        Ok(op) => {
            let embed = now_playing_embed(&op)
                .ok_or_else(|| -> Error { "internal: now playing embed".into() })?;
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Lyrics for the current track (Lavalink + LavaLyrics on the node)
#[poise::command(slash_command, guild_only)]
pub async fn lyrics(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_lyrics(ctx.data(), guild_id).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, poise::ChoiceParameter)]
pub enum LoopSetting {
    #[name = "off"]
    Off,
    #[name = "track"]
    Track,
    #[name = "queue"]
    Queue,
}

/// Loop the current track, the whole queue, or off
#[poise::command(slash_command, guild_only, rename = "loop")]
pub async fn loop_cmd(
    ctx: Context<'_>,
    #[description = "Loop mode"] mode: LoopSetting,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    let mode_arg = match mode {
        LoopSetting::Off => LoopModeArg::Off,
        LoopSetting::Track => LoopModeArg::Track,
        LoopSetting::Queue => LoopModeArg::Queue,
    };
    match music_loop(ctx.serenity_context(), ctx.data(), guild_id, mode_arg).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Clear upcoming tracks (keeps the current song)
#[poise::command(slash_command, guild_only)]
pub async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_clear(ctx.serenity_context(), ctx.data(), guild_id).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Shuffle upcoming tracks (not the current song)
#[poise::command(slash_command, guild_only)]
pub async fn shuffle(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_shuffle(ctx.serenity_context(), ctx.data(), guild_id).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Remove a track by queue position (1 = now playing, 2 = next, …)
#[poise::command(slash_command, guild_only)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Position in queue (1 = now playing)"] position: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_remove(ctx.serenity_context(), ctx.data(), guild_id, position).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Move a track within the queue (1 = now playing)
#[poise::command(slash_command, guild_only)]
pub async fn move_track(
    ctx: Context<'_>,
    #[description = "From position (1 = now playing)"] from: u32,
    #[description = "To position"] to: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    match music_move_track(ctx.serenity_context(), ctx.data(), guild_id, from, to).await {
        Ok(op) => {
            ctx.say(op.discord_message()).await?;
        }
        Err(e) => {
            ctx.say(e).await?;
        }
    }
    Ok(())
}

/// Show the current queue
#[poise::command(slash_command, guild_only)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird Voice client not initialized")?;

    if manager.get(guild_id).is_some() {
        let mut page = 0usize;
        let items_per_page = 10;

        let built = if ctx.data().lavalink.is_some() {
            if lavalink_queue_is_empty(ctx.data(), guild_id).await {
                ctx.say("📭 Queue is empty").await?;
                return Ok(());
            }
            build_queue_page_lavalink(ctx.data(), guild_id, page, items_per_page).await?
        } else {
            let handler_lock = manager.get(guild_id).unwrap();
            let handler = handler_lock.lock().await;
            let queue = handler.queue();
            let all = queue.current_queue();
            if all.is_empty() {
                ctx.say("📭 Queue is empty").await?;
                return Ok(());
            }
            let tracks = all.clone();
            drop(handler);
            let loop_mode = ctx.data().music.loop_mode(guild_id.get());
            build_queue_page(&tracks, page, items_per_page, loop_mode).await
        };

        let embed = CreateEmbed::new()
            .title("🎶 Music Queue")
            .description(format!(
                "{}{}\n\n**Total (est.):** `{}`",
                built.body,
                built.loop_note,
                format_hms(built.total_dur)
            ))
            .footer(CreateEmbedFooter::new(format!(
                "Page {}/{}",
                page + 1,
                built.total_pages
            )))
            .color(0x5865F2);

        let prev_btn = CreateButton::new("prev")
            .emoji('⬅')
            .style(ButtonStyle::Secondary)
            .disabled(page == 0);

        let next_btn = CreateButton::new("next")
            .emoji('➡')
            .style(ButtonStyle::Secondary)
            .disabled(page >= built.total_pages.saturating_sub(1));

        let pause_btn = CreateButton::new("pause")
            .emoji('⏯')
            .style(ButtonStyle::Primary);

        let skip_btn = CreateButton::new("skip")
            .emoji('⏭')
            .style(ButtonStyle::Success);

        let stop_btn = CreateButton::new("stop")
            .emoji('⏹')
            .style(ButtonStyle::Danger);

        let row = CreateActionRow::Buttons(vec![
            prev_btn,
            pause_btn.clone(),
            stop_btn.clone(),
            skip_btn.clone(),
            next_btn,
        ]);

        let reply = ctx
            .send(
                poise::CreateReply::default()
                    .embed(embed.clone())
                    .components(vec![row.clone()]),
            )
            .await?;
        let mut message = reply.into_message().await?;
        let deadline = std::time::Instant::now() + MUSIC_COMPONENT_SESSION;

        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                let _ = message
                    .edit(
                        ctx.serenity_context(),
                        EditMessage::new().components(Vec::new()),
                    )
                    .await;
                return Ok(());
            }
            let wait = deadline.saturating_duration_since(now);

            let Some(interaction) = message.await_component_interaction(ctx).timeout(wait).await
            else {
                let _ = message
                    .edit(
                        ctx.serenity_context(),
                        EditMessage::new().components(Vec::new()),
                    )
                    .await;
                return Ok(());
            };
            let _ = interaction
                .create_response(ctx.serenity_context(), CreateInteractionResponse::Acknowledge)
                .await;

            let custom_id = &interaction.data.custom_id;

            if ["pause", "skip", "stop"].contains(&custom_id.as_str()) {
                match custom_id.as_str() {
                    "pause" => {
                        let _ =
                            music_toggle_pause(ctx.serenity_context(), ctx.data(), guild_id).await;
                    }
                    "skip" => {
                        let _ = music_skip_button(ctx.serenity_context(), ctx.data(), guild_id).await;
                    }
                    "stop" => {
                        let _ = music_stop_and_leave(ctx.serenity_context(), ctx.data(), guild_id)
                            .await;
                        let _ = interaction
                            .edit_response(
                                ctx.serenity_context(),
                                EditInteractionResponse::new()
                                    .content("Stopped playback and left channel.")
                                    .components(vec![])
                                    .embeds(vec![]),
                            )
                            .await;
                        return Ok(());
                    }
                    _ => {}
                }
            } else if custom_id == "prev" {
                page = page.saturating_sub(1);
            } else if custom_id == "next" {
                page = page.saturating_add(1);
            }

            if ctx.data().lavalink.is_some() {
                if lavalink_queue_is_empty(ctx.data(), guild_id).await {
                    let _ = interaction
                        .edit_response(
                            ctx.serenity_context(),
                            EditInteractionResponse::new()
                                .content("Queue is now empty.")
                                .components(vec![])
                                .embeds(vec![]),
                        )
                        .await;
                    return Ok(());
                }
                let mut built =
                    build_queue_page_lavalink(ctx.data(), guild_id, page, items_per_page).await?;
                let total_pages = built.total_pages;
                if page >= total_pages {
                    page = total_pages.saturating_sub(1);
                }
                built = build_queue_page_lavalink(ctx.data(), guild_id, page, items_per_page).await?;

                let new_embed = CreateEmbed::new()
                    .title("🎶 Music Queue")
                    .description(format!(
                        "{}{}\n\n**Total (est.):** `{}`",
                        built.body,
                        built.loop_note,
                        format_hms(built.total_dur)
                    ))
                    .footer(CreateEmbedFooter::new(format!(
                        "Page {}/{}",
                        page + 1,
                        built.total_pages
                    )))
                    .color(0x5865F2);

                let prev_btn = CreateButton::new("prev")
                    .emoji('⬅')
                    .style(ButtonStyle::Secondary)
                    .disabled(page == 0);

                let next_btn = CreateButton::new("next")
                    .emoji('➡')
                    .style(ButtonStyle::Secondary)
                    .disabled(page >= built.total_pages.saturating_sub(1));

                let row = CreateActionRow::Buttons(vec![
                    prev_btn,
                    pause_btn.clone(),
                    stop_btn.clone(),
                    skip_btn.clone(),
                    next_btn,
                ]);

                let _ = interaction
                    .edit_response(
                        ctx.serenity_context(),
                        EditInteractionResponse::new()
                            .embed(new_embed)
                            .components(vec![row]),
                    )
                    .await;
            } else if let Some(handler_lock) = manager.get(guild_id) {
                let handler = handler_lock.lock().await;
                let queue = handler.queue();
                let all = queue.current_queue();
                if all.is_empty() {
                    let _ = interaction
                        .edit_response(
                            ctx.serenity_context(),
                            EditInteractionResponse::new()
                                .content("Queue is now empty.")
                                .components(vec![])
                                .embeds(vec![]),
                        )
                        .await;
                    return Ok(());
                }
                let upcoming_only_len = all.len().saturating_sub(1);
                let total_pages = if upcoming_only_len == 0 {
                    1usize
                } else {
                    (upcoming_only_len as f32 / items_per_page as f32).ceil() as usize
                };
                if page >= total_pages {
                    page = total_pages.saturating_sub(1);
                }
                let loop_mode = ctx.data().music.loop_mode(guild_id.get());
                let built = build_queue_page(&all, page, items_per_page, loop_mode).await;

                let new_embed = CreateEmbed::new()
                    .title("🎶 Music Queue")
                    .description(format!(
                        "{}{}\n\n**Total (est.):** `{}`",
                        built.body,
                        built.loop_note,
                        format_hms(built.total_dur)
                    ))
                    .footer(CreateEmbedFooter::new(format!(
                        "Page {}/{}",
                        page + 1,
                        built.total_pages
                    )))
                    .color(0x5865F2);

                let prev_btn = CreateButton::new("prev")
                    .emoji('⬅')
                    .style(ButtonStyle::Secondary)
                    .disabled(page == 0);

                let next_btn = CreateButton::new("next")
                    .emoji('➡')
                    .style(ButtonStyle::Secondary)
                    .disabled(page >= total_pages.saturating_sub(1));

                let row = CreateActionRow::Buttons(vec![
                    prev_btn,
                    pause_btn.clone(),
                    stop_btn.clone(),
                    skip_btn.clone(),
                    next_btn,
                ]);

                let _ = interaction
                    .edit_response(
                        ctx.serenity_context(),
                        EditInteractionResponse::new()
                            .embed(new_embed)
                            .components(vec![row]),
                    )
                    .await;
            }
        }
    } else {
        ctx.say("❌ I'm not in a voice channel").await?;
    }

    Ok(())
}
