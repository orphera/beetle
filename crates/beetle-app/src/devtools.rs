//! Developer-only hooks driven by environment variables, for automated
//! screenshots of the real app (used to review UI changes):
//!
//! - `BEETLE_CAPTURE=<file.bmp>`: write one backbuffer capture to this path.
//! - `BEETLE_CAPTURE_SCREEN=boot|title|songselect|loading|gameplay|result|keyconfig|settings`
//!   (default `songselect`): screen to capture. `title` also keeps the title
//!   screen after the library read (any other capture variable skips it).
//! - `BEETLE_CAPTURE_DELAY=<seconds>` (default 2): wall time after entering
//!   that screen.
//! - `BEETLE_CAPTURE_EXIT=1`: quit right after capturing.
//! - `BEETLE_CAPTURE_ROW=<index>`: with the Settings screen, start on that row.
//! - `BEETLE_AUTOPLAY=1`: start with auto play enabled.
//! - `BEETLE_CAPTURE_MODAL=options|exit`: start with that song select modal open.
//!   `folders` with the Settings screen opens the Song Folders list.
//!   `songmenu` opens the selected song's menu, `songmenu-delete` its delete confirm.
//! - `BEETLE_CAPTURE_HOVER=<0..4>`: song select draws the pointer over that top
//!   bar button (left to right: auto, options, key config, settings, quit), so
//!   a capture shows its hover state and tooltip.
//! - With `BEETLE_CAPTURE_SCREEN=keyconfig` or `settings` the app starts there
//!   (it is otherwise only reachable by key press).
//! - `BEETLE_CAPTURE_TRANSITION=<0..1>`: draws the shutter frozen at that
//!   progress of its length on every frame, so a capture shows a mid-wipe.
//! - `BEETLE_CAPTURE_REVEAL=<seconds>`: freezes the Stage Result reveal at
//!   that time (0 to 3.2), for captures of each stage.
//! - `BEETLE_CAPTURE_LAMP=<failed|easy|clear|hard|fullcombo|perfect>`: the
//!   end banner shows this lamp instead of the play's real one (display only).
//!
//! - `BEETLE_CAPTURE_FOLDER=<path>`: song select starts in this folder
//!   (`dir`, `mode/7k`, ...) instead of the saved one.
//! - `BEETLE_LANG=ko|en|ja`: the UI language for this run only (not saved).
//! - `BEETLE_DEV_DROP=<path>;<path>...`: adds these as if they were dropped on
//!   the window at startup (see `install.rs`).
//!
//! Nothing here runs unless the variables are set.

use crate::state::AppScreen;
use beetle_render::{ColorRgba, GpuBackend, ImageBuffer};
use std::time::Instant;

pub struct Capture {
    path: String,
    screen: AppScreen,
    delay: f64,
    exit: bool,
    entered: Option<(AppScreen, Instant)>,
    frames: u32,
    done: bool,
}

fn parse_screen(s: &str) -> Option<AppScreen> {
    Some(match s.to_ascii_lowercase().as_str() {
        "boot" => AppScreen::Boot,
        "title" => AppScreen::Title,
        "songselect" => AppScreen::SongSelect,
        "loading" => AppScreen::Loading,
        "gameplay" => AppScreen::Gameplay,
        "result" => AppScreen::Result,
        "keyconfig" => AppScreen::KeyConfig,
        "settings" => AppScreen::Settings,
        _ => return None,
    })
}

/// `BEETLE_LANG`: the UI language for this run, for captures. Not saved.
pub fn lang_override() -> Option<beetle_render::strings::Lang> {
    std::env::var("BEETLE_LANG")
        .ok()
        .and_then(|v| beetle_render::strings::Lang::from_code(&v))
}

/// Appends to `devtools.log` in the working directory (the app is a GUI
/// subsystem binary, so stderr is not visible).
pub fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("devtools.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}

pub fn autoplay_requested() -> bool {
    std::env::var("BEETLE_AUTOPLAY").is_ok_and(|v| v == "1")
}

/// The frozen shutter progress from `BEETLE_CAPTURE_TRANSITION`, read once.
pub fn frozen_shutter_progress() -> Option<f32> {
    static PROGRESS: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *PROGRESS.get_or_init(|| {
        std::env::var("BEETLE_CAPTURE_TRANSITION")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|p| p.clamp(0.0, 1.0))
    })
}

