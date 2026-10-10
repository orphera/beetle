use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::gameplay::queue_start_gameplay;
use crate::handlers::options::{change_option, close_options, move_option_row, open_options};
use crate::state::{AppScreen, AppState};

/// Handles keyboard input on the Stage Result screen. While the play options
/// panel is open over it, the panel takes the keys.
pub fn handle_result_input(state: &mut AppState, key_state: ElementState, code: KeyCode) {
    if key_state != ElementState::Pressed {
        return;
    }

    if state.show_option_modal {
        handle_result_options_input(state, code);
        return;
    }

    match code {
        KeyCode::Enter | KeyCode::Space | KeyCode::Escape => to_song_select(state),
        KeyCode::KeyR => retry_song(state),
        KeyCode::Tab => open_options(state),
        KeyCode::KeyP => take_screenshot(state),
        _ => (),
    }
}

/// Keys while the play options panel is open over the result. ENTER plays
/// again with the options as changed; TAB and ESC only close the panel.
/// Space steps a value forward (ENTER is the start here, not a step).
pub fn handle_result_options_input(state: &mut AppState, code: KeyCode) {
    let row = state.modal_row;
    match code {
        KeyCode::Tab | KeyCode::Escape => close_options(state),
        KeyCode::Enter => retry_with_options(state),
        KeyCode::ArrowUp | KeyCode::KeyK => move_option_row(state, false),
        KeyCode::ArrowDown | KeyCode::KeyJ => move_option_row(state, true),
        KeyCode::ArrowLeft => change_option(state, row, false),
        KeyCode::ArrowRight | KeyCode::Space => change_option(state, row, true),
        _ => (),
    }
}

/// Back to the song list (ENTER, ESC, or the footer button).
pub fn to_song_select(state: &mut AppState) {
    state.screen = AppScreen::SongSelect;
}

/// Plays the song just played again (R, or the footer button).
pub fn retry_song(state: &mut AppState) {
    if let Some(song) = state.current_selected_song().cloned() {
        queue_start_gameplay(state, &song);
    }
}

/// Closes the play options panel (saving the values) and plays the song again
/// with them (ENTER or the start button in the panel). The gauge, modifier
/// and the other play options are read when the new play starts.
pub fn retry_with_options(state: &mut AppState) {
    close_options(state);
    retry_song(state);
}

/// Saves a screenshot of the next frame (P, or the footer button).
pub fn take_screenshot(state: &mut AppState) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("screenshots/result_{}.bmp", timestamp);
    // Taken from the backbuffer when the next frame is drawn.
    state.pending_screenshot = Some(path);
    state.window.request_redraw();
}
