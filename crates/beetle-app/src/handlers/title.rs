//! The title screen: a menu of four entries (start, settings, key
//! configuration, quit). Keys and clicks run the same actions. Escape asks to
//! quit, as it does on the song select root.

use beetle_audio::UiSound;
use beetle_render::HitId;
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::handlers::key_config::open_key_config_from;
use crate::handlers::options::move_row;
use crate::handlers::settings::open_settings;
use crate::handlers::song_select::open_exit_prompt;
use crate::menu_audio::play_ui;
use crate::state::{AppScreen, AppState};

/// What each entry of the title menu does, top to bottom. Its labels are the
/// render crate's `title::MENU_LABELS`, in the same order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleAction {
    /// Opens song select.
    Start,
    Settings,
    KeyConfig,
    /// Asks to quit (the quit prompt), like Escape.
    Quit,
}

pub const TITLE_ACTIONS: [TitleAction; 4] = [
    TitleAction::Start,
    TitleAction::Settings,
    TitleAction::KeyConfig,
    TitleAction::Quit,
];

/// Handles keyboard input on the title screen.
pub fn handle_title_input(state: &mut AppState, key_state: ElementState, code: KeyCode) {
    if key_state != ElementState::Pressed {
        return;
    }
    if state.show_exit_modal {
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::KeyY => {
                state.should_exit_app = true;
            }
            KeyCode::Escape | KeyCode::KeyN => {
                play_ui(state, UiSound::Close);
                state.show_exit_modal = false;
            }
            _ => (),
        }
        return;
    }
    match code {
        KeyCode::ArrowUp | KeyCode::KeyK => move_title_row(state, false),
        KeyCode::ArrowDown | KeyCode::KeyJ => move_title_row(state, true),
        KeyCode::Enter | KeyCode::NumpadEnter => activate(state, state.title_row),
        KeyCode::Escape => open_exit_prompt(state),
        _ => (),
    }
}

/// A click on the title screen. A click on an entry highlights it and runs it.
pub fn title_click(state: &mut AppState, id: HitId) {
    if state.show_exit_modal {
        match id {
            HitId::ExitQuit => state.should_exit_app = true,
            HitId::ExitCancel | HitId::Blocker => {
                play_ui(state, UiSound::Close);
                state.show_exit_modal = false;
            }
            _ => (),
        }
        return;
    }
    if let HitId::TitleItem(i) = id {
        state.title_row = i;
        activate(state, i);
    }
}

/// Moves the highlight one entry down or up (clamped, as the other menus do).
fn move_title_row(state: &mut AppState, down: bool) {
    play_ui(state, UiSound::Move);
    move_row(&mut state.title_row, TITLE_ACTIONS.len(), down);
}

/// Runs entry `row` of the title menu.
fn activate(state: &mut AppState, row: usize) {
    let Some(action) = TITLE_ACTIONS.get(row).copied() else {
        return;
    };
    match action {
        TitleAction::Start => {
            play_ui(state, UiSound::Confirm);
            state.screen = AppScreen::SongSelect;
        }
        TitleAction::Settings => open_settings(state),
        TitleAction::KeyConfig => open_key_config_from(state, AppScreen::Title),
        TitleAction::Quit => open_exit_prompt(state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_follow_the_rendered_menu_order() {
        assert_eq!(
            TITLE_ACTIONS,
            [
                TitleAction::Start,
                TitleAction::Settings,
                TitleAction::KeyConfig,
                TitleAction::Quit,
            ]
        );
        assert_eq!(TITLE_ACTIONS.len(), beetle_render::TITLE_MENU_LABELS.len());
    }
}
