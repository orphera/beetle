//! The song menu on song select (plan S4, `docs/plans/2026-10-21-song-install-in-app.md`):
//! right-click a song or press M. Every song can open its folder. A song from
//! an installed package can be deleted (after a confirm page) and, when the
//! package has more than one version, switched to another version. A song
//! from a folder (linked, or `songs/`) is never deleted: it can only be hidden
//! from the list (collection design §9.2). Hidden charts are listed by id in
//! `hidden.dat`; the Settings row shows them all again.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use beetle_audio::UiSound;
use beetle_core::{ChartId, SongAddress, SongMetadata};
use beetle_render::{strings, HitId, ToastKind};
use bms_package::installed::packages_root;
use bms_package_manager::PackageManager;
use winit::keyboard::KeyCode;

use crate::menu_audio::play_ui;
use crate::state::AppState;
use crate::transition::show_toast;

/// Where the selected song is loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// An entry of an installed `.bmsp`.
    Package { bmsp: PathBuf },
    /// A chart file in a folder.
    Folder { file: PathBuf },
}

impl Source {
    /// From a song's load path (`SongAddress`). A disc image counts as a
    /// folder chart: the user's own file, opened in its folder and only hidden.
    pub fn of(file_path: &str) -> Self {
        match SongAddress::parse(file_path) {
            SongAddress::Package { bmsp, .. } => Source::Package { bmsp: bmsp.into() },
            SongAddress::Folder { file } | SongAddress::Disc { image: file, .. } => {
                Source::Folder { file: file.into() }
            }
        }
    }

    /// The folder Explorer opens: the chart's folder, or the package file's.
    fn folder(&self) -> Option<&Path> {
        match self {
            Source::Package { bmsp } => bmsp.parent(),
            Source::Folder { file } => file.parent(),
        }
    }
}

/// What a menu row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    OpenFolder,
    /// Goes to the confirm page.
    AskDelete,
    Delete,
    Hide,
    /// Goes to the version page.
    Versions,
    /// Makes this state of the package the one that plays.
    UseVersion(String),
    /// Back to the first page (the confirm page's cancel).
    Back,
}

/// One row: its label and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub action: Action,
}

/// The package a song comes from, read from the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub id: String,
    /// (state hash, label) of each installed version, oldest install first.
    pub versions: Vec<(String, String)>,
    pub active: String,
}

/// The menu while it is open.
pub struct SongMenu {
    pub title: String,
    /// A line under the title (the confirm page's warning).
    pub note: Option<String>,
    pub items: Vec<Item>,
    pub row: usize,
    source: Source,
    package: Option<PackageInfo>,
    chart: ChartId,
    /// How many listed charts the package holds (all of them go with a delete).
    charts_in_package: usize,
}

/// The first page's rows for a song from `source`.
pub fn first_page(source: &Source, package: Option<&PackageInfo>) -> Vec<Item> {
    let mut items = vec![Item {
        label: strings::MENU_OPEN_FOLDER().to_string(),
        action: Action::OpenFolder,
    }];
    match (source, package) {
        (Source::Package { .. }, Some(p)) => {
            if p.versions.len() > 1 {
                items.push(Item {
                    label: strings::MENU_VERSIONS().to_string(),
                    action: Action::Versions,
                });
            }
            items.push(Item {
                label: strings::MENU_DELETE().to_string(),
                action: Action::AskDelete,
            });
        }
        // A package the registry does not know cannot be removed here.
        (Source::Package { .. }, None) => {}
        (Source::Folder { .. }, _) => items.push(Item {
            label: strings::MENU_HIDE().to_string(),
            action: Action::Hide,
        }),
    }
    items
}

/// The version page: one row per installed version, the playing one marked.
pub fn version_page(package: &PackageInfo) -> Vec<Item> {
    package
        .versions
        .iter()
        .map(|(hash, label)| Item {
            label: if *hash == package.active {
                strings::fill(strings::MENU_VERSION_ACTIVE(), &[label])
            } else {
                label.clone()
            },
            action: Action::UseVersion(hash.clone()),
        })
        .collect()
}

