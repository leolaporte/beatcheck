use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum AppAction {
    Quit,
    MoveUp,
    MoveDown,
    SelectArticle,
    RefreshFeeds,
    ToggleStarred,
    ToggleRead,
    OpenInBrowser,
    SaveToRaindrop,
    CycleFilter,
    RegenerateSummary,
    #[allow(dead_code)]
    ImportOpml(PathBuf),
    ShowHelp,
    HideHelp,
    // Tag input actions
    TagInputChar(char),
    TagInputBackspace,
    TagInputConfirm,
    TagInputCancel,
}

pub fn handle_key_event(key: KeyEvent, tag_input_active: bool, show_help: bool) -> Option<AppAction> {
    // If help is showing, any key closes it
    if show_help {
        return Some(AppAction::HideHelp);
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

    // Normal mode
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => Some(AppAction::Quit),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(AppAction::Quit),

        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Some(AppAction::MoveDown),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Some(AppAction::MoveUp),

        (KeyCode::Enter, _) => Some(AppAction::SelectArticle),

        (KeyCode::Char('r'), _) => Some(AppAction::RefreshFeeds),
        (KeyCode::Char('s'), _) => Some(AppAction::ToggleStarred),
        (KeyCode::Char('m'), _) => Some(AppAction::ToggleRead),
        (KeyCode::Char('o'), _) => Some(AppAction::OpenInBrowser),
        (KeyCode::Char('S'), _) => Some(AppAction::SaveToRaindrop),
        (KeyCode::Char('f'), _) => Some(AppAction::CycleFilter),
        (KeyCode::Char('g'), _) => Some(AppAction::RegenerateSummary),

        (KeyCode::Char('?'), _) => Some(AppAction::ShowHelp),

        _ => None,
    }
}
