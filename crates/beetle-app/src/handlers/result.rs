use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::gameplay::queue_start_gameplay;
use crate::state::{AppScreen, AppState};

/// Handles keyboard input on the Stage Result screen.
pub fn handle_result_input(state: &mut AppState, key_state: ElementState, code: KeyCode) {
    if key_state != ElementState::Pressed {
        return;
    }

    match code {
        KeyCode::Enter | KeyCode::Space | KeyCode::Escape => to_song_select(state),
        KeyCode::KeyR => retry_song(state),
        KeyCode::KeyP => take_screenshot(state),
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
