//! Fills the word pictures of game disc songs (`beetle_render::words`) for the
//! charts near the select list, from the names files in `.cache/names`
//! (`names`). A chart without names keeps its text title.
//!
//! Like `thumbs`, this runs on the render thread once a frame: it only writes
//! the words of rows that are on screen or next to it, and each word is a
//! small copy into the atlas, so nothing here waits on the disc.

use beetle_render::words::Word;
use cs_disc::popn::Mask;

use crate::folders::ListEntry;
use crate::state::AppState;

/// A mask as the atlas takes it.
fn word(mask: &Mask) -> Word<'_> {
    Word {
        width: mask.width,
        height: mask.height,
        coverage: &mask.coverage,
    }
}

/// Writes the words of the rows around the list window that are not resident
/// yet. The names are read once, on the first call.
pub fn pump(state: &mut AppState, scroll: f32, visible: usize) {
    let AppState {
        songs,
        entries,
        gpu_ui,
        disc_names,
        disc_names_songs,
        ..
    } = state;
    // Read again whenever the list has changed size, so a scan that finishes
    // after the first frame is still picked up.
    if disc_names.is_none() || *disc_names_songs != songs.len() {
        *disc_names = Some(crate::names::load_all());
        *disc_names_songs = songs.len();
    }
    let Some(names) = disc_names.as_ref() else {
        return;
    };
    if names.is_empty() {
        return;
    }
    let ui = gpu_ui.ui.menu();
    let range = crate::thumbs::prefetch_rows(scroll.max(0.0) as usize, visible, entries.len());
    for row in range {
        // A group row is one song's charts; they share the song's names.
        let indices: &[usize] = match entries.get(row) {
            Some(ListEntry::Song(i)) => std::slice::from_ref(i),
            // Only the chart the row shows: the slots hold one chart per row,
            // so every chart of every group would crowd the rows out.
            Some(ListEntry::Group {
                charts, selected, ..
            }) => charts
                .get(*selected)
                .map(std::slice::from_ref)
                .unwrap_or(&[]),
            _ => continue,
        };
        for &i in indices {
            let Some(song) = songs.get(i) else {
                continue;
            };
            if ui.words.contains(song.id) {
                continue;
            }
            let Some(song_names) = names.get(&song.id) else {
                continue;
            };
            let title = song_names.title.as_ref().map(word);
            let small_title = song_names.small_title.as_ref().map(word);
            let artist = song_names.artist.as_ref().map(word);
            let genre = song_names.genre.as_ref().map(word);
            if title.is_none() && small_title.is_none() && artist.is_none() && genre.is_none() {
                continue;
            }
            ui.words.insert(
                ui.canvas.atlas_mut(),
                song.id,
                title,
                small_title,
                artist,
                genre,
            );
        }
    }
}
