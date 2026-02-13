# Smart Brevity Summary Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace BeatCheck's 3-5 bullet point summaries with Axios-style Smart Brevity format (Editorial/Product auto-detect).

**Architecture:** Prompt-only change. The AI outputs human-readable Smart Brevity text stored as a plain `String`. No structural changes to the Summary model, database, or UI. The excerpt cleaner is updated to strip Smart Brevity prefixes for Raindrop bookmarks.

**Tech Stack:** Rust, Claude API (Haiku 3.5), tokio-rusqlite, ratatui

**Design doc:** `docs/plans/2026-02-12-smart-brevity-design.md`

---

### Task 1: Update the AI prompt to Smart Brevity format

**Files:**
- Modify: `src/ai/summarizer.rs:52-86` (the `generate_summary` method)

**Step 1: Replace the system prompt and user message**

The current code at lines 57-76 uses a bullet-point system prompt and a separate user message. Replace both with a single user message containing the Smart Brevity prompt (adapted from `~/Projects/briefing/crates/shared/src/summarizer.rs:124-155`).

The key difference from `briefing`: instead of asking for machine-parseable `KEY: value` lines, ask the AI to output human-readable text with label prefixes like `What's happening:`.

```rust
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

    // ... rest of the method unchanged
```

Note: `system` changes from `Some(...)` to `None` since the full prompt is now in the user message.

**Step 2: Verify it compiles**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles with no errors.

**Step 3: Commit**

```bash
git add src/ai/summarizer.rs
git commit -m "feat: replace bullet summaries with Smart Brevity format"
```

---

### Task 2: Update excerpt cleaner for Smart Brevity prefixes

**Files:**
- Modify: `src/app.rs:866-899` (the `clean_summary_for_excerpt` method)

**Step 1: Replace the prefix list and extraction logic**

The current `clean_summary_for_excerpt` strips old "here's the summary" prefixes. Replace with Smart Brevity prefix stripping. The excerpt should be the first meaningful content line (the "What's happening" or "The product" value).

```rust
fn clean_summary_for_excerpt(summary: &str) -> String {
    // Smart Brevity label prefixes to strip
    let prefixes = [
        "what's happening:",
        "the product:",
        "why it matters:",
        "the big picture:",
        "cost:",
        "availability:",
        "platforms:",
        // Legacy bullet-point format
        "summary:",
        "here's the summary:",
        "here's a summary:",
        "here is the summary:",
        "here is a summary:",
        "here's the summary of the article:",
        "here's a summary of the article:",
        "here is the summary of the article:",
        "here is a summary of the article:",
    ];

    // Get first non-empty line
    let first_line = summary
        .lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .unwrap_or("");

    // Strip label prefix if present
    let mut text = first_line.to_string();
    let text_lower = text.to_lowercase();
    for prefix in &prefixes {
        if text_lower.starts_with(prefix) {
            text = text[prefix.len()..].trim_start().to_string();
            break;
        }
    }

    // Also strip bullet prefix from legacy format
    if text.starts_with("• ") {
        text = text[4..].to_string();
    }

    Self::get_first_sentence(&text)
}
```

**Step 2: Verify it compiles**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles with no errors.

**Step 3: Commit**

```bash
git add src/app.rs
git commit -m "fix: update excerpt cleaner for Smart Brevity prefixes"
```

---

### Task 3: Manual smoke test

**Step 1: Run BeatCheck and generate a summary**

Run: `cargo run`

1. Select an article
2. Press Enter to generate a summary
3. Verify the summary appears in Smart Brevity format (starts with "What's happening:" or "The product:")
4. Verify spinner shows during generation
5. Press `g` to regenerate and confirm it works again

**Step 2: Test Raindrop bookmark excerpt**

1. Select a summarized article
2. Press `b` to bookmark
3. Verify the excerpt in Raindrop.io is a clean sentence (no "What's happening:" prefix)

**Step 3: Verify cached summary still displays**

1. Select an article that already has a cached bullet-point summary
2. Verify it displays without errors (old format is still valid text)
