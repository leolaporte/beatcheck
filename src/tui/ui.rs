use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::models::SummaryStatus;

pub fn draw(frame: &mut Frame, app: &App) {
    // Main horizontal split: 1/3 left, 2/3 right
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3), // Left pane: article list
            Constraint::Ratio(2, 3), // Right pane: summary
        ])
        .split(frame.area());

    // Left pane: header + article list + status
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(0),    // Article list
            Constraint::Length(1), // Status line
        ])
        .split(main_chunks[0]);

    // Right pane: title + summary content + status
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Article title
            Constraint::Min(0),    // Summary content
            Constraint::Length(1), // Status (generating/cached)
        ])
        .split(main_chunks[1]);

    // Render left pane
    render_header(frame, app, left_chunks[0]);
    render_article_list(frame, app, left_chunks[1]);
    render_left_status(frame, app, left_chunks[2]);

    // Render right pane
    render_article_title(frame, app, right_chunks[0]);
    render_summary(frame, app, right_chunks[1]);
    render_right_status(frame, app, right_chunks[2]);

    // Render tag input popup if active
    if app.tag_input_active {
        render_tag_input(frame, app);
    }

    // Render help popup if active
    if app.show_help {
        render_help(frame);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let filter_label = app.filter.label();
    let title = format!(" RSS Reader [{filter_label}] ");
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, area);
}

fn render_article_list(frame: &mut Frame, app: &App, area: Rect) {
    let articles = app.filtered_articles();

    let items: Vec<ListItem> = articles
        .iter()
        .map(|article| {
            let style = if article.is_read {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let star = if article.is_starred { "★ " } else { "  " };
            let feed = article
                .feed_title
                .as_deref()
                .unwrap_or("Unknown");
            let title = &article.title;

            let line = Line::from(vec![
                Span::styled(star, Style::default().fg(Color::Yellow)),
                Span::styled(format!("[{feed}] "), Style::default().fg(Color::Blue)),
                Span::styled(title, style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index));

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_left_status(frame: &mut Frame, app: &App, area: Rect) {
    let status = if app.is_refreshing {
        "Refreshing feeds..."
    } else {
        "j/k:nav  r:refresh  s:star  ?:help  q:quit"
    };

    let paragraph = Paragraph::new(status).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}

fn render_article_title(frame: &mut Frame, app: &App, area: Rect) {
    let title = app
        .selected_article()
        .map(|a| a.title.as_str())
        .unwrap_or("No article selected");

    let block = Block::default()
        .title(" Article ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let paragraph = Paragraph::new(title)
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_summary(frame: &mut Frame, app: &App, area: Rect) {
    let content = match app.summary_status {
        SummaryStatus::NotGenerated => "Press Enter to generate summary...".to_string(),
        SummaryStatus::Generating => "Generating summary...".to_string(),
        SummaryStatus::Failed => "Failed to generate summary. Press 'g' to retry.".to_string(),
        SummaryStatus::Generated => app
            .current_summary
            .as_ref()
            .map(|s| s.content.clone())
            .unwrap_or_else(|| "No summary available".to_string()),
    };

    let block = Block::default()
        .title(" Summary ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_right_status(frame: &mut Frame, app: &App, area: Rect) {
    let status = match app.summary_status {
        SummaryStatus::NotGenerated => "",
        SummaryStatus::Generating => "⏳ Generating...",
        SummaryStatus::Failed => "❌ Failed",
        SummaryStatus::Generated => "✓ Cached",
    };

    let raindrop_status = if app.selected_article().map(|a| a.id).is_some() {
        if app.is_saved_to_raindrop {
            " | 💧 Saved"
        } else {
            " | S:save to Raindrop"
        }
    } else {
        ""
    };

    let text = format!("{status}{raindrop_status}");
    let paragraph = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}

fn render_tag_input(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, frame.area());

    let block = Block::default()
        .title(" Save to Raindrop.io - Enter tags (comma separated) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);

    // Clear the area first
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let input_text = format!("> {}_", app.tag_input);
    let paragraph = Paragraph::new(input_text).style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, inner);
}

fn render_help(frame: &mut Frame) {
    let area = centered_rect(50, 60, frame.area());

    let help_text = vec![
        "",
        " Navigation:",
        "   j / ↓    Move down",
        "   k / ↑    Move up",
        "   Enter    Select / Generate summary",
        "",
        " Actions:",
        "   r        Refresh all feeds",
        "   s        Toggle starred",
        "   m        Toggle read/unread",
        "   o        Open in browser",
        "   S        Save to Raindrop.io",
        "   g        Regenerate summary",
        "   f        Cycle filter",
        "   i        Import OPML",
        "",
        " General:",
        "   ?        Toggle this help",
        "   q        Quit",
        "",
        " Press any key to close",
    ];

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(help_text.join("\n"))
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
