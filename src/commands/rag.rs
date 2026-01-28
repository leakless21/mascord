use crate::{Context, Error};
use crate::rag::SearchFilter;
use chrono::{Utc, Duration};

/// Search for messages in history
#[poise::command(slash_command)]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Search query"] query: String,
    #[description = "Limit results to latest XX days"] days: Option<i64>,
    #[description = "Limit results to specific channel"] channel_id: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let db = &ctx.data().db;
    let llm_client = &ctx.data().llm_client;

    // Generate embedding for query
    let embedding = llm_client.get_embeddings(&query).await?;

    let mut filter = SearchFilter::default()
        .with_limit(5);

    if let Some(d) = days {
        let from_date = Utc::now() - Duration::days(d);
        filter = filter.with_from_date(from_date);
    }

    if let Some(c) = channel_id {
        filter = filter.with_channel(c);
    } else {
        // Default to current channel
        filter = filter.with_channel(ctx.channel_id().to_string());
    }

    // Perform search (Implementation in Database)
    let results = db.search_messages(&query, embedding, filter).await?;

    if results.is_empty() {
        ctx.say("No relevant messages found.").await?;
        return Ok(());
    }

    let mut response = String::from("**Search Results:**\n");
    for (i, msg) in results.iter().enumerate() {
        response.push_str(&format!("{}. [{}] <@{}>: {}\n", i+1, msg.timestamp, msg.user_id, msg.content));
    }

    ctx.say(response).await?;
    Ok(())
}
