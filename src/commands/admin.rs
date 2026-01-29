use crate::{Context, Error};
use tracing::info;

/// Shut down the bot (Owner only)
#[poise::command(slash_command, owners_only, hide_in_help)]
pub async fn shutdown(ctx: Context<'_>) -> Result<(), Error> {
    info!("Shutdown command received from owner: {}", ctx.author().name);
    ctx.say("👋 Shutting down...").await?;
    ctx.framework().shard_manager().shutdown_all().await;
    Ok(())
}
