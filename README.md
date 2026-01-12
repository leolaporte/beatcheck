# rss-reader

A terminal-based RSS reader with AI-powered article summaries.

I built this for my personal use on a Sunday morning entirely vibe coded with [Claude Code](https://claude.ai/code).

## Features

- Two-pane TUI interface (article list + AI summary)
- Claude API integration for intelligent article summaries
- Raindrop.io integration for bookmarking
- OPML import for feed subscriptions
- SQLite caching for offline reading
- Auto-mark articles as read after 2 seconds

## Installation

### From Release Binary

Download the latest release for your platform from the [Releases](https://github.com/leolaporte/rss-reader/releases) page.

```bash
# Linux x86_64
curl -LO https://github.com/leolaporte/rss-reader/releases/latest/download/rss-reader-linux-x86_64.tar.gz
tar -xzf rss-reader-linux-x86_64.tar.gz -C ~/.local/bin/

# macOS Apple Silicon
curl -LO https://github.com/leolaporte/rss-reader/releases/latest/download/rss-reader-macos-aarch64.tar.gz
tar -xzf rss-reader-macos-aarch64.tar.gz -C ~/.local/bin/

# macOS Intel
curl -LO https://github.com/leolaporte/rss-reader/releases/latest/download/rss-reader-macos-x86_64.tar.gz
tar -xzf rss-reader-macos-x86_64.tar.gz -C ~/.local/bin/
```

### From Source

Requires Rust 1.70+:

```bash
git clone https://github.com/leolaporte/rss-reader.git
cd rss-reader
cargo install --path .
```

## Configuration

Create `~/.config/rss-reader/config.toml`:

```toml
# Required for AI summaries
claude_api_key = "sk-ant-..."

# Optional: Raindrop.io integration
raindrop_token = "..."
```

## Usage

```bash
# Run the TUI
rss-reader

# Import OPML subscriptions
rss-reader --import feeds.opml

# Headless refresh (for cron/systemd)
rss-reader --refresh
```

### Key Bindings

| Key | Action |
|-----|--------|
| `j`/`k` or `↓`/`↑` | Navigate articles |
| `Enter` | Generate/show summary |
| `r` | Refresh all feeds |
| `s` | Toggle starred |
| `m` | Toggle read/unread |
| `o` | Open in browser |
| `S` | Save to Raindrop.io |
| `f` | Cycle filter (Unread/Starred/All) |
| `g` | Regenerate summary |
| `?` | Show help |
| `q` | Quit |

## Systemd Timer (Auto-refresh)

To refresh feeds automatically every hour:

```bash
# Copy service files
mkdir -p ~/.config/systemd/user
cp systemd/*.{service,timer} ~/.config/systemd/user/

# Enable timer
systemctl --user enable --now rss-reader-refresh.timer
```

## License

MIT