/// Finds the package whose installed file is `bmsp`.
fn find_package(mgr: &PackageManager, bmsp: &Path) -> Option<PackageInfo> {
    let root = mgr.root_dir();
    for record in mgr.registry().list_packages() {
        let has = record
            .state_hashes
            .values()
            .any(|st| same_file(&root.join(&st.path), bmsp));
        if has {
            let mut versions: Vec<(String, String, String)> = record
                .state_hashes
                .iter()
                .map(|(hash, st)| {
                    let short: String = hash.chars().take(8).collect();
                    let date: String = st.installed_at.chars().take(10).collect();
                    (
                        st.installed_at.clone(),
                        hash.clone(),
                        format!("{date} ({short})"),
                    )
                })
                .collect();
            versions.sort();
            return Some(PackageInfo {
                id: record.id.clone(),
                versions: versions.into_iter().map(|(_, h, l)| (h, l)).collect(),
                active: record.active_state.clone(),
            });
        }
    }
    None
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// Opens the menu for the selected song (right-click on its row, or M).
pub fn open(state: &mut AppState) {
    let Some(song) = state.current_selected_song() else {
        return;
    };
    let source = Source::of(&song.file_path);
    let title = song.title.clone();
    let chart = song.id;
    let package = match &source {
        Source::Package { bmsp } => PackageManager::new(packages_root())
            .ok()
            .and_then(|mgr| find_package(&mgr, bmsp)),
        Source::Folder { .. } => None,
    };
    let charts_in_package = match &source {
        Source::Package { bmsp } => {
            let prefix = format!("{}::", bmsp.to_string_lossy());
            state
                .songs
                .iter()
                .filter(|s| s.file_path.starts_with(&prefix))
                .count()
        }
        Source::Folder { .. } => 0,
    };
    play_ui(state, UiSound::Open);
    state.song_menu = Some(SongMenu {
        title,
        note: None,
        items: first_page(&source, package.as_ref()),
        row: 0,
        source,
        package,
        chart,
        charts_in_package,
    });
}

/// `BEETLE_CAPTURE_MODAL=songmenu[-delete]`: opens the menu, and with
/// `delete` goes on to its delete confirm when the song has one.
pub fn dev_open(state: &mut AppState, delete: bool) {
    // The root lists folders: go into the first ones until a song is selected.
    for _ in 0..3 {
        if state.current_selected_song().is_some() {
            break;
        }
        crate::handlers::song_select::enter_folder(state);
    }
    open(state);
    let ask = state
        .song_menu
        .as_ref()
        .and_then(|m| m.items.iter().position(|i| i.action == Action::AskDelete));
    if let (true, Some(i)) = (delete, ask) {
        run(state, i);
    }
}

fn close(state: &mut AppState) {
    play_ui(state, UiSound::Close);
    state.song_menu = None;
}

/// Runs row `i`.
fn run(state: &mut AppState, i: usize) {
    let Some(menu) = &mut state.song_menu else {
        return;
    };
    let Some(action) = menu.items.get(i).map(|it| it.action.clone()) else {
        return;
    };
    match action {
        Action::OpenFolder => {
            let opened = menu
                .source
                .folder()
                .is_some_and(|dir| crate::shell::open("explore", dir));
            state.song_menu = None;
            if !opened {
                play_ui(state, UiSound::Toast);
                show_toast(state, ToastKind::Error, strings::TOAST_FOLDER_FAILED());
            }
        }
        Action::AskDelete => {
            let count = menu.charts_in_package.max(1);
            menu.note = Some(if count > 1 {
                strings::fill(strings::MENU_DELETE_NOTE_MANY(), &[&count.to_string()])
            } else {
                strings::MENU_DELETE_NOTE().to_string()
            });
            menu.items = vec![
                Item {
                    label: strings::CANCEL().to_string(),
                    action: Action::Back,
                },
                Item {
                    label: strings::MENU_DELETE_CONFIRM().to_string(),
                    action: Action::Delete,
                },
            ];
            menu.row = 0;
            play_ui(state, UiSound::Open);
        }
        Action::Back => {
            menu.note = None;
            menu.items = first_page(&menu.source, menu.package.as_ref());
            menu.row = 0;
            play_ui(state, UiSound::Cancel);
        }
        Action::Versions => {
            if let Some(p) = &menu.package {
                menu.items = version_page(p);
                menu.row = p
                    .versions
                    .iter()
                    .position(|(h, _)| *h == p.active)
                    .unwrap_or(0);
                play_ui(state, UiSound::Open);
            }
        }
        Action::Delete => {
            let Some(p) = menu.package.clone() else {
                return;
            };
            let title = menu.title.clone();
            state.song_menu = None;
            let result = PackageManager::new(packages_root()).and_then(|mut mgr| {
                for (hash, _) in &p.versions {
                    mgr.uninstall(&p.id, hash)?;
                }
                Ok(())
            });
            changed(state, result.is_ok(), strings::TOAST_DELETED(), &title);
        }
        Action::UseVersion(hash) => {
            let Some(p) = menu.package.clone() else {
                return;
            };
            let title = menu.title.clone();
            state.song_menu = None;
            if hash == p.active {
                play_ui(state, UiSound::Close);
                return;
            }
            let result = PackageManager::new(packages_root())
                .and_then(|mut mgr| mgr.set_active(&p.id, &hash));
            changed(state, result.is_ok(), strings::TOAST_VERSION_SET(), &title);
        }
        Action::Hide => {
            let chart = menu.chart;
            let title = menu.title.clone();
            state.song_menu = None;
            let mut hidden = load_hidden();
            hidden.insert(chart);
            play_ui(state, UiSound::Toast);
            if save_hidden(&hidden).is_err() {
                show_toast(state, ToastKind::Error, strings::TOAST_HIDDEN_SAVE_FAILED());
                return;
            }
            state.hidden_songs = hidden.len();
            state.songs.retain(|s| s.id != chart);
            state.recompute_entries();
            show_toast(
                state,
                ToastKind::Success,
                strings::fill(strings::TOAST_HIDDEN(), &[&title]),
            );
        }
    }
}

/// After the registry changed: read the library again, and say what happened
/// once the read is done (in place of the rescan's toast).
fn changed(state: &mut AppState, ok: bool, done: &str, title: &str) {
    if !ok {
        play_ui(state, UiSound::Toast);
        show_toast(state, ToastKind::Error, strings::TOAST_SONG_CHANGE_FAILED());
        return;
    }
    state.installs.summary = Some(crate::install::Summary {
        text: strings::fill(done, &[title]),
        kind: ToastKind::Success,
        focus: Vec::new(),
    });
    state.start_rescan();
}

/// Keys while the menu is open: arrows move, ENTER runs, ESC closes (or goes
/// back from the confirm and version pages).
pub fn key(state: &mut AppState, code: KeyCode) {
    let Some(menu) = &mut state.song_menu else {
        return;
    };
    let rows = menu.items.len();
    match code {
        KeyCode::Escape if menu.note.is_some() || is_version_page(menu) => run_back(state),
        KeyCode::Escape | KeyCode::KeyM => close(state),
        KeyCode::ArrowUp | KeyCode::KeyK if menu.row > 0 => {
            menu.row -= 1;
            play_ui(state, UiSound::Move);
        }
        KeyCode::ArrowDown | KeyCode::KeyJ if menu.row + 1 < rows => {
            menu.row += 1;
            play_ui(state, UiSound::Move);
        }
        KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
            let row = menu.row;
            run(state, row);
        }
        _ => (),
    }
}

