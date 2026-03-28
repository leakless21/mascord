//! Voice music commands (yt-dlp / YouTube and other yt-dlp sources).

mod format;
mod handlers;
pub mod playback;
mod queue_ops;
pub mod state;

use format::{format_hms, format_pos_dur, truncate};

pub use state::{LoopMode, MusicState};

use crate::commands::music::playback::{
    enqueue_playback, join_voice_channel_serenity, EnqueueOpts, TrackUserData,
};
use crate::{Context, Error};
use poise::serenity_prelude::{
    ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter,
    CreateInteractionResponse, CreateInteractionResponseMessage,
};
use queue_ops::{move_by_adjacent_swaps, shuffle_queue_tail_keep_head};
use rand::thread_rng;
use songbird::tracks::PlayMode;
use std::time::Duration;
use tracing::info;

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
    let channel_id = join_voice_channel_serenity(
        ctx.serenity_context(),
        ctx.data(),
        guild_id,
        ctx.author().id,
        &ctx.data().music,
        ctx.data().http_client.clone(),
        ctx.data().config.youtube_cookies.clone(),
    )
    .await?;
    ctx.say(format!("🔊 Joined <#{}>", channel_id)).await?;
    Ok(())
}

/// Play audio (YouTube search, video URL, or yt-dlp-supported URL)
#[poise::command(
    slash_command,
    guild_only,
    required_bot_permissions = "CONNECT | SPEAK"
)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL or search query"] query: String,
    #[description = "Expand YouTube playlists (URLs only, max 50 tracks)"] playlist: Option<bool>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let expand = playlist.unwrap_or(false);
    let summary = enqueue_playback(
        ctx.serenity_context(),
        ctx.data(),
        guild_id,
        ctx.author().id,
        query.clone(),
        &ctx.data().music,
        EnqueueOpts {
            expand_playlist: expand,
            skip_voice_check: false,
        },
    )
    .await
    .map_err(|e| -> Error { e.into() })?;

    let title = summary.first_title.unwrap_or_else(|| query.clone());
    let embed = CreateEmbed::new()
        .title(if summary.added > 1 {
            format!("🎵 Added {} tracks", summary.added)
        } else {
            "🎵 Added to Queue".to_string()
        })
        .description(format!("**{}**", truncate(&title, 200)))
        .color(0x57F287);
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Skip the current song
#[poise::command(slash_command, guild_only)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird Voice client not initialized")?;

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();

        if queue.is_empty() {
            ctx.say("📭 Queue is empty").await?;
        } else {
            info!("Skip command in guild {}: skipping current song", guild_id);
            queue.skip()?;
            ctx.say("⏭️ Skipped current song").await?;
        }
    } else {
        ctx.say("❌ I'm not in a voice channel").await?;
    }

    Ok(())
}

/// Stop playback and leave
#[poise::command(slash_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a server")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird Voice client not initialized")?;

    if manager.get(guild_id).is_some() {
        info!(
            "Leave command: Removing voice handler for guild {}",
            guild_id
        );
        ctx.data().music.clear_voice_hooks(guild_id.get());
        manager.remove(guild_id).await?;
        ctx.say("👋 Left voice channel").await?;
    } else {
        ctx.say("❌ I'm not in a voice channel").await?;
    }

    Ok(())
}

/// Pause playback
#[poise::command(slash_command, guild_only)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.pause()?;
    ctx.say("⏸️ Paused").await?;
    Ok(())
}

/// Resume playback
#[poise::command(slash_command, guild_only)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.resume()?;
    ctx.say("▶️ Resumed").await?;
    Ok(())
}

/// Set playback volume (0–200, applied to current and future tracks in this session)
#[poise::command(slash_command, guild_only)]
pub async fn volume(
    ctx: Context<'_>,
    #[description = "Volume 0–200 (100 = default)"] percent: u8,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    let v = (percent as f32 / 100.0).clamp(0.0, 2.0);
    ctx.data().music.set_volume(guild_id.get(), v);
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    if let Some(lock) = manager.get(guild_id) {
        let handler = lock.lock().await;
        let q = handler.queue();
        for h in q.current_queue() {
            let _ = h.set_volume(v);
        }
    }
    ctx.say(format!("🔊 Volume set to **{}%**", percent))
        .await?;
    Ok(())
}

