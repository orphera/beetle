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
        KeyCode::Enter | KeyCode::Space | KeyCode::Escape => {
            state.screen = AppScreen::SongSelect;
        }
        KeyCode::KeyR => {
            if let Some(song) = state.current_selected_song().cloned() {
                queue_start_gameplay(state, &song);
            }
        }
        KeyCode::KeyP => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let path = format!("screenshots/result_{}.bmp", timestamp);
            let _ = state.renderer.save_screenshot(&path);
        }
        _ => (),
    }
}
