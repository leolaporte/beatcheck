use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub id: i64,
    pub article_id: i64,
    pub content: String,
    pub model_version: String,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SummaryStatus {
    #[default]
    NotGenerated,
    Generating,
    Generated,
    Failed,
}