/// Show what is playing
#[poise::command(slash_command, guild_only, rename = "nowplaying")]
pub async fn now_playing_cmd(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let q = handler.queue();
    let Some(cur) = q.current() else {
        ctx.say("📭 Nothing playing").await?;
        return Ok(());
    };
    let data = cur.data::<TrackUserData>();
    let state = cur.get_info().await.ok();
    let pos = state.as_ref().map(|s| s.position);
    let dur = data.duration;
    let line = format!("**{}**\n{}", data.title, format_pos_dur(pos, dur));
    let mut e = CreateEmbed::new()
        .title("🎶 Now playing")
        .description(line)
        .color(0x5865F2);
    if let Some(ref t) = data.thumbnail {
        e = e.image(t);
    }
    ctx.send(poise::CreateReply::default().embed(e)).await?;
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
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    match mode {
        LoopSetting::Off => {
            ctx.data()
                .music
                .set_loop_mode(guild_id.get(), LoopMode::Off);
            if let Some(h) = queue.current() {
                let _ = h.disable_loop();
            }
            ctx.say("🔁 Loop **off**").await?;
        }
        LoopSetting::Track => {
            ctx.data()
                .music
                .set_loop_mode(guild_id.get(), LoopMode::Track);
            if let Some(h) = queue.current() {
                let _ = h.disable_loop();
                let _ = h.enable_loop();
            }
            ctx.say("🔁 Looping **current track**").await?;
        }
        LoopSetting::Queue => {
            let handles = queue.current_queue();
            let snapshot: Vec<(String, bool)> = handles
                .iter()
                .map(|h| {
                    let u = h.data::<TrackUserData>();
                    let is_url =
                        u.source.starts_with("http://") || u.source.starts_with("https://");
                    (u.source.clone(), is_url)
                })
                .collect();
            if snapshot.is_empty() {
                ctx.say("📭 Queue is empty — nothing to loop.").await?;
                return Ok(());
            }
            drop(handler);
            ctx.data()
                .music
                .set_queue_snapshot(guild_id.get(), snapshot);
            ctx.say("🔁 **Queue** will repeat when it finishes.")
                .await?;
        }
    }
    Ok(())
}

/// Clear upcoming tracks (keeps the current song)
#[poise::command(slash_command, guild_only)]
pub async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    let mut n = 0u32;
    queue.modify_queue(|q| {
        while q.len() > 1 {
            if let Some(t) = q.pop_back() {
                let _ = t.stop();
                n += 1;
            }
        }
    });
    ctx.say(format!("🗑️ Cleared **{}** upcoming track(s)", n))
        .await?;
    Ok(())
}

/// Shuffle upcoming tracks (not the current song)
#[poise::command(slash_command, guild_only)]
pub async fn shuffle(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.modify_queue(|q| shuffle_queue_tail_keep_head(q, &mut thread_rng()));
    ctx.say("🔀 Shuffled upcoming tracks").await?;
    Ok(())
}