/// The clear lamp the end banner shows instead of the real one, from
/// `BEETLE_CAPTURE_LAMP` (`failed`, `clear`, `hard`, `easy`, `fullcombo`,
/// `perfect`). Display only: the play is scored as it was. It exists for
/// captures: an autoplay judges every note PERFECT, so it can show PERFECT
/// but never FULL COMBO, and FAILED needs a real miss run instead.
pub fn banner_lamp_override() -> Option<beetle_core::ClearType> {
    use beetle_core::ClearType;
    static LAMP: std::sync::OnceLock<Option<ClearType>> = std::sync::OnceLock::new();
    *LAMP.get_or_init(|| {
        match std::env::var("BEETLE_CAPTURE_LAMP")
            .ok()?
            .to_ascii_lowercase()
            .as_str()
        {
            "failed" => Some(ClearType::Failed),
            "easy" => Some(ClearType::Easy),
            "clear" => Some(ClearType::Clear),
            "hard" => Some(ClearType::Hard),
            "fullcombo" => Some(ClearType::FullCombo),
            "perfect" => Some(ClearType::Perfect),
            _ => None,
        }
    })
}

/// The reveal clock frozen at `BEETLE_CAPTURE_REVEAL=<seconds>`, so a capture
/// shows one stage of the Stage Result reveal. Read once.
pub fn frozen_reveal_seconds() -> Option<f64> {
    static REVEAL: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
    *REVEAL.get_or_init(|| {
        std::env::var("BEETLE_CAPTURE_REVEAL")
            .ok()?
            .parse::<f64>()
            .ok()
            .filter(|t| t.is_finite())
            .map(|t| t.max(0.0))
    })
}

/// Whether the title screen follows the library read at startup. A chart path
/// on the command line skips it, and so does every dev capture or autoplay
/// variable, so those runs start as they did before the title existed.
/// `BEETLE_CAPTURE_SCREEN=title` keeps it.
pub fn title_after_boot(cli_chart: bool) -> bool {
    boot_shows_title(cli_chart, |key| std::env::var(key).ok())
}

fn boot_shows_title(cli_chart: bool, var: impl Fn(&str) -> Option<String>) -> bool {
    if cli_chart {
        return false;
    }
    if var("BEETLE_CAPTURE_SCREEN").is_some_and(|s| parse_screen(&s) == Some(AppScreen::Title)) {
        return true;
    }
    [
        "BEETLE_CAPTURE",
        "BEETLE_CAPTURE_SCREEN",
        "BEETLE_AUTOPLAY",
        "BEETLE_DEV_DROP",
    ]
    .iter()
    .all(|key| var(key).is_none())
}

/// `BEETLE_DEV_DROP`: paths to add as if dropped on the window.
pub fn dropped_paths() -> Vec<std::path::PathBuf> {
    std::env::var("BEETLE_DEV_DROP")
        .map(|v| {
            v.split(';')
                .filter(|p| !p.trim().is_empty())
                .map(|p| p.trim().into())
                .collect()
        })
        .unwrap_or_default()
}

/// Screen to start on instead of song select (only Key Config and Settings, which have
/// no other automatic way in).
pub fn start_screen() -> Option<AppScreen> {
    std::env::var("BEETLE_CAPTURE")
        .ok()
        .and_then(|_| std::env::var("BEETLE_CAPTURE_SCREEN").ok())
        .and_then(|s| parse_screen(&s))
        .filter(|s| matches!(s, AppScreen::KeyConfig | AppScreen::Settings))
}

/// `BEETLE_CAPTURE_FOLDER`: the song select folder to start in.
pub fn start_folder() -> Option<crate::folders::FolderPath> {
    std::env::var("BEETLE_CAPTURE_FOLDER")
        .ok()
        .map(|p| crate::folders::FolderPath::parse(&p))
}

/// The Settings row the capture starts on (`BEETLE_CAPTURE_ROW=<index>`), so a
/// capture can show a scrolled column without key input.
pub fn start_settings_row() -> Option<usize> {
    std::env::var("BEETLE_CAPTURE_ROW").ok()?.parse().ok()
}

/// `BEETLE_CAPTURE_MODAL=folders`: the Settings capture opens the Song Folders list.
pub fn folders_requested() -> bool {
    std::env::var("BEETLE_CAPTURE_MODAL").is_ok_and(|v| v == "folders")
}

/// `BEETLE_CAPTURE_MODAL=songmenu[-delete]`: the song menu to open after the
/// library read: `Some(false)` its first page, `Some(true)` the delete confirm.
pub fn song_menu_requested() -> Option<bool> {
    match std::env::var("BEETLE_CAPTURE_MODAL").as_deref() {
        Ok("songmenu") => Some(false),
        Ok("songmenu-delete") => Some(true),
        _ => None,
    }
}

