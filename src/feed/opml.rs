use opml::{Outline, OPML};
use std::path::Path;

use crate::error::{AppError, Result};
use crate::models::NewFeed;

pub fn parse_opml_file(path: &Path) -> Result<Vec<NewFeed>> {
    let content = std::fs::read_to_string(path)?;
    let opml = OPML::from_str(&content).map_err(|e| AppError::OpmlParse(e.to_string()))?;

    let mut feeds = Vec::new();
    collect_feeds(&opml.body.outlines, &mut feeds);

    Ok(feeds)
}

fn collect_feeds(outlines: &[Outline], feeds: &mut Vec<NewFeed>) {
    for outline in outlines {
        // Check if this outline is a feed (has xmlUrl)
        if let Some(xml_url) = &outline.xml_url {
            feeds.push(NewFeed {
                title: outline.text.clone(),
                url: xml_url.clone(),
                site_url: outline.html_url.clone(),
                description: outline.description.clone(),
            });
        }

        // Recursively process nested outlines (categories/folders)
        if !outline.outlines.is_empty() {
            collect_feeds(&outline.outlines, feeds);
        }
    }
}
