use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::{AppError, Result};

const RAINDROP_API_URL: &str = "https://api.raindrop.io/rest/v1";
const NEWS_COLLECTION_NAME: &str = "News";

// Cache for collection ID
static NEWS_COLLECTION_ID: OnceLock<Mutex<Option<i64>>> = OnceLock::new();

#[derive(Debug, Serialize)]
struct CreateRaindropRequest {
    link: String,
    title: Option<String>,
    excerpt: Option<String>,
    tags: Vec<String>,
    #[serde(rename = "pleaseParse")]
    please_parse: PleaseParse,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection: Option<CollectionRef>,
}

#[derive(Debug, Serialize)]
struct CollectionRef {
    #[serde(rename = "$id")]
    id: i64,
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

#[derive(Debug, Deserialize)]
struct CollectionsResponse {
    items: Vec<Collection>,
}

#[derive(Debug, Deserialize)]
struct Collection {
    #[serde(rename = "_id")]
    id: i64,
    title: String,
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

    /// Get the News collection ID, fetching and caching it if needed
    async fn get_news_collection_id(&self) -> Result<Option<i64>> {
        let cache = NEWS_COLLECTION_ID.get_or_init(|| Mutex::new(None));
        let mut cached = cache.lock().await;

        if cached.is_some() {
            return Ok(*cached);
        }

        // Fetch collections from API
        let response = self
            .client
            .get(format!("{}/collections", RAINDROP_API_URL))
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::warn!("Failed to fetch collections, will save to unsorted");
            return Ok(None);
        }

        let collections: CollectionsResponse = response.json().await?;

        // Find the News collection
        let news_id = collections
            .items
            .iter()
            .find(|c| c.title == NEWS_COLLECTION_NAME)
            .map(|c| c.id);

        if news_id.is_none() {
            tracing::warn!("News collection not found, will save to unsorted");
        }

        *cached = news_id;
        Ok(news_id)
    }

    /// Save a bookmark to Raindrop.io (in the News collection)
    pub async fn save_bookmark(
        &self,
        url: &str,
        title: Option<&str>,
        excerpt: Option<&str>,
        tags: Vec<String>,
    ) -> Result<i64> {
        // Get the News collection ID
        let collection = self
            .get_news_collection_id()
            .await?
            .map(|id| CollectionRef { id });

        let request = CreateRaindropRequest {
            link: url.to_string(),
            title: title.map(|s| s.to_string()),
            excerpt: excerpt.map(|s| s.to_string()),
            tags,
            please_parse: PleaseParse {},
            collection,
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
