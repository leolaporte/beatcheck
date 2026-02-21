use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

mod ai;
mod app;
mod blocklist;
mod config;
mod db;
mod error;
mod feed;
mod models;
mod services;
mod tui;

use app::App;
use config::Config;
use error::Result;
use tui::{draw, handle_key_event};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (only show warnings and errors by default)
    // Filter out html5ever warnings which corrupt the TUI display
    // Also write to /tmp/beatcheck-errors.log
    use std::fs::OpenOptions;
    use std::io::Write;
    use tracing_subscriber::fmt::MakeWriter;

    let log_file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/beatcheck-errors.log")
    {
        Ok(file) => Some(std::sync::Arc::new(std::sync::Mutex::new(file))),
        Err(err) => {
            eprintln!("Warning: unable to open /tmp/beatcheck-errors.log: {err}");
            None
        }
    };

    struct DualWriter {
        file: Option<std::sync::Arc<std::sync::Mutex<std::fs::File>>>,
    }

    impl Write for DualWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if let Some(file) = &self.file {
                if let Ok(mut file) = file.lock() {
                    let _ = file.write_all(buf);
                }
            }
            std::io::stderr().write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            if let Some(file) = &self.file {
                if let Ok(mut file) = file.lock() {
                    let _ = file.flush();
                }
            }
            std::io::stderr().flush()
        }
    }

    impl<'a> MakeWriter<'a> for DualWriter {
        type Writer = DualWriter;

        fn make_writer(&'a self) -> Self::Writer {
            DualWriter {
                file: self.file.clone(),
            }
        }
    }

    let dual_writer = DualWriter { file: log_file };

    let mut env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(tracing::Level::WARN.into());
    if let Ok(directive) = "html5ever=error".parse() {
        env_filter = env_filter.add_directive(directive);
    }

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(dual_writer)
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Load configuration
    let config = Config::load()?;

    // Check for --import flag
    let import_path = if args.len() >= 3 && args[1] == "--import" {
        Some(PathBuf::from(&args[2]))
    } else {
        None
    };

    // Check for --refresh flag (headless refresh)
    let headless_refresh = args.len() >= 2 && args[1] == "--refresh";

    // Initialize app
    let mut app = App::new(&config).await?;

    // If import path provided, import OPML and exit
    if let Some(path) = import_path {
        app.import_opml(&path).await?;
        println!("Imported feeds from {:?}", path);
        return Ok(());
    }

    // If headless refresh, just refresh and exit
    if headless_refresh {
        app.refresh_feeds_blocking().await?;
        println!("Refreshed {} feeds", app.feeds.len());
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let result = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        // Advance spinner animation
        app.tick_spinner();

        // Poll for completed summary results
        app.poll_summary_result().await?;

        // Poll for completed refresh results
        app.poll_refresh_result().await?;

        // Poll for completed feed discovery results
        app.poll_discovery_result().await?;

        // Clear bookmark status after timeout
        app.check_bookmark_status_timeout();

        // Poll for events with timeout to allow async operations
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = handle_key_event(
                    key,
                    app.tag_input_active,
                    app.feed_input_active,
                    app.opml_input_active,
                    app.opml_export_active,
                    app.show_help,
                    app.bookmark_prefix_active,
                ) {
                    let should_quit = app.handle_action(action).await?;
                    if should_quit {
                        return Ok(());
                    }
                }
            }
        }
    }
}