/// Remove a track by queue position (1 = now playing, 2 = next, …)
#[poise::command(slash_command, guild_only)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Position in queue (1 = now playing)"] position: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Guild only")?;
    if position == 0 {
        ctx.say("❌ Position must be at least 1").await?;
        return Ok(());
    }
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    let idx = (position - 1) as usize;
    if idx >= queue.len() {
        ctx.say("❌ Invalid position").await?;
        return Ok(());
    }
    if idx == 0 {
        queue.skip()?;
        ctx.say("⏭️ Removed current track (skipped)").await?;
    } else if let Some(t) = queue.dequeue(idx) {
        let _ = t.stop();
        ctx.say(format!("🗑️ Removed track at position **{}**", position))
            .await?;
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
    if from == 0 || to == 0 {
        ctx.say("❌ Positions must be ≥ 1").await?;
        return Ok(());
    }
    let from_i = (from - 1) as usize;
    let to_i = (to - 1) as usize;
    if from_i == to_i {
        ctx.say("Nothing to do.").await?;
        return Ok(());
    }
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Songbird not initialized")?;
    let Some(lock) = manager.get(guild_id) else {
        ctx.say("❌ Not in voice").await?;
        return Ok(());
    };
    let handler = lock.lock().await;
    let queue = handler.queue();
    queue.modify_queue(|q| move_by_adjacent_swaps(q, from_i, to_i));
    ctx.say(format!("↔️ Moved **{}** → **{}**", from, to))
        .await?;
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

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        let all = queue.current_queue();
        if all.is_empty() {
            ctx.say("📭 Queue is empty").await?;
            return Ok(());
        }

        let mut total_dur = Duration::ZERO;
        for h in &all {
            if let Some(d) = h.data::<TrackUserData>().duration {
                total_dur += d;
            }
        }

        let tracks = all.clone();
        drop(handler);

        let mut page = 0usize;
        let items_per_page = 10;
        let upcoming_only_len = tracks.len().saturating_sub(1);
        let total_pages = if upcoming_only_len == 0 {
            1usize
        } else {
            (upcoming_only_len as f32 / items_per_page as f32).ceil() as usize
        };

        let mut description = String::new();
        if let Some(h) = tracks.first() {
            let d = h.data::<TrackUserData>();
            let st = h.get_info().await.ok();
            let pos = st.as_ref().map(|s| s.position);
            description.push_str("**Now playing:**\n");
            description.push_str(&format!(
                "🎶 **{}**\n{}\n\n",
                truncate(&d.title, 80),
                format_pos_dur(pos, d.duration)
            ));
        }

        let start = page * items_per_page;
        let end = (start + items_per_page).min(upcoming_only_len);
        if upcoming_only_len > 0 {
            description.push_str("**Up next:**\n");
            for i in start..end {
                let idx = i + 1;
                let h = &tracks[idx];
                let d = h.data::<TrackUserData>();
                let dur = d.duration.map(format_hms).unwrap_or_else(|| "?".into());
                description.push_str(&format!(
                    "{}. {} — `{}`\n",
                    idx + 1,
                    truncate(&d.title, 60),
                    dur
                ));
            }
        } else {
            description.push_str("*Nothing else queued*");
        }

        let loop_note = match ctx.data().music.loop_mode(guild_id.get()) {
            LoopMode::Off => "",
            LoopMode::Track => "\n🔁 Track loop on",
            LoopMode::Queue => "\n🔁 Queue loop on",
        };

        let embed = CreateEmbed::new()
            .title("🎶 Music Queue")
            .description(format!(
                "{}{}\n\n**Total (est.):** `{}`",
                description,
                loop_note,
                format_hms(total_dur)
            ))
            .footer(CreateEmbedFooter::new(format!(
                "Page {}/{}",
                page + 1,
                total_pages
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
        let message = reply.into_message().await?;

        while let Some(interaction) = message
            .await_component_interaction(ctx)
            .timeout(std::time::Duration::from_secs(60 * 5))
            .await
        {
            let custom_id = &interaction.data.custom_id;

            if ["pause", "skip", "stop"].contains(&custom_id.as_str()) {
                if let Some(handler_lock) = manager.get(guild_id) {
                    let mut handler = handler_lock.lock().await;
                    let queue = handler.queue();
                    match custom_id.as_str() {
                        "pause" => {
                            if let Some(cur) = queue.current() {
                                if let Ok(st) = cur.get_info().await {
                                    if st.playing == PlayMode::Pause {
                                        let _ = queue.resume();
                                    } else {
                                        let _ = queue.pause();
                                    }
                                }
                            }
                        }
                        "skip" => {
                            let _ = queue.skip();
                        }
                        "stop" => {
                            queue.stop();
                            ctx.data().music.clear_voice_hooks(guild_id.get());
                            handler.leave().await.ok();
                            let _ = interaction
                                .create_response(
                                    ctx.serenity_context(),
                                    CreateInteractionResponse::UpdateMessage(
                                        CreateInteractionResponseMessage::new()
                                            .content("Stopped playback and left channel.")
                                            .components(vec![])
                                            .embeds(vec![]),
                                    ),
                                )
                                .await;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            } else if custom_id == "prev" {
                page = page.saturating_sub(1);
            } else if custom_id == "next" {
                page = (page + 1).min(total_pages.saturating_sub(1));
            }

            let mut new_description = String::new();
            if let Some(handler_lock) = manager.get(guild_id) {
                let handler = handler_lock.lock().await;
                let queue = handler.queue();
                let all = queue.current_queue();
                if all.is_empty() {
                    let _ = interaction
                        .create_response(
                            ctx.serenity_context(),
                            CreateInteractionResponse::UpdateMessage(
                                CreateInteractionResponseMessage::new()
                                    .content("Queue is now empty.")
                                    .components(vec![])
                                    .embeds(vec![]),
                            ),
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
                if let Some(h) = all.first() {
                    let d = h.data::<TrackUserData>();
                    let st = h.get_info().await.ok();
                    let pos = st.as_ref().map(|s| s.position);
                    new_description.push_str("**Now playing:**\n");
                    new_description.push_str(&format!(
                        "🎶 **{}**\n{}\n\n",
                        truncate(&d.title, 80),
                        format_pos_dur(pos, d.duration)
                    ));
                }
                let start = page * items_per_page;
                let end = (start + items_per_page).min(upcoming_only_len);
                if upcoming_only_len > 0 {
                    new_description.push_str("**Up next:**\n");
                    for i in start..end {
                        let idx = i + 1;
                        let h = &all[idx];
                        let d = h.data::<TrackUserData>();
                        let dur = d.duration.map(format_hms).unwrap_or_else(|| "?".into());
                        new_description.push_str(&format!(
                            "{}. {} — `{}`\n",
                            idx + 1,
                            truncate(&d.title, 60),
                            dur
                        ));
                    }
                } else {
                    new_description.push_str("*Nothing else queued*");
                }

                let mut total_dur = Duration::ZERO;
                for h in &all {
                    if let Some(d) = h.data::<TrackUserData>().duration {
                        total_dur += d;
                    }
                }

                let loop_note = match ctx.data().music.loop_mode(guild_id.get()) {
                    LoopMode::Off => "",
                    LoopMode::Track => "\n🔁 Track loop on",
                    LoopMode::Queue => "\n🔁 Queue loop on",
                };

                let new_embed = CreateEmbed::new()
                    .title("🎶 Music Queue")
                    .description(format!(
                        "{}{}\n\n**Total (est.):** `{}`",
                        new_description,
                        loop_note,
                        format_hms(total_dur)
                    ))
                    .footer(CreateEmbedFooter::new(format!(
                        "Page {}/{}",
                        page + 1,
                        total_pages
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
                    .create_response(
                        ctx.serenity_context(),
                        CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .embed(new_embed)
                                .components(vec![row]),
                        ),
                    )
                    .await;
            }
        }
    } else {
        ctx.say("❌ I'm not in a voice channel").await?;
    }

    Ok(())
}
