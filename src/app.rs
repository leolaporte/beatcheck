use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::ai::Summarizer;
use crate::config::Config;
use crate::db::Repository;
use crate::error::Result;
use crate::feed::{parse_opml_file, FeedFetcher};
use crate::models::{Article, ArticleFilter, Feed, Summary, SummaryStatus};
use crate::services::RaindropClient;
use crate::tui::AppAction;

// Message for completed summary
pub struct SummaryResult {
    pub article_id: i64,
    pub result: std::result::Result<(String, String), String>, // (content, model) or error
}

pub struct App {
    // Data
    pub feeds: Vec<Feed>,
    pub articles: Vec<Article>,
    pub current_summary: Option<Summary>,

    // UI State
    pub selected_index: usize,
    pub filter: ArticleFilter,
    pub show_help: bool,
    pub tag_input_active: bool,
    pub tag_input: String,
    pub is_saved_to_raindrop: bool,
    selection_time: Option<Instant>,

    // Async state
    pub is_refreshing: bool,
    pub summary_status: SummaryStatus,
    pub pending_summary_article_id: Option<i64>,
    summary_rx: mpsc::Receiver<SummaryResult>,
    summary_tx: mpsc::Sender<SummaryResult>,

    // Services
    pub repository: Repository,
    fetcher: FeedFetcher,
    summarizer: Option<Arc<Summarizer>>,
    raindrop: Option<RaindropClient>,
}

impl App {
    pub async fn new(config: &Config) -> Result<Self> {
        let repository = Repository::new(&config.db_path).await?;
        let fetcher = FeedFetcher::new();

        let summarizer = config
            .claude_api_key
            .as_ref()
            .map(|key| Arc::new(Summarizer::new(key.clone())));

        let raindrop = config
            .raindrop_token
            .as_ref()
            .map(|token| RaindropClient::new(token.clone()));

        let feeds = repository.get_all_feeds().await?;
        let articles = repository.get_all_articles_sorted().await?;

        let (summary_tx, summary_rx) = mpsc::channel(1);

        Ok(Self {
            feeds,
            articles,
            current_summary: None,
            selected_index: 0,
            filter: ArticleFilter::Unread,
            show_help: false,
            tag_input_active: false,
            tag_input: String::new(),
            is_saved_to_raindrop: false,
            selection_time: None,
            is_refreshing: false,
            summary_status: SummaryStatus::NotGenerated,
            pending_summary_article_id: None,
            summary_rx,
            summary_tx,
            repository,
            fetcher,
            summarizer,
            raindrop,
        })
    }

    pub fn filtered_articles(&self) -> Vec<&Article> {
        self.articles
            .iter()
            .filter(|a| match self.filter {
                ArticleFilter::All => true,
                ArticleFilter::Unread => !a.is_read,
                ArticleFilter::Starred => a.is_starred,
            })
            .collect()
    }

    pub fn selected_article(&self) -> Option<&Article> {
        let articles = self.filtered_articles();
        articles.get(self.selected_index).copied()
    }

    pub async fn handle_action(&mut self, action: AppAction) -> Result<bool> {
        match action {
            AppAction::Quit => return Ok(true),

            AppAction::MoveUp => {
                let len = self.filtered_articles().len();
                if len > 0 && self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.on_selection_changed().await?;
                }
            }

            AppAction::MoveDown => {
                let len = self.filtered_articles().len();
                if len > 0 && self.selected_index < len - 1 {
                    self.selected_index += 1;
                    self.on_selection_changed().await?;
                }
            }

            AppAction::SelectArticle => {
                self.generate_summary().await?;
            }

            AppAction::RefreshFeeds => {
                self.refresh_feeds().await?;
            }

            AppAction::ToggleStarred => {
                if let Some(article) = self.selected_article() {
                    let id = article.id;
                    self.repository.toggle_article_starred(id).await?;
                    self.reload_articles().await?;
                }
            }

            AppAction::ToggleRead => {
                if let Some(article) = self.selected_article() {
                    let id = article.id;
                    let new_state = !article.is_read;
                    self.repository.mark_article_read(id, new_state).await?;
                    self.reload_articles().await?;
                }
            }

            AppAction::OpenInBrowser => {
                if let Some(article) = self.selected_article() {
                    let url = article.url.clone();
                    let _ = open::that(&url);
                }
            }

            AppAction::SaveToRaindrop => {
                if self.raindrop.is_some() && self.selected_article().is_some() {
                    self.tag_input_active = true;
                    self.tag_input.clear();
                }
            }

            AppAction::CycleFilter => {
                self.filter = self.filter.cycle();
                self.selected_index = 0;
                self.on_selection_changed().await?;
            }

            AppAction::RegenerateSummary => {
                self.summary_status = SummaryStatus::NotGenerated;
                self.current_summary = None;
                self.generate_summary().await?;
            }

            AppAction::ImportOpml(path) => {
                self.import_opml(&path).await?;
            }

            AppAction::ShowHelp => {
                self.show_help = true;
            }

            AppAction::HideHelp => {
                self.show_help = false;
            }

            AppAction::TagInputChar(c) => {
                self.tag_input.push(c);
            }

            AppAction::TagInputBackspace => {
                self.tag_input.pop();
            }

            AppAction::TagInputConfirm => {
                self.save_to_raindrop().await?;
                self.tag_input_active = false;
                self.tag_input.clear();
            }

            AppAction::TagInputCancel => {
                self.tag_input_active = false;
                self.tag_input.clear();
            }
        }

