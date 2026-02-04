use crate::{Context, Error};
use tracing::info;

/// Shut down the bot (Owner only)
#[poise::command(slash_command, owners_only, hide_in_help)]
pub async fn shutdown(ctx: Context<'_>) -> Result<(), Error> {
    info!(
        "Shutdown command received from owner: {}",
        ctx.author().name
    );
    ctx.say("👋 Shutting down...").await?;
    ctx.framework().shard_manager().shutdown_all().await;
    Ok(())
}
/// Restart the bot (Owner only)
#[poise::command(slash_command, owners_only, hide_in_help)]
pub async fn restart(ctx: Context<'_>) -> Result<(), Error> {
    info!("Restart command received from owner: {}", ctx.author().name);
    ctx.say("🔄 Restarting bot...").await?;

    // Give Discord a moment to receive the message
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Shutdown gracefully (will exit with code 0)
    ctx.framework().shard_manager().shutdown_all().await;

    Ok(())
}
