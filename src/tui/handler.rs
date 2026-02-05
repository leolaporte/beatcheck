use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub enum AppAction {
    Quit,
    MoveUp,
    MoveDown,
    MoveToTop,
    MoveToBottom,
    SelectArticle,
    RefreshFeeds,
    OpenInBrowser,
    EmailArticle,
    SaveToRaindrop,
    SaveToRaindropWithTag(String), // Quick bookmark with preset tag
    RegenerateSummary,
    DeleteArticle,
    DeleteFeed,
    UndeleteArticle,
    AddFeed,
    ShowHelp,
    HideHelp,
    // Tag input actions
    TagInputChar(char),
    TagInputBackspace,
    TagInputConfirm,
    TagInputCancel,
    // Feed input actions
    FeedInputChar(char),
    FeedInputBackspace,
    FeedInputConfirm,
    FeedInputCancel,
    // OPML input actions
    ImportOpmlStart,
    OpmlInputChar(char),
    OpmlInputBackspace,
    OpmlInputConfirm,
    OpmlInputCancel,
    // OPML export actions
    ExportOpmlStart,
    OpmlExportChar(char),
    OpmlExportBackspace,
    OpmlExportConfirm,
    OpmlExportCancel,
    // Space prefix mode for quick bookmarks
    BookmarkPrefixStart,
    CancelBookmarkPrefix,
}

pub fn handle_key_event(
    key: KeyEvent,
    tag_input_active: bool,
    feed_input_active: bool,
    opml_input_active: bool,
    opml_export_active: bool,
    show_help: bool,
    bookmark_prefix_active: bool,
) -> Option<AppAction> {
    // If help is showing, any key closes it
    if show_help {
        return Some(AppAction::HideHelp);
    }

    // Space prefix mode (waiting for second key after Space)
    if bookmark_prefix_active {
        return match key.code {
            KeyCode::Char('t') => Some(AppAction::SaveToRaindropWithTag("twit".to_string())),
            KeyCode::Char('i') => Some(AppAction::SaveToRaindropWithTag("im".to_string())),
            KeyCode::Char('m') => Some(AppAction::SaveToRaindropWithTag("mbw".to_string())),
            KeyCode::Esc => Some(AppAction::CancelBookmarkPrefix),
            _ => Some(AppAction::CancelBookmarkPrefix), // Any other key cancels
        };
    }

    // Tag input mode
    if tag_input_active {
        return match key.code {
            KeyCode::Enter => Some(AppAction::TagInputConfirm),
            KeyCode::Esc => Some(AppAction::TagInputCancel),
            KeyCode::Backspace => Some(AppAction::TagInputBackspace),
            KeyCode::Char(c) => Some(AppAction::TagInputChar(c)),
            _ => None,
        };
    }

    // Feed input mode
    if feed_input_active {
        return match key.code {
            KeyCode::Enter => Some(AppAction::FeedInputConfirm),
            KeyCode::Esc => Some(AppAction::FeedInputCancel),
            KeyCode::Backspace => Some(AppAction::FeedInputBackspace),
            KeyCode::Char(c) => Some(AppAction::FeedInputChar(c)),
            _ => None,
        };
    }

    // OPML import input mode
    if opml_input_active {
        return match key.code {
            KeyCode::Enter => Some(AppAction::OpmlInputConfirm),
            KeyCode::Esc => Some(AppAction::OpmlInputCancel),
            KeyCode::Backspace => Some(AppAction::OpmlInputBackspace),
            KeyCode::Char(c) => Some(AppAction::OpmlInputChar(c)),
            _ => None,
        };
    }

    // OPML export input mode
    if opml_export_active {
        return match key.code {
            KeyCode::Enter => Some(AppAction::OpmlExportConfirm),
            KeyCode::Esc => Some(AppAction::OpmlExportCancel),
            KeyCode::Backspace => Some(AppAction::OpmlExportBackspace),
            KeyCode::Char(c) => Some(AppAction::OpmlExportChar(c)),
            _ => None,
        };
    }

    // Normal mode
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => Some(AppAction::Quit),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(AppAction::Quit),

        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Some(AppAction::MoveDown),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Some(AppAction::MoveUp),
        (KeyCode::Char('<'), _) => Some(AppAction::MoveToTop),
        (KeyCode::Char('>'), _) => Some(AppAction::MoveToBottom),

        (KeyCode::Enter, _) => Some(AppAction::SelectArticle),

        (KeyCode::Char('r'), _) => Some(AppAction::RefreshFeeds),
        (KeyCode::Char('o'), _) => Some(AppAction::OpenInBrowser),
        (KeyCode::Char('e'), _) => Some(AppAction::EmailArticle),
        (KeyCode::Char('b'), _) => Some(AppAction::SaveToRaindrop),
        (KeyCode::Char(' '), _) => Some(AppAction::BookmarkPrefixStart),
        (KeyCode::Char('g'), _) => Some(AppAction::RegenerateSummary),
        (KeyCode::Char('d'), KeyModifiers::NONE) | (KeyCode::Backspace, _) => Some(AppAction::DeleteArticle),
        (KeyCode::Char('D'), KeyModifiers::SHIFT) => Some(AppAction::DeleteFeed),
        (KeyCode::Char('u'), _) => Some(AppAction::UndeleteArticle),
        (KeyCode::Char('a'), _) => Some(AppAction::AddFeed),
        (KeyCode::Char('i'), _) => Some(AppAction::ImportOpmlStart),
        (KeyCode::Char('w'), _) => Some(AppAction::ExportOpmlStart),

        (KeyCode::Char('?'), _) => Some(AppAction::ShowHelp),

        _ => None,
    }
}
