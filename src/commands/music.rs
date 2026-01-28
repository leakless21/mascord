use crate::{Context, Error};
use songbird::input::YoutubeDl;
use poise::serenity_prelude::CreateEmbed;

/// Join a voice channel
#[poise::command(
    slash_command,
    guild_only,
    required_bot_permissions = "CONNECT | SPEAK"
)]
pub async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("This command must be used in a server")?;
    
    let channel_id = {
        let guild = ctx.guild().ok_or("Could not access guild")?;
        guild.voice_states
            .get(&ctx.author().id)
            .and_then(|vs| vs.channel_id)
            .ok_or("You must be in a voice channel to use this command")?
    };

    let manager = songbird::get(ctx.serenity_context()).await
        .ok_or("Songbird Voice client not initialized")?.clone();

    match manager.join(guild_id, channel_id).await {
        Ok(_) => {
            ctx.say(format!("🔊 Joined <#{}>", channel_id)).await?;
        }
        Err(e) => {
            ctx.say(format!("❌ Failed to join voice channel: {}", e)).await?;
        }
    }

    Ok(())
}

/// Play audio from YouTube
#[poise::command(
    slash_command,
    guild_only,
    required_bot_permissions = "CONNECT | SPEAK"
)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "YouTube URL or search query"] url: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().ok_or("This command must be used in a server")?;
    let manager = songbird::get(ctx.serenity_context()).await
        .ok_or("Songbird Voice client not initialized")?.clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let source = YoutubeDl::new(ctx.data().http_client.clone(), url.clone());
        handler.enqueue_input(source.into()).await;

        let embed = CreateEmbed::new()
            .title("🎵 Added to Queue")
            .description(format!("```{}```", truncate(&url, 100)))
            .color(0x57F287);
        
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
    } else {
        ctx.say("❌ I'm not in a voice channel. Use `/join` first.").await?;
    }

    Ok(())
}

/// Skip the current song
#[poise::command(slash_command, guild_only)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("This command must be used in a server")?;
    let manager = songbird::get(ctx.serenity_context()).await
        .ok_or("Songbird Voice client not initialized")?;

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        
        if queue.is_empty() {
            ctx.say("📭 Queue is empty").await?;
        } else {
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
    let guild_id = ctx.guild_id().ok_or("This command must be used in a server")?;
    let manager = songbird::get(ctx.serenity_context()).await
        .ok_or("Songbird Voice client not initialized")?;

    if manager.get(guild_id).is_some() {
        manager.remove(guild_id).await?;
        ctx.say("👋 Left voice channel").await?;
    } else {
        ctx.say("❌ I'm not in a voice channel").await?;
    }

    Ok(())
}

/// Show the current queue
#[poise::command(slash_command, guild_only)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("This command must be used in a server")?;
    let manager = songbird::get(ctx.serenity_context()).await
        .ok_or("Songbird Voice client not initialized")?;

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        
        if queue.is_empty() {
            ctx.say("📭 Queue is empty").await?;
        } else {
            let count = queue.len();
            let embed = CreateEmbed::new()
                .title("🎶 Current Queue")
                .description(format!("{} track(s) in queue", count))
                .color(0x5865F2);
            
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
        }
    } else {
        ctx.say("❌ I'm not in a voice channel").await?;
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}
