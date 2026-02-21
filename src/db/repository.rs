use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};
use tokio_rusqlite::Connection;

use crate::error::Result;
use crate::models::{Article, Feed, NewArticle, NewFeed, Summary};

use super::schema::SCHEMA;

pub struct Repository {
    conn: Connection,
}

impl Repository {
    pub async fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path).await?;

        conn.call(|conn| {
            // Set busy timeout to 5 seconds to handle concurrent access
            conn.busy_timeout(std::time::Duration::from_secs(5))?;
            // Enable WAL mode for better concurrency
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            conn.execute_batch(SCHEMA)?;
            Ok(())
        })
        .await?;

        Ok(Self { conn })
    }

    // Feed operations

    pub async fn insert_feed(&self, feed: NewFeed) -> Result<i64> {
        let id = self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO feeds (title, url, site_url, description) VALUES (?1, ?2, ?3, ?4)",
                    params![feed.title, feed.url, feed.site_url, feed.description],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?;
        Ok(id)
    }

    pub async fn get_all_feeds(&self) -> Result<Vec<Feed>> {
        let feeds = self
            .conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, title, url, site_url, description, last_fetched, created_at, updated_at FROM feeds ORDER BY title",
                )?;
                let feeds = stmt
                    .query_map([], feed_from_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(feeds)
            })
            .await?;
        Ok(feeds)
    }

    pub async fn update_feed_last_fetched(&self, id: i64) -> Result<()> {
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE feeds SET last_fetched = datetime('now'), updated_at = datetime('now') WHERE id = ?1",
                    params![id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_feed(&self, id: i64) -> Result<()> {
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM feeds WHERE id = ?1", params![id])?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    // Article operations

    pub async fn upsert_article(&self, article: NewArticle) -> Result<i64> {
        let id = self
            .conn
            .call(move |conn| {
                // Check if this article was previously deleted
                let was_deleted: bool = conn.query_row(
                    "SELECT 1 FROM deleted_articles WHERE feed_id = ?1 AND guid = ?2",
                    params![article.feed_id, article.guid],
                    |_| Ok(true),
                ).unwrap_or(false);

                if was_deleted {
                    return Ok(0); // Skip deleted articles
                }

                conn.execute(
                    r#"INSERT INTO articles (feed_id, guid, title, url, author, content, content_text, published_at)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                       ON CONFLICT(feed_id, guid) DO UPDATE SET
                           title = excluded.title,
                           url = excluded.url,
                           author = excluded.author,
                           content = excluded.content,
                           content_text = excluded.content_text,
                           published_at = excluded.published_at"#,
                    params![
                        article.feed_id,
                        article.guid,
                        article.title,
                        article.url,
                        article.author,
                        article.content,
                        article.content_text,
                        article.published_at.map(|dt| dt.to_rfc3339()),
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?;
        Ok(id)
    }

    pub async fn get_all_articles_sorted(&self) -> Result<Vec<Article>> {
        let articles = self
            .conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    r#"SELECT a.id, a.feed_id, a.guid, a.title, a.url, a.author, a.content,
                              a.content_text, a.published_at, a.fetched_at,
                              f.title as feed_title
                       FROM articles a
                       JOIN feeds f ON a.feed_id = f.id
                       ORDER BY a.published_at DESC NULLS LAST, a.fetched_at DESC"#,
                )?;
                let articles = stmt
                    .query_map([], article_from_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(articles)
            })
            .await?;
        Ok(articles)
    }

    pub async fn delete_article(&self, id: i64) -> Result<()> {
        self.conn
            .call(move |conn| {
                // Record the article's feed_id and guid before deleting (to prevent re-adding)
                conn.execute(
                    r#"INSERT OR IGNORE INTO deleted_articles (feed_id, guid)
                       SELECT feed_id, guid FROM articles WHERE id = ?1"#,
                    params![id],
                )?;
                // Delete related data first
                conn.execute("DELETE FROM summaries WHERE article_id = ?1", params![id])?;
                conn.execute(
                    "DELETE FROM saved_to_raindrop WHERE article_id = ?1",
                    params![id],
                )?;
                // Delete the article
                conn.execute("DELETE FROM articles WHERE id = ?1", params![id])?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn undelete_article(&self, feed_id: i64, guid: &str) -> Result<()> {
        let guid = guid.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM deleted_articles WHERE feed_id = ?1 AND guid = ?2",
                    params![feed_id, guid],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_old_articles(&self, days: i64) -> Result<usize> {
        let deleted = self
            .conn
            .call(move |conn| {
                // Delete summaries and raindrop entries for old articles first
                conn.execute(
                    r#"DELETE FROM summaries WHERE article_id IN (
                        SELECT id FROM articles
                        WHERE published_at < datetime('now', '-' || ?1 || ' days')
                           OR (published_at IS NULL AND fetched_at < datetime('now', '-' || ?1 || ' days'))
                    )"#,
                    params![days],
                )?;
                conn.execute(
                    r#"DELETE FROM saved_to_raindrop WHERE article_id IN (
                        SELECT id FROM articles
                        WHERE published_at < datetime('now', '-' || ?1 || ' days')
                           OR (published_at IS NULL AND fetched_at < datetime('now', '-' || ?1 || ' days'))
                    )"#,
                    params![days],
                )?;
                // Delete old articles (using published_at, fallback to fetched_at if null)
                let deleted = conn.execute(
                    r#"DELETE FROM articles
                       WHERE published_at < datetime('now', '-' || ?1 || ' days')
                          OR (published_at IS NULL AND fetched_at < datetime('now', '-' || ?1 || ' days'))"#,
                    params![days],
                )?;
                Ok(deleted)
            })
            .await?;
        Ok(deleted)
    }

    pub async fn compact_database(&self, days: i64) -> Result<usize> {
        let result = self
            .conn
            .call(move |conn| {
                // Delete old articles first
                conn.execute(
                    r#"DELETE FROM summaries WHERE article_id IN (
                        SELECT id FROM articles
                        WHERE published_at < datetime('now', '-' || ?1 || ' days')
                           OR (published_at IS NULL AND fetched_at < datetime('now', '-' || ?1 || ' days'))
                    )"#,
                    params![days],
                )?;
                conn.execute(
                    r#"DELETE FROM saved_to_raindrop WHERE article_id IN (
                        SELECT id FROM articles
                        WHERE published_at < datetime('now', '-' || ?1 || ' days')
                           OR (published_at IS NULL AND fetched_at < datetime('now', '-' || ?1 || ' days'))
                    )"#,
                    params![days],
                )?;
                let old_deleted = conn.execute(
                    r#"DELETE FROM articles
                       WHERE published_at < datetime('now', '-' || ?1 || ' days')
                          OR (published_at IS NULL AND fetched_at < datetime('now', '-' || ?1 || ' days'))"#,
                    params![days],
                )?;

                // Clean up old deleted_articles tracking entries
                conn.execute(
                    "DELETE FROM deleted_articles WHERE deleted_at < datetime('now', '-' || ?1 || ' days')",
                    params![days],
                )?;

                // Vacuum to reclaim space
                conn.execute("VACUUM", [])?;

                Ok(old_deleted)
            })
            .await?;
        Ok(result)
    }

    // Summary operations

    pub async fn get_summary(&self, article_id: i64) -> Result<Option<Summary>> {
        let summary = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, article_id, content, model_version, generated_at FROM summaries WHERE article_id = ?1",
                )?;
                let summary = stmt
                    .query_row(params![article_id], summary_from_row)
                    .optional()?;
                Ok(summary)
            })
            .await?;
        Ok(summary)
    }

    pub async fn save_summary(
        &self,
        article_id: i64,
        content: String,
        model: String,
    ) -> Result<()> {
        self.conn
            .call(move |conn| {
                conn.execute(
                    r#"INSERT INTO summaries (article_id, content, model_version)
                       VALUES (?1, ?2, ?3)
                       ON CONFLICT(article_id) DO UPDATE SET
                           content = excluded.content,
                           model_version = excluded.model_version,
                           generated_at = datetime('now')"#,
                    params![article_id, content, model],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    // Raindrop tracking

    pub async fn mark_saved_to_raindrop(
        &self,
        article_id: i64,
        raindrop_id: i64,
        tags: Vec<String>,
    ) -> Result<()> {
        let tags_json = serde_json::to_string(&tags)?;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO saved_to_raindrop (article_id, raindrop_id, tags) VALUES (?1, ?2, ?3)",
                    params![article_id, raindrop_id, tags_json],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn is_saved_to_raindrop(&self, article_id: i64) -> Result<bool> {
        let exists = self
            .conn
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM saved_to_raindrop WHERE article_id = ?1",
                    params![article_id],
                    |row| row.get(0),
                )?;
                Ok(count > 0)
            })
            .await?;
        Ok(exists)
    }
}

fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    // Try RFC3339 first (e.g., "2026-01-11T12:34:56+00:00")
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try SQLite datetime format (e.g., "2026-01-11 12:34:56")
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(naive.and_utc());
    }
    None
}

fn feed_from_row(row: &Row) -> rusqlite::Result<Feed> {
    Ok(Feed {
        id: row.get(0)?,
        title: row.get(1)?,
        url: row.get(2)?,
        site_url: row.get(3)?,
        description: row.get(4)?,
        last_fetched: row
            .get::<_, Option<String>>(5)?
            .and_then(|s| parse_datetime(&s)),
        created_at: row
            .get::<_, String>(6)
            .ok()
            .and_then(|s| parse_datetime(&s))
            .unwrap_or_else(Utc::now),
        updated_at: row
            .get::<_, String>(7)
            .ok()
            .and_then(|s| parse_datetime(&s))
            .unwrap_or_else(Utc::now),
    })
}

fn article_from_row(row: &Row) -> rusqlite::Result<Article> {
    Ok(Article {
        id: row.get(0)?,
        feed_id: row.get(1)?,
        guid: row.get(2)?,
        title: row.get(3)?,
        url: row.get(4)?,
        author: row.get(5)?,
        content: row.get(6)?,
        content_text: row.get(7)?,
        published_at: row
            .get::<_, Option<String>>(8)?
            .and_then(|s| parse_datetime(&s)),
        fetched_at: row
            .get::<_, String>(9)
            .ok()
            .and_then(|s| parse_datetime(&s))
            .unwrap_or_else(Utc::now),
        feed_title: row.get(10)?,
    })
}

fn summary_from_row(row: &Row) -> rusqlite::Result<Summary> {
    Ok(Summary {
        id: row.get(0)?,
        article_id: row.get(1)?,
        content: row.get(2)?,
        model_version: row.get(3)?,
        generated_at: row
            .get::<_, String>(4)
            .ok()
            .and_then(|s| parse_datetime(&s))
            .unwrap_or_else(Utc::now),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    struct TestRepo {
        repo: Repository,
        _tmpdir: TempDir,
    }

    async fn test_repo() -> TestRepo {
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("test.db");
        let repo = Repository::new(db_path.to_string_lossy().as_ref())
            .await
            .unwrap();
        TestRepo {
            repo,
            _tmpdir: tmpdir,
        }
    }

    #[tokio::test]
    async fn insert_and_read_article_with_summary() {
        let test = test_repo().await;
        let repo = &test.repo;
        let feed_id = repo
            .insert_feed(NewFeed {
                title: "Feed".into(),
                url: "https://example.com/rss".into(),
                site_url: Some("https://example.com".into()),
                description: Some("Example feed".into()),
            })
            .await
            .unwrap();

        repo.upsert_article(NewArticle {
            feed_id,
            guid: "guid-1".into(),
            title: "Hello".into(),
            url: "https://example.com/post".into(),
            author: Some("leo".into()),
            content: Some("<p>Hello</p>".into()),
            content_text: Some("Hello".into()),
            published_at: Some(Utc::now()),
        })
        .await
        .unwrap();

        let articles = repo.get_all_articles_sorted().await.unwrap();
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "Hello");
        assert_eq!(articles[0].feed_title.as_deref(), Some("Feed"));

        repo.save_summary(articles[0].id, "summary".into(), "claude-test".into())
            .await
            .unwrap();
        let summary = repo.get_summary(articles[0].id).await.unwrap().unwrap();
        assert_eq!(summary.content, "summary");
    }

    #[tokio::test]
    async fn deleted_article_is_not_reinserted() {
        let test = test_repo().await;
        let repo = &test.repo;
        let feed_id = repo
            .insert_feed(NewFeed {
                title: "Feed".into(),
                url: "https://example.com/rss".into(),
                site_url: None,
                description: None,
            })
            .await
            .unwrap();

        let inserted = repo
            .upsert_article(NewArticle {
                feed_id,
                guid: "guid-1".into(),
                title: "First".into(),
                url: "https://example.com/1".into(),
                author: None,
                content: None,
                content_text: None,
                published_at: None,
            })
            .await
            .unwrap();
        repo.delete_article(inserted).await.unwrap();

        let skipped = repo
            .upsert_article(NewArticle {
                feed_id,
                guid: "guid-1".into(),
                title: "Re-added".into(),
                url: "https://example.com/1".into(),
                author: None,
                content: None,
                content_text: None,
                published_at: None,
            })
            .await
            .unwrap();
        assert_eq!(skipped, 0);
        assert!(repo.get_all_articles_sorted().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn invalid_datetime_in_row_falls_back_to_now() {
        let test = test_repo().await;
        let repo = &test.repo;
        let feed_id = repo
            .insert_feed(NewFeed {
                title: "Feed".into(),
                url: "https://example.com/rss".into(),
                site_url: None,
                description: None,
            })
            .await
            .unwrap();

        let article_id = repo
            .upsert_article(NewArticle {
                feed_id,
                guid: "guid-2".into(),
                title: "Date test".into(),
                url: "https://example.com/date".into(),
                author: None,
                content: None,
                content_text: None,
                published_at: None,
            })
            .await
            .unwrap();

        repo.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE articles SET fetched_at = 'not-a-datetime' WHERE id = ?1",
                    params![article_id],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        let article = repo.get_all_articles_sorted().await.unwrap().remove(0);
        assert!(article.fetched_at > Utc::now() - Duration::minutes(1));
    }
}
