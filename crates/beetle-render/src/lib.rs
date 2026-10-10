//! # beetle-render
//!
//! Direct3D 11 2D renderer for the Beetle player (ADR-026): an immediate-mode
//! `Canvas` that batches into one shared atlas, a proportional text engine,
//! a code-generated skin, and the screens built on them (`Ui`). Hardware
//! adapters are preferred; WARP runs the same pipeline on the CPU.
//! Also hosts the image decoders and video BGA player the app loads songs with.

pub mod art;
pub mod backend;
pub mod canvas;
pub mod hit;
pub mod image;
pub mod motion;
pub mod screens;
pub mod skin;
pub mod strings;
pub mod text;
pub mod theme;
pub mod ui;
pub mod video;
pub mod view;

pub use art::{Halo, NineSlice, Skin};
#[cfg(target_os = "windows")]
pub use backend::D3d11Backend;
pub use backend::{BlendMode, GpuBackend, TextureId, Vertex2D};
pub use canvas::{AtlasRegion, Canvas, Insets, Rect};
pub use hit::{hit_at, Hit, HitId};
pub use image::{BgaAtlasBuilder, ImageBuffer};
pub use screens::{
    centred_start, clamp_into_window, draw_boot, draw_calibrate, draw_exit_modal, draw_gameplay,
    draw_key_config, draw_loading, draw_options_modal, draw_result, draw_screen_fade,
    draw_settings, draw_song_select, draw_toast, filter_items, scroll_by, visible_rows,
    window_start, BootFrame, CalibrateFrame, CalibratePhase, FilterBar, FilterItem, KeyBinding,
    KeyConfigFrame, LoadingFrame, OptionLine, PlayFrame, Rebind, ResultFrame, SelectFrame,
    SelectRow, SettingsFrame, SizedTexture, SortMenu, ToastFrame, ToastKind, KEY_MODES,
};
pub use skin::{
    green_ms_to_px_per_sec, px_per_sec_to_green_ms, scratch_side_applies, ColorRgba, EightKForm,
    FieldPosition, ScratchSide, SkinConfig,
};
pub use text::{Align, FontMetrics, TextEngine, TextStyle, Weight};
pub use ui::Ui;
pub use video::{is_video_path, BgaVideoPlayer, VIDEO_EXTENSIONS};
pub use view::{HitBurst, ViewState, Viewport};

/// Result screen rank-letter pop-in duration (ease_out_back settle).
pub const RANK_POP_SECONDS: f64 = 0.35;
/// Result screen EX score count-up duration (ease_out_cubic).
pub const SCORE_COUNT_SECONDS: f64 = 0.9;
/// Longest of the Result screen's reveal animations — callers (the app's
/// render loop) use this to know how long to keep forcing redraws past the
/// initial dirty-flag render before the screen is fully settled and static.
pub const RESULT_REVEAL_DURATION_SECONDS: f64 = SCORE_COUNT_SECONDS;
