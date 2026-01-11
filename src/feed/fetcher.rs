use std::time::Duration;

use feed_rs::parser;
use futures::stream::{self, StreamExt};
use reqwest::Client;

use crate::error::Result;
use crate::models::{Feed, NewArticle};

pub struct FeedFetcher {
    client: Client,
}

impl FeedFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("rss-reader/0.1")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    pub async fn fetch_feed(&self, feed_id: i64, url: &str) -> Result<Vec<NewArticle>> {
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch feed: HTTP {}", response.status()).into());
        }

        let bytes = response.bytes().await?;
        let feed = parser::parse(&bytes[..])?;

        let articles: Vec<NewArticle> = feed
            .entries
            .into_iter()
            .map(|entry| {
                // Try content first, then fall back to summary
                let content_html = entry
                    .content
                    .as_ref()
                    .and_then(|c| c.body.as_ref())
                    .or_else(|| entry.summary.as_ref().map(|s| &s.content));

                let content_text = content_html.and_then(|html| {
                    html2text::from_read(html.as_bytes(), 80).ok()
                });

                NewArticle {
                    feed_id,
                    guid: entry.id,
                    title: entry
                        .title
                        .map(|t| t.content)
                        .unwrap_or_else(|| "Untitled".to_string()),
                    url: entry
                        .links
                        .first()
                        .map(|l| l.href.clone())
                        .unwrap_or_default(),
                    author: entry.authors.first().map(|a| a.name.clone()),
                    content: content_html.cloned(),
                    content_text,
                    published_at: entry.published.or(entry.updated),
                }
            })
            .collect();

        Ok(articles)
    }

    /// Refresh all feeds concurrently with rate limiting
    pub async fn refresh_all(&self, feeds: Vec<Feed>) -> Vec<(i64, Vec<NewArticle>)> {
        let results: Vec<_> = stream::iter(feeds)
            .map(|feed| async move {
                match self.fetch_feed(feed.id, &feed.url).await {
                    Ok(articles) => {
                        tracing::debug!("Fetched {} articles from {}", articles.len(), feed.title);
                        Some((feed.id, articles))
                    }
                    Err(e) => {
                        tracing::debug!("Failed to fetch {}: {}", feed.url, e);
                        None
                    }
                }
            })
            .buffer_unordered(5) // Max 5 concurrent fetches
            .filter_map(|r| async { r })
            .collect()
            .await;

        results
    }
}

impl Default for FeedFetcher {
    fn default() -> Self {
        Self::new()
    }
}