/// The song select top bar button the pointer is drawn over
/// (`BEETLE_CAPTURE_HOVER=<index>`), read once.
pub fn capture_hover() -> Option<usize> {
    static HOVER: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    *HOVER.get_or_init(|| std::env::var("BEETLE_CAPTURE_HOVER").ok()?.parse().ok())
}

/// Song select modal to open at startup: (options, exit).
pub fn modal_requested() -> (bool, bool) {
    match std::env::var("BEETLE_CAPTURE_MODAL").as_deref() {
        Ok("options") => (true, false),
        Ok("exit") => (false, true),
        _ => (false, false),
    }
}

impl Capture {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("BEETLE_CAPTURE").ok()?;
        let screen = std::env::var("BEETLE_CAPTURE_SCREEN")
            .ok()
            .and_then(|s| parse_screen(&s))
            .unwrap_or(AppScreen::SongSelect);
        let delay = std::env::var("BEETLE_CAPTURE_DELAY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2.0);
        let exit = std::env::var("BEETLE_CAPTURE_EXIT").is_ok_and(|v| v == "1");
        Some(Self {
            path,
            screen,
            delay,
            exit,
            entered: None,
            frames: 0,
            done: false,
        })
    }

    pub fn pending(&self) -> bool {
        !self.done
    }

    /// Call every presented frame after drawing and before present.
    /// Returns `true` when the app should exit.
    pub fn on_frame(&mut self, screen: AppScreen, backend: &mut dyn GpuBackend) -> bool {
        if self.done {
            return false;
        }
        match self.entered {
            Some((s, _)) if s == screen => {}
            _ => {
                log(&format!("screen {screen:?}"));
                self.entered = Some((screen, Instant::now()));
                self.frames = 0;
            }
        }
        let (s, at) = self.entered.unwrap();
        self.frames += 1;
        if s != self.screen || at.elapsed().as_secs_f64() < self.delay {
            return false;
        }
        let secs = at.elapsed().as_secs_f64();
        log(&format!(
            "{} frames in {secs:.1}s = {:.0} fps ({}, {})",
            self.frames,
            self.frames as f64 / secs,
            backend.backend_name(),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        ));
        let msg = match save_backbuffer(backend, &self.path) {
            Ok((w, h)) => format!("captured {} ({w}x{h})", self.path),
            Err(e) => format!("capture failed: {e}"),
        };
        log(&msg);
        self.done = true;
        self.exit
    }
}

/// Writes the current backbuffer (call before present) to a BMP file.
pub fn save_backbuffer(backend: &mut dyn GpuBackend, path: &str) -> std::io::Result<(u32, u32)> {
    let (w, h, px) = backend
        .capture_frame()
        .ok_or_else(|| std::io::Error::other("backbuffer readback failed"))?;
    let img = ImageBuffer {
        width: w,
        height: h,
        pixels: px
            .chunks_exact(4)
            .map(|p| ColorRgba::new(p[0], p[1], p[2], 255))
            .collect(),
    };
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, img.encode_bmp_bytes())?;
    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn a_plain_start_shows_the_title() {
        assert!(boot_shows_title(false, env(&[])));
    }

    #[test]
    fn a_chart_path_skips_the_title() {
        assert!(!boot_shows_title(true, env(&[])));
        assert!(!boot_shows_title(
            true,
            env(&[("BEETLE_CAPTURE_SCREEN", "title")])
        ));
    }

    #[test]
    fn dev_capture_and_autoplay_skip_the_title() {
        for key in ["BEETLE_CAPTURE", "BEETLE_CAPTURE_SCREEN", "BEETLE_AUTOPLAY"] {
            assert!(!boot_shows_title(false, env(&[(key, "1")])), "{key}");
        }
        assert!(!boot_shows_title(
            false,
            env(&[("BEETLE_AUTOPLAY", "1"), ("BEETLE_CAPTURE", "x.bmp")])
        ));
    }

    #[test]
    fn capture_screen_title_keeps_it_and_parses() {
        assert!(boot_shows_title(
            false,
            env(&[
                ("BEETLE_CAPTURE", "x.bmp"),
                ("BEETLE_CAPTURE_SCREEN", "title")
            ])
        ));
        assert_eq!(parse_screen("title"), Some(AppScreen::Title));
        assert_eq!(parse_screen("TITLE"), Some(AppScreen::Title));
        assert!(!boot_shows_title(
            false,
            env(&[("BEETLE_CAPTURE_SCREEN", "songselect")])
        ));
    }
}
