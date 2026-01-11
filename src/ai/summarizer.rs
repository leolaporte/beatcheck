use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const CLAUDE_MODEL: &str = "claude-sonnet-4-20250514";

#[derive(Debug, Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
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
        let system_prompt = r#"You are a helpful assistant that summarizes news articles.
Provide a concise, informative summary in 2-3 paragraphs.
Focus on the key facts, main arguments, and important conclusions.
Use clear, accessible language."#;

        // Truncate content if too long
        let content = if article_content.len() > 10000 {
            &article_content[..10000]
        } else {
            article_content
        };

        let user_message = format!(
            "Please summarize the following article:\n\nTitle: {}\n\nContent:\n{}",
            article_title, content
        );

        let request = MessageRequest {
            model: CLAUDE_MODEL.to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: user_message,
            }],
            system: Some(system_prompt.to_string()),
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

        Ok(summary)
    }

    pub fn model_version(&self) -> &'static str {
        CLAUDE_MODEL
    }
}
