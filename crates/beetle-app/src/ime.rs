//! Text input for the song select search box. Typed characters come from
//! `KeyEvent::text`; Korean and Japanese come from the OS input method, which
//! is switched on only while the search box is open (gameplay reads physical
//! keys and must never see an IME).

use std::time::Instant;

use beetle_render::Rect;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::Ime;

use crate::state::{AppScreen, AppState};

/// Opens or closes the search box. Every path that changes the search state
/// goes through here, so the IME is on exactly while the box is open.
pub fn set_search_active(state: &mut AppState, active: bool) {
    if state.is_search_active == active {
        return;
    }
    state.is_search_active = active;
    if !active {
        state.search_preedit.clear();
        state.ime_caret_sent = None;
    }
    state.window.set_ime_allowed(active);
}

/// Whether the search box may stay open on this screen (it closes itself
/// when the player leaves song select or opens a modal).
pub fn search_allowed(screen: AppScreen, option_modal: bool, exit_modal: bool) -> bool {
    screen == AppScreen::SongSelect && !option_modal && !exit_modal
}

/// Closes the search box if the screen or a modal no longer allows it.
pub fn close_search_if_unavailable(state: &mut AppState) {
    if state.is_search_active
        && !search_allowed(state.screen, state.show_option_modal, state.show_exit_modal)
    {
        set_search_active(state, false);
    }
}

/// Text from a key press that goes into the query.
///
/// Double input: `KeyEvent::text` comes from `WM_CHAR`, and composed text
/// reaches the app as `Ime::Commit`. winit (0.30, Windows) answers
/// `WM_IME_COMPOSITION` without `DefWindowProc`, so the composition itself
/// does not become `WM_CHAR`. An IME can still deliver a committed character
/// as `WM_IME_CHAR`, and `DefWindowProc` turns that into `WM_CHAR`. So with the
/// IME on, non-ASCII key text is dropped and `Commit` is the only source of
/// Hangul and Kana. ASCII key text is kept (English-mode typing has no
/// composition). While a composition is open, the key belongs to the IME, so
/// its text is dropped too. Live check: a posted non-ASCII `WM_CHAR` and a raw
/// `WM_IME_CHAR` both left the query unchanged. A real IME was not available
/// to test, so the `Commit` path is unverified (see the U1c note in the plan).
pub fn key_text_to_append(text: &str, preedit_empty: bool) -> Option<&str> {
    if !preedit_empty || !text.is_ascii() {
        return None;
    }
    Some(text)
}

/// Appends typed or committed text to the query, dropping control characters.
/// Returns whether anything was appended.
pub fn append_text(query: &mut String, text: &str) -> bool {
    let before = query.len();
    query.extend(text.chars().filter(|c| !c.is_control()));
    query.len() != before
}

/// Backspace: removes the last character of the query, unless a composition is
/// open (then the IME takes the key). Returns whether the query changed.
pub fn backspace(query: &mut String, preedit: &str) -> bool {
    if !preedit.is_empty() {
        return false;
    }
    query.pop().is_some()
}

/// Handles an IME event from the window.
pub fn handle_ime(state: &mut AppState, event: Ime) {
    if !state.is_search_active || state.screen != AppScreen::SongSelect {
        // Stale composition after the box closed: never let it linger.
        state.search_preedit.clear();
        return;
    }
    match event {
        Ime::Enabled => {}
        Ime::Preedit(text, _cursor) => state.search_preedit = text,
        Ime::Commit(text) => {
            state.search_preedit.clear();
            append_text(&mut state.search_query, &text);
            state.recompute_filtered_songs();
            state.cursor_settle_time = Instant::now();
        }
        Ime::Disabled => state.search_preedit.clear(),
    }
}

/// Tells the OS where the search caret is, so the candidate list opens under
/// it. Only sends when the caret moved.
pub fn sync_caret(state: &mut AppState, caret: Option<Rect>) {
    let Some(r) = caret else {
        return;
    };
    if state.ime_caret_sent == Some(r) {
        return;
    }
    state.window.set_ime_cursor_area(
        PhysicalPosition::new(f64::from(r.x), f64::from(r.y)),
        PhysicalSize::new(f64::from(r.w), f64::from(r.h)),
    );
    state.ime_caret_sent = Some(r);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_appends_and_drops_control_characters() {
        let mut q = String::from("가");
        assert!(append_text(&mut q, "을\u{7}밤"));
        assert_eq!(q, "가을밤");
        assert!(
            !append_text(&mut q, "\u{8}"),
            "a control-only commit appends nothing"
        );
        assert_eq!(q, "가을밤");
    }

    #[test]
    fn preedit_is_not_query_text() {
        // While a composition is open, key text and Backspace leave the query
        // alone; only a commit changes it.
        let mut q = String::from("가을");
        assert!(key_text_to_append("x", false).is_none());
        assert!(!backspace(&mut q, "밤"));
        assert_eq!(q, "가을");
    }

    #[test]
    fn backspace_without_a_composition_pops_one_character() {
        let mut q = String::from("가을");
        assert!(backspace(&mut q, ""));
        assert_eq!(q, "가");
        assert!(backspace(&mut q, ""));
        assert!(!backspace(&mut q, ""), "nothing left to pop");
    }

    #[test]
    fn ascii_key_text_types_only_outside_a_composition() {
        assert_eq!(key_text_to_append("a", true), Some("a"));
        assert_eq!(key_text_to_append("a", false), None);
        // A Hangul key text is the IME's to commit, never the key's.
        assert_eq!(key_text_to_append("가", true), None);
    }

    #[test]
    fn search_closes_off_song_select_and_under_modals() {
        assert!(search_allowed(AppScreen::SongSelect, false, false));
        assert!(!search_allowed(AppScreen::SongSelect, true, false));
        assert!(!search_allowed(AppScreen::SongSelect, false, true));
        assert!(!search_allowed(AppScreen::Gameplay, false, false));
        assert!(!search_allowed(AppScreen::Result, false, false));
        assert!(!search_allowed(AppScreen::KeyConfig, false, false));
    }
}
