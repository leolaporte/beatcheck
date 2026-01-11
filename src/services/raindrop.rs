use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const RAINDROP_API_URL: &str = "https://api.raindrop.io/rest/v1";

#[derive(Debug, Serialize)]
struct CreateRaindropRequest {
    link: String,
    title: Option<String>,
    excerpt: Option<String>,
    tags: Vec<String>,
    #[serde(rename = "pleaseParse")]
    please_parse: PleaseParse,
}

#[derive(Debug, Serialize)]
struct PleaseParse {}

#[derive(Debug, Deserialize)]
struct RaindropResponse {
    #[allow(dead_code)]
    result: bool,
    item: Option<RaindropItem>,
}

#[derive(Debug, Deserialize)]
struct RaindropItem {
    #[serde(rename = "_id")]
    id: i64,
}

pub struct RaindropClient {
    client: Client,
    access_token: String,
}

impl RaindropClient {
    pub fn new(access_token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        Self {
            client,
            access_token,
        }
    }

    /// Save a bookmark to Raindrop.io
    pub async fn save_bookmark(
        &self,
        url: &str,
        title: Option<&str>,
        excerpt: Option<&str>,
        tags: Vec<String>,
    ) -> Result<i64> {
        let request = CreateRaindropRequest {
            link: url.to_string(),
            title: title.map(|s| s.to_string()),
            excerpt: excerpt.map(|s| s.to_string()),
            tags,
            please_parse: PleaseParse {},
        };

        let response = self
            .client
            .post(format!("{}/raindrop", RAINDROP_API_URL))
            .bearer_auth(&self.access_token)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(AppError::RaindropApi(format!("API error: {}", error_text)));
        }

        let raindrop_response: RaindropResponse = response.json().await?;

        raindrop_response
            .item
            .map(|item| item.id)
            .ok_or_else(|| AppError::RaindropApi("No item returned from API".to_string()))
    }
}
