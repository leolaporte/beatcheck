# Speedy Reader (RSS Reader)

Central config: `~/.claude/CLAUDE.md`
Rust rules: `~/.claude/rules/rust-projects.md`

## Build & Run

```bash
cargo build              # Dev build
cargo build --release    # Release build
cargo run                # Run TUI
cargo run -- --refresh   # Headless refresh
cargo run -- --import feeds.opml  # Import OPML
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt                # Format
```

## Architecture

- **Async TUI**: tokio + ratatui with non-blocking operations
- **Database**: SQLite via tokio-rusqlite (7-day retention, auto-compaction)
- **Config**: TOML at `~/.config/speedy-reader/config.toml`

### Modules

| Module | Purpose |
|--------|---------|
| `app.rs` | Central state, async channels |
| `tui/ui.rs` | Rendering (split-pane layout) |
| `tui/handler.rs` | Key bindings |
| `db/repository.rs` | Database operations |
| `feed/fetcher.rs` | RSS/Atom fetching, auto-discovery |
| `feed/opml.rs` | Import/export |
| `ai/summarizer.rs` | Claude API for summaries |
| `services/raindrop.rs` | Raindrop.io bookmarking |

### Async Pattern

1. Action triggers `tokio::spawn`
2. Task sends result via channel (`summary_tx`, `refresh_tx`, `discovery_tx`)
3. Main loop polls channels every 100ms
4. Results update state and trigger redraw

## Filter Modes

`f` key cycles: Unread -> Starred -> All

## Spinners

Braille animation: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`