fn is_version_page(menu: &SongMenu) -> bool {
    matches!(
        menu.items.first().map(|i| &i.action),
        Some(Action::UseVersion(_))
    )
}

fn run_back(state: &mut AppState) {
    if let Some(menu) = &mut state.song_menu {
        menu.note = None;
        menu.items = first_page(&menu.source, menu.package.as_ref());
        menu.row = 0;
    }
    play_ui(state, UiSound::Cancel);
}

/// A click while the menu is open. A click outside it closes it.
pub fn click(state: &mut AppState, id: HitId) {
    match id {
        HitId::Blocker => close(state),
        HitId::MenuItem(i) => run(state, i),
        _ => (),
    }
}

// ---------------------------------------------------------------------------
// hidden.dat
// ---------------------------------------------------------------------------

/// `hidden.dat` in the working directory: one chart id (hex) per line.
fn hidden_file() -> PathBuf {
    std::env::var("BEETLE_HIDDEN_FILE").map_or_else(|_| PathBuf::from("hidden.dat"), PathBuf::from)
}

pub fn parse_hidden(text: &str) -> HashSet<ChartId> {
    text.lines()
        .filter_map(|l| ChartId::from_hex(l.trim()))
        .collect()
}

pub fn serialize_hidden(hidden: &HashSet<ChartId>) -> String {
    let mut lines: Vec<String> = hidden.iter().map(|id| id.to_hex()).collect();
    lines.sort();
    let mut out = String::from("# Charts hidden from the song list\n");
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    out
}

