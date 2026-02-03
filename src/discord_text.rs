use poise::serenity_prelude as serenity;

pub fn strip_bot_mentions(input: &str, bot_id: u64) -> String {
    let mention = format!("<@{}>", bot_id);
    let mention_nick = format!("<@!{}>", bot_id);

    input
        .replace(&mention, "")
        .replace(&mention_nick, "")
        .trim()
        .to_string()
}

pub fn extract_message_text(message: &serenity::Message) -> String {
    let mut parts = Vec::new();

    let content = message.content.trim();
    if !content.is_empty() {
        parts.push(content.to_string());
    }

    for embed in &message.embeds {
        if let Some(title) = &embed.title {
            let title = title.trim();
            if !title.is_empty() {
                parts.push(title.to_string());
            }
        }

        if let Some(description) = &embed.description {
            let description = description.trim();
            if !description.is_empty() {
                parts.push(description.to_string());
            }
        }

        for field in &embed.fields {
            let name = field.name.trim();
            let value = field.value.trim();

            if name.is_empty() && value.is_empty() {
                continue;
            }

            if name.is_empty() {
                parts.push(value.to_string());
                continue;
            }

            if value.is_empty() {
                parts.push(name.to_string());
                continue;
            }

            parts.push(format!("{}: {}", name, value));
        }
    }

    parts.join("\n")
}
