# Smart Brevity Summary Format for BeatCheck

## What We're Changing

Replace the current 3-5 bullet point AI summaries with Axios-style Smart Brevity format. The AI auto-detects whether an article is editorial (news/analysis) or product-focused, and outputs the appropriate format as readable plain text.

## Format

### Editorial (news, policy, analysis)

```
What's happening: One strong lede sentence capturing the core news.
Why it matters: 1-2 sentences on significance.
The big picture: Broader implications (omitted if article is too narrow).
"Quote text" -- Speaker Name (omitted if no quotes in article).
```

### Product (hardware, software, apps)

```
The product: What it is and what it does.
Cost: Pricing details (omitted if not mentioned).
Availability: When/where (omitted if not mentioned).
Platforms: What it runs on (omitted for hardware or if not mentioned).
"Quote text" -- Speaker Name (omitted if no quotes).
```

## Scope

Two files changed:

1. **`src/ai/summarizer.rs`** — Replace system prompt with Smart Brevity prompt (adapted from `briefing` project). AI outputs human-readable text, not machine-parseable keys.

2. **`src/app.rs`** — Update `clean_summary_for_excerpt()` to strip Smart Brevity prefixes for clean Raindrop.io excerpts.

## What Stays the Same

- `Summary` model (`content: String`) — no structural change
- Database schema — no migration
- TUI rendering — displays the string as-is
- Async channel plumbing
- Cached summaries — old bullet format remains in DB, still displays fine

## Reference

Ported from `~/Projects/briefing/` which already uses this format in production.
