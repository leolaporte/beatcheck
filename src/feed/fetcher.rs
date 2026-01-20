use std::time::Duration;

use feed_rs::parser;
use futures::stream::{self, StreamExt};
use regex::Regex;
use reqwest::Client;

use crate::error::Result;
use crate::models::{Feed, NewArticle, NewFeed};

#[derive(Clone)]
pub struct FeedFetcher {
    client: Client,
}

impl FeedFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("beatcheck/1.2.0")
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

    /// Discover and create a feed from a URL
    /// If the URL is a direct RSS/Atom feed, parse it directly
    /// If it's an HTML page, look for feed links in <link> tags
    pub async fn discover_feed(&self, url: &str) -> Result<NewFeed> {
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch URL: HTTP {}", response.status()).into());
        }

        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = response.bytes().await?;

        // Try parsing as RSS/Atom feed first
        if let Ok(feed) = parser::parse(&bytes[..]) {
            let title = feed
                .title
                .map(|t| t.content)
                .unwrap_or_else(|| "Untitled Feed".to_string());
            let description = feed.description.map(|d| d.content);
            let site_url = feed.links.first().map(|l| l.href.clone());

            return Ok(NewFeed {
                title,
                url: final_url,
                site_url,
                description,
            });
        }

        // If content looks like HTML, search for feed links
        if content_type.contains("html") || bytes.starts_with(b"<!") || bytes.starts_with(b"<html") {
            let html = String::from_utf8_lossy(&bytes);
            if let Some(feed_url) = self.find_feed_link(&html, &final_url) {
                // Fetch the discovered feed URL
                let feed_response = self.client.get(&feed_url).send().await?;
                if feed_response.status().is_success() {
                    let feed_bytes = feed_response.bytes().await?;
                    if let Ok(feed) = parser::parse(&feed_bytes[..]) {
                        let title = feed
                            .title
                            .map(|t| t.content)
                            .unwrap_or_else(|| "Untitled Feed".to_string());
                        let description = feed.description.map(|d| d.content);
                        let site_url = feed.links.first().map(|l| l.href.clone());

                        return Ok(NewFeed {
                            title,
                            url: feed_url,
                            site_url,
                            description,
                        });
                    }
                }
            }
        }

        Err(anyhow::anyhow!("Could not find RSS/Atom feed at this URL").into())
    }

    /// Search HTML for RSS/Atom feed links
    fn find_feed_link(&self, html: &str, base_url: &str) -> Option<String> {
        // Look for <link rel="alternate" type="application/rss+xml" href="...">
        // or <link rel="alternate" type="application/atom+xml" href="...">
        let link_re = Regex::new(
            r#"<link[^>]*rel=["']alternate["'][^>]*type=["']application/(rss|atom)\+xml["'][^>]*href=["']([^"']+)["']"#
        ).ok()?;

        // Also try reverse order (type before rel)
        let link_re2 = Regex::new(
            r#"<link[^>]*type=["']application/(rss|atom)\+xml["'][^>]*href=["']([^"']+)["']"#
        ).ok()?;

        let href = link_re
            .captures(html)
            .or_else(|| link_re2.captures(html))
            .and_then(|cap: regex::Captures| cap.get(2))
            .map(|m: regex::Match| m.as_str().to_string())?;

        // Resolve relative URLs
        Some(self.resolve_url(&href, base_url))
    }

    /// Resolve a potentially relative URL against a base URL
    fn resolve_url(&self, href: &str, base_url: &str) -> String {
        if href.starts_with("http://") || href.starts_with("https://") {
            return href.to_string();
        }

        if let Ok(base) = url::Url::parse(base_url) {
            if let Ok(resolved) = base.join(href) {
                return resolved.to_string();
            }
        }

        href.to_string()
    }
}

impl Default for FeedFetcher {
    fn default() -> Self {
        Self::new()
    }
}
