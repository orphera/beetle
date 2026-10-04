//! Developer-only hooks driven by environment variables, for automated
//! screenshots of the real app (used to review UI changes):
//!
//! - `BEETLE_CAPTURE=<file.bmp>`: write one backbuffer capture to this path.
//! - `BEETLE_CAPTURE_SCREEN=songselect|loading|gameplay|result|keyconfig`
//!   (default `songselect`): screen to capture.
//! - `BEETLE_CAPTURE_DELAY=<seconds>` (default 2): wall time after entering
//!   that screen.
//! - `BEETLE_CAPTURE_EXIT=1`: quit right after capturing.
//! - `BEETLE_AUTOPLAY=1`: start with auto play enabled.
//! - `BEETLE_CAPTURE_MODAL=options|exit`: start with that song select modal open.
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
        "songselect" => AppScreen::SongSelect,
        "loading" => AppScreen::Loading,
        "gameplay" => AppScreen::Gameplay,
        "result" => AppScreen::Result,
        "keyconfig" => AppScreen::KeyConfig,
        _ => return None,
    })
}

/// Appends to `devtools.log` in the working directory (the app is a GUI
/// subsystem binary, so stderr is not visible).
fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("devtools.log") {
        let _ = writeln!(f, "{msg}");
    }
}

pub fn autoplay_requested() -> bool {
    std::env::var("BEETLE_AUTOPLAY").is_ok_and(|v| v == "1")
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
            if cfg!(debug_assertions) { "debug" } else { "release" }
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
        pixels: px.chunks_exact(4).map(|p| ColorRgba::new(p[0], p[1], p[2], 255)).collect(),
    };
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, img.encode_bmp_bytes())?;
    Ok((w, h))
}
