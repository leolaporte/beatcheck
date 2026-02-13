use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const CLAUDE_MODEL: &str = "claude-3-5-haiku-20241022";

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
            r#"You are a journalist writing in Axios Smart Brevity style. Summarize the article below using the appropriate format.

First, determine: Is this article primarily about a specific PRODUCT (hardware, software, app, device) or is it EDITORIAL (news, policy, analysis, industry event)?

RULES:
1. Use ONLY information from the article - no external knowledge
2. Each section should be 1-2 concise sentences
3. If the article has insufficient content, respond with just: "Insufficient content for summary"
4. If there are direct quotes with clear speaker attribution, include the most important one
5. Output ONLY the summary lines below - no introductions, conclusions, or commentary
6. Do NOT state the format type (e.g. "This is an EDITORIAL summary") - just start with the first line

If EDITORIAL, respond in this exact format:
What's happening: One strong sentence capturing the core news or development.
Why it matters: 1-2 sentences explaining why this is significant.
The big picture: One sentence on broader industry or societal implications. Omit this line if the article is too narrow for broader context.
"quote text" -- Speaker Name

If PRODUCT, respond in this exact format:
The product: What the product is and what it does (1-2 sentences).
Cost: Pricing details. Omit this line if pricing is not mentioned.
Availability: When and where it is available. Omit this line if not mentioned.
Platforms: What platforms or operating systems it runs on. Omit this line for hardware-only products or if not mentioned.
"quote text" -- Speaker Name

Omit the quote line if there are no quotes or no clear speaker attribution in the article.

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

        // Strip preamble lines like "This is an EDITORIAL summary"
        let summary = summary
            .lines()
            .filter(|line| {
                let lower = line.trim().to_lowercase();
                !lower.starts_with("this is an editorial")
                    && !lower.starts_with("this is a product")
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
