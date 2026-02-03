use anyhow::Context as _;
use poise::serenity_prelude as serenity;
use serde_json::Value;
use std::time::Duration;

pub struct ToolConfirmationContext<'a> {
    pub serenity_ctx: &'a serenity::Context,
    pub channel_id: serenity::ChannelId,
    pub user_id: serenity::UserId,
    pub timeout: Duration,
}

impl<'a> ToolConfirmationContext<'a> {
    pub fn new(
        serenity_ctx: &'a serenity::Context,
        channel_id: serenity::ChannelId,
        user_id: serenity::UserId,
        timeout: Duration,
    ) -> Self {
        Self {
            serenity_ctx,
            channel_id,
            user_id,
            timeout,
        }
    }
}

pub async fn confirm_tool_execution(
    ctx: &ToolConfirmationContext<'_>,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<bool> {
    use serenity::{
        ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateInteractionResponse,
        CreateInteractionResponseMessage, CreateMessage,
    };

    let mut args_pretty = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string());
    if args_pretty.len() > 1000 {
        args_pretty.truncate(1000);
        args_pretty.push('…');
    }

    let embed = CreateEmbed::new()
        .title("Tool Confirmation Required")
        .description(format!(
            "Requested tool: `{}`\nRequested by: <@{}>\n\nArguments:\n```json\n{}\n```",
            tool_name, ctx.user_id, args_pretty
        ))
        .color(0xFEE75C);

    let confirm_btn = CreateButton::new("confirm_tool")
        .label("Confirm")
        .style(ButtonStyle::Success);
    let cancel_btn = CreateButton::new("cancel_tool")
        .label("Cancel")
        .style(ButtonStyle::Danger);
    let row = CreateActionRow::Buttons(vec![confirm_btn, cancel_btn]);

    let mut message = ctx
        .channel_id
        .send_message(
            &ctx.serenity_ctx.http,
            CreateMessage::new().embed(embed).components(vec![row]),
        )
        .await
        .context("Failed to send tool confirmation message")?;

    loop {
        let Some(interaction) = message
            .await_component_interaction(ctx.serenity_ctx)
            .timeout(ctx.timeout)
            .await
        else {
            // Timeout: best-effort disable buttons.
            let _ = message
                .edit(
                    &ctx.serenity_ctx.http,
                    serenity::EditMessage::new().components(Vec::new()),
                )
                .await;
            return Ok(false);
        };

        // Only the requesting user can confirm/cancel.
        if interaction.user.id != ctx.user_id {
            let _ = interaction
                .create_response(
                    &ctx.serenity_ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(format!(
                                "Only <@{}> can confirm this tool execution.",
                                ctx.user_id
                            ))
                            .ephemeral(true),
                    ),
                )
                .await;
            continue;
        }

        let custom_id = interaction.data.custom_id.as_str();
        let decision = match custom_id {
            "confirm_tool" => Some(true),
            "cancel_tool" => Some(false),
            _ => None,
        };

        if let Some(confirmed) = decision {
            let status = if confirmed {
                "Confirmed. Executing tool…"
            } else {
                "Cancelled."
            };

            let _ = interaction
                .create_response(
                    &ctx.serenity_ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .content(status)
                            .components(vec![])
                            .embeds(vec![]),
                    ),
                )
                .await;

            return Ok(confirmed);
        }
    }
}
