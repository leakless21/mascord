//! Long assistant replies as embeds (used by mention/reply handlers).

use crate::config::DISCORD_EMBED_LIMIT;
use crate::Error;
use poise::serenity_prelude::{CacheHttp, ChannelId, MessageId};
use poise::serenity_prelude::{CreateEmbed, CreateEmbedFooter, CreateMessage};

/// Send an embed response in a channel, optionally as a reply to another message.
pub async fn send_embed_reply(
    http: impl CacheHttp,
    channel_id: ChannelId,
    content: &str,
    reply_to: Option<MessageId>,
) -> Result<Vec<MessageId>, Error> {
    let mut sent_ids = Vec::new();
    let mut message = CreateMessage::new();
    if let Some(id) = reply_to {
        message = message.reference_message((channel_id, id));
    }

    if content.len() <= DISCORD_EMBED_LIMIT {
        let embed = CreateEmbed::new()
            .title("🤖 Mascord Response")
            .description(content)
            .color(0x5865F2)
            .footer(CreateEmbedFooter::new("Powered by llama.cpp"));

        let sent = channel_id.send_message(http, message.embed(embed)).await?;
        sent_ids.push(sent.id);
    } else {
        let chunks: Vec<&str> = content
            .as_bytes()
            .chunks(DISCORD_EMBED_LIMIT - 100)
            .map(|c| std::str::from_utf8(c).unwrap_or("..."))
            .collect();

        for (i, chunk) in chunks.iter().enumerate() {
            let embed = CreateEmbed::new()
                .title(format!("🤖 Response (Part {}/{})", i + 1, chunks.len()))
                .description(*chunk)
                .color(0x5865F2);

            let sent = channel_id
                .send_message(&http, message.clone().embed(embed))
                .await?;
            sent_ids.push(sent.id);
        }
    }
    Ok(sent_ids)
}
