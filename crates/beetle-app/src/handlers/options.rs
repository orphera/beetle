use winit::keyboard::KeyCode;

use crate::handlers::key_config::open_key_config_from;
use crate::options_table::{self, OptionDesc, PLAY_OPTIONS};
use crate::state::{AppScreen, AppState};

/// Opens the play options panel (TAB, O, or the OPTIONS button).
pub fn open_options(state: &mut AppState) {
    state.show_option_modal = true;
    state.modal_row = 0;
}

/// Closes the play options panel and saves the options.
pub fn close_options(state: &mut AppState) {
    state.show_option_modal = false;
    state.save_config();
}

/// Moves the cursor of a list one row down or up (no wrapping).
pub fn move_row(row: &mut usize, len: usize, down: bool) {
    *row = if down {
        (*row + 1).min(len.saturating_sub(1))
    } else {
        row.saturating_sub(1)
    };
}

/// Moves the highlighted row of the play options panel.
pub fn move_option_row(state: &mut AppState, down: bool) {
    move_row(&mut state.modal_row, PLAY_OPTIONS.len(), down);
}

/// Handles keyboard input when the play options panel is open.
pub fn handle_option_modal_input(state: &mut AppState, code: KeyCode) {
    let row = state.modal_row;
    match code {
        KeyCode::Tab | KeyCode::Escape => close_options(state),
        KeyCode::ArrowUp | KeyCode::KeyK => move_option_row(state, false),
        KeyCode::ArrowDown | KeyCode::KeyJ => move_option_row(state, true),
        KeyCode::ArrowLeft => change_option(state, row, false),
        KeyCode::ArrowRight => change_option(state, row, true),
        KeyCode::Enter | KeyCode::Space => change_option(state, row, true),
        _ => (),
    }
}

/// Changes the value of row `row` of the play options one step and saves.
pub fn change_option(state: &mut AppState, row: usize, forward: bool) {
    if let Some(desc) = PLAY_OPTIONS.get(row) {
        options_table::step(state, desc.id, forward);
        state.save_config();
    }
}

/// ENTER on a row of `table` (in `screen`): the row's activation if it has
/// one (the key layout opens Key Config, and Back returns to `screen`), else
/// a step forward. Returns whether it opened something.
pub fn enter_row(
    state: &mut AppState,
    table: &[OptionDesc],
    row: usize,
    screen: AppScreen,
) -> bool {
    let Some(desc) = table.get(row) else {
        return false;
    };
    match options_table::activation(desc.id) {
        Some(options_table::Activation::KeyConfig) => {
            state.save_config();
            open_key_config_from(state, screen);
            true
        }
        None => {
            options_table::step(state, desc.id, true);
            state.save_config();
            false
        }
    }
}