        Ok(false)
    }

    async fn on_selection_changed(&mut self) -> Result<()> {
        // Reset summary state when selection changes
        self.summary_status = SummaryStatus::NotGenerated;
        self.current_summary = None;

        // Start the read timer for the new selection
        self.selection_time = Some(Instant::now());

        // Check if current article is saved to raindrop
        let article_id = self.selected_article().map(|a| a.id);
        if let Some(id) = article_id {
            self.is_saved_to_raindrop = self
                .repository
                .is_saved_to_raindrop(id)
                .await?;

            // Check for cached summary
            if let Some(summary) = self.repository.get_summary(id).await? {
                self.current_summary = Some(summary);
                self.summary_status = SummaryStatus::Generated;
            }
        }

        Ok(())
    }

    async fn generate_summary(&mut self) -> Result<()> {
        let Some(summarizer) = &self.summarizer else {
            return Ok(());
        };

        let Some(article) = self.selected_article() else {
            return Ok(());
        };

        // Check cache first
        if let Some(summary) = self.repository.get_summary(article.id).await? {
            self.current_summary = Some(summary);
            self.summary_status = SummaryStatus::Generated;
            return Ok(());
        }

        // Mark article as read
        self.repository.mark_article_read(article.id, true).await?;

        let article_id = article.id;
        let title = article.title.clone();
        let content = article
            .content_text
            .clone()
            .or_else(|| article.content.clone())
            .unwrap_or_default();

        self.summary_status = SummaryStatus::Generating;
        self.pending_summary_article_id = Some(article_id);

        // Spawn background task for summary generation
        let summarizer = Arc::clone(summarizer);
        let tx = self.summary_tx.clone();

        tokio::spawn(async move {
            let result = match summarizer.generate_summary(&title, &content).await {
                Ok(summary_text) => {
                    let model = summarizer.model_version().to_string();
                    Ok((summary_text, model))
                }
                Err(e) => Err(e.to_string()),
            };

            let _ = tx.send(SummaryResult { article_id, result }).await;
        });

        // Reload to update read status
        self.reload_articles().await?;

        Ok(())
    }

    /// Poll for completed summary results (non-blocking)
    pub async fn poll_summary_result(&mut self) -> Result<()> {
        if let Ok(result) = self.summary_rx.try_recv() {
            // Only process if this is the summary we're waiting for
            if self.pending_summary_article_id == Some(result.article_id) {
                match result.result {
                    Ok((summary_text, model)) => {
                        self.repository
                            .save_summary(result.article_id, summary_text.clone(), model.clone())
                            .await?;

                        self.current_summary = Some(Summary {
                            id: 0,
                            article_id: result.article_id,
                            content: summary_text,
                            model_version: model,
                            generated_at: chrono::Utc::now(),
                        });
                        self.summary_status = SummaryStatus::Generated;
                    }
                    Err(e) => {
                        tracing::error!("Failed to generate summary: {}", e);
                        self.summary_status = SummaryStatus::Failed;
                    }
                }
                self.pending_summary_article_id = None;
            }
        }
        Ok(())
    }

    /// Check if article has been viewed for 2+ seconds and mark as read
    pub async fn check_read_timer(&mut self) -> Result<()> {
        use std::time::Duration;

        if let Some(selection_time) = self.selection_time {
            if selection_time.elapsed() >= Duration::from_secs(2) {
                if let Some(article) = self.selected_article() {
                    if !article.is_read {
                        let id = article.id;
                        self.repository.mark_article_read(id, true).await?;
                        self.reload_articles().await?;
                    }
                }
                // Clear the timer so we don't keep checking
                self.selection_time = None;
            }
        }
        Ok(())
    }

    pub async fn refresh_feeds(&mut self) -> Result<()> {
        self.is_refreshing = true;

        let feeds = self.feeds.clone();
        let results = self.fetcher.refresh_all(feeds).await;

        for (feed_id, articles) in results {
            for article in articles {
                self.repository.upsert_article(article).await?;
            }
            self.repository.update_feed_last_fetched(feed_id).await?;
        }

        self.reload_articles().await?;
        self.is_refreshing = false;

        Ok(())
    }

    async fn reload_articles(&mut self) -> Result<()> {
        self.articles = self.repository.get_all_articles_sorted().await?;
        Ok(())
    }

    async fn save_to_raindrop(&mut self) -> Result<()> {
        let Some(raindrop) = &self.raindrop else {
            return Ok(());
        };

        let Some(article) = self.selected_article() else {
            return Ok(());
        };

        let tags: Vec<String> = self
            .tag_input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let article_id = article.id;
        let url = article.url.clone();
        let title = article.title.clone();

        match raindrop
            .save_bookmark(&url, Some(&title), None, tags.clone())
            .await
        {
            Ok(raindrop_id) => {
                self.repository
                    .mark_saved_to_raindrop(article_id, raindrop_id, tags)
                    .await?;
                // Mark as read when bookmarked
                self.repository.mark_article_read(article_id, true).await?;
                self.is_saved_to_raindrop = true;
                tracing::info!("Saved to Raindrop: {}", url);
            }
            Err(e) => {
                tracing::error!("Failed to save to Raindrop: {}", e);
            }
        }

        // Reload to update read status in the list
        self.reload_articles().await?;

        Ok(())
    }

    pub async fn import_opml(&mut self, path: &Path) -> Result<()> {
        let feeds = parse_opml_file(path)?;

        for feed in feeds {
            match self.repository.insert_feed(feed).await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Failed to insert feed: {}", e);
                }
            }
        }

        self.feeds = self.repository.get_all_feeds().await?;

        // Refresh the newly imported feeds
        self.refresh_feeds().await?;

        Ok(())
    }
}
