use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const CLAUDE_MODEL: &str = "claude-haiku-4-5-20251001";

#[derive(Debug, Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct MessageResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    content_type: String,
    text: Option<String>,
}

pub struct Summarizer {
    client: Client,
    api_key: String,
}

impl Summarizer {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");
        Self { client, api_key }
    }

    pub async fn generate_summary(
        &self,
        article_title: &str,
        article_content: &str,
    ) -> Result<String> {
        // Truncate content if too long (find valid UTF-8 boundary)
        let content = if article_content.len() > 10000 {
            let mut end = 10000;
            while end > 0 && !article_content.is_char_boundary(end) {
                end -= 1;
            }
            &article_content[..end]
        } else {
            article_content
        };

        let user_message = format!(
            r#"You are a journalist summarizing articles using the nut graph structure. Summarize the article below using the appropriate format.

First, determine: Is this article primarily about a specific PRODUCT (hardware, software, app, device) or is it EDITORIAL (news, policy, analysis, industry event)?

RULES:
1. Use ONLY information from the article - no external knowledge
2. If the article has insufficient content, respond with just: "Insufficient content for summary"
3. QUOTE must be copied VERBATIM from the article — the exact words as they appear, with clear speaker attribution. Do not paraphrase or alter the quote in any way.

If EDITORIAL, respond with:
One strong sentence identifying WHO is involved and WHAT happened or was announced.

A paragraph (2-4 sentences) explaining WHY this matters. Contextualize the most important facts and give the reader a clear understanding of the central issue or topic.

"exact verbatim quote from the article" -- Speaker Name

If PRODUCT, respond with:
What the product is and what it does (1-2 sentences).

Pricing details. Omit if not mentioned.

When and where it is available. Omit if not mentioned.

What platforms or operating systems it runs on. Omit for hardware-only products or if not mentioned.

"exact verbatim quote from the article" -- Speaker Name

Do NOT include any labels, prefixes, or headings. Just the plain text. Omit the quote line if there are no direct quotes with clear speaker attribution in the article.

Title: {}

Article:
{}"#,
            article_title, content
        );

        let request = MessageRequest {
            model: CLAUDE_MODEL.to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: user_message,
            }],
            system: None,
        };

        let response = self
            .client
            .post(CLAUDE_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(AppError::ClaudeApi(format!("API error: {}", error_text)));
        }

        let message_response: MessageResponse = response.json().await?;

        let summary = message_response
            .content
            .into_iter()
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n");

        // Strip format/type labels the model sometimes adds despite instructions
        let summary = summary
            .lines()
            .filter(|line| {
                let upper = line.trim().to_uppercase();
                !upper.starts_with("FORMAT:")
                    && upper != "EDITORIAL"
                    && upper != "PRODUCT"
                    && upper != "**EDITORIAL**"
                    && upper != "**PRODUCT**"
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        Ok(summary)
    }

    pub fn model_version(&self) -> &'static str {
        CLAUDE_MODEL
    }
}