pub fn load_hidden() -> HashSet<ChartId> {
    parse_hidden(&fs::read_to_string(hidden_file()).unwrap_or_default())
}

fn save_hidden(hidden: &HashSet<ChartId>) -> std::io::Result<()> {
    fs::write(hidden_file(), serialize_hidden(hidden))
}

/// Drops the hidden charts from a freshly read library. Returns how many
/// charts are hidden.
pub fn drop_hidden(songs: &mut Vec<SongMetadata>) -> usize {
    let hidden = load_hidden();
    if !hidden.is_empty() {
        songs.retain(|s| !hidden.contains(&s.id));
    }
    hidden.len()
}

/// Shows every hidden chart again (the Settings row) and reads the library again.
pub fn unhide_all(state: &mut AppState) {
    if state.hidden_songs == 0 {
        return;
    }
    play_ui(state, UiSound::Toast);
    if fs::remove_file(hidden_file()).is_err() {
        show_toast(state, ToastKind::Error, strings::TOAST_HIDDEN_SAVE_FAILED());
        return;
    }
    state.installs.summary = Some(crate::install::Summary {
        text: strings::TOAST_UNHIDDEN().to_string(),
        kind: ToastKind::Success,
        focus: Vec::new(),
    });
    state.start_rescan();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(versions: usize) -> PackageInfo {
        PackageInfo {
            id: "p".into(),
            versions: (0..versions)
                .map(|i| (format!("h{i}"), format!("v{i}")))
                .collect(),
            active: "h0".into(),
        }
    }

    fn actions(items: &[Item]) -> Vec<Action> {
        items.iter().map(|i| i.action.clone()).collect()
    }

    #[test]
    fn the_load_path_tells_package_from_folder() {
        assert_eq!(
            Source::of("D:/g/packages/p/s/package.bmsp::a/x.bms"),
            Source::Package {
                bmsp: "D:/g/packages/p/s/package.bmsp".into()
            }
        );
        assert_eq!(
            Source::of("D:/BMS/song/x.bms"),
            Source::Folder {
                file: "D:/BMS/song/x.bms".into()
            }
        );
        assert_eq!(
            Source::of("disc:D:/iso/iidx16.iso::017/sp_another"),
            Source::Folder {
                file: "D:/iso/iidx16.iso".into()
            }
        );
    }

    #[test]
    fn a_folder_song_is_hidden_never_deleted() {
        let src = Source::of("D:/BMS/song/x.bms");
        assert_eq!(
            actions(&first_page(&src, None)),
            vec![Action::OpenFolder, Action::Hide]
        );
    }

    #[test]
    fn versions_show_only_with_two_or_more() {
        let src = Source::of("a.bmsp::x.bms");
        assert_eq!(
            actions(&first_page(&src, Some(&package(1)))),
            vec![Action::OpenFolder, Action::AskDelete]
        );
        assert_eq!(
            actions(&first_page(&src, Some(&package(2)))),
            vec![Action::OpenFolder, Action::Versions, Action::AskDelete]
        );
        assert_eq!(actions(&first_page(&src, None)), vec![Action::OpenFolder]);
    }

    #[test]
    fn the_version_page_marks_the_playing_one() {
        let page = version_page(&package(2));
        assert_eq!(page.len(), 2);
        assert_ne!(page[0].label, "v0");
        assert_eq!(page[1].label, "v1");
        assert_eq!(page[1].action, Action::UseVersion("h1".into()));
    }

    #[test]
    fn hidden_list_round_trips() {
        let mut set = HashSet::new();
        set.insert(ChartId::synthetic(1));
        set.insert(ChartId::synthetic(2));
        assert_eq!(parse_hidden(&serialize_hidden(&set)), set);
    }
}
