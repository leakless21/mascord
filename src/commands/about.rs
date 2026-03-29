use crate::{Context, Error};

/// Quick overview of Mascord features and setup.
#[poise::command(slash_command)]
pub async fn about(ctx: Context<'_>) -> Result<(), Error> {
    let embed = poise::serenity_prelude::CreateEmbed::new()
        .title("Mascord")
        .description(
            "Agentic Discord assistant with chat, memory, reminders, RAG search, web tools, and music.",
        )
        .field(
            "Core Commands",
            "Mention/reply for the agent; `/search`, `/memory`, `/reminder`, `/settings`, `/play`, `/queue`",
            false,
        )
        .field(
            "Admin Safety",
            "Keep `REGISTER_COMMANDS=false` for normal restarts. Only enable it when command schema changes.",
            false,
        )
        .field(
            "Deployment",
            "Use systemd + release build for production. Use `cargo watch` only in development.",
            false,
        )
        .color(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
