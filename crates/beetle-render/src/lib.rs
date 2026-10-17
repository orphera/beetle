//! # beetle-render
//!
//! Direct3D 11 2D renderer for the Beetle player (ADR-026): an immediate-mode
//! `Canvas` that batches into one shared atlas, a proportional text engine,
//! a code-generated skin, and the screens built on them (`Ui`). Hardware
//! adapters are preferred; WARP runs the same pipeline on the CPU.
//! Also hosts the image decoders and video BGA player the app loads songs with.

pub mod art;
pub mod backdrop;
pub mod backend;
pub mod canvas;
pub mod hit;
pub mod image;
pub mod jacket;
pub mod motion;
pub mod reveal;
pub mod screens;
pub mod skin;
pub mod strings;
pub mod text;
pub mod theme;
pub mod ui;
pub mod video;
pub mod view;

pub use art::{Halo, NineSlice, Skin};
pub use backdrop::{BlurBackdrop, Mix};
#[cfg(target_os = "windows")]
pub use backend::D3d11Backend;
pub use backend::{BlendMode, GpuBackend, TextureId, Vertex2D};
pub use canvas::{AtlasRegion, Canvas, Insets, Rect};
pub use hit::{hit_at, Hit, HitId};
pub use image::{BgaAtlasBuilder, ImageBuffer};
pub use screens::{
    centred_start, clamp_into_window, draw_boot, draw_calibrate, draw_exit_modal, draw_gameplay,
    draw_key_config, draw_loading, draw_options_modal, draw_options_panel, draw_result,
    draw_screen_fade, draw_settings, draw_shutter, draw_song_select, draw_title, draw_toast,
    filter_items, first_note_time, follow_settings_scroll, scroll_by, shutter_geometry,
    visible_rows, window_start, BootFrame, CalibrateFrame, CalibratePhase, FilterBar, FilterItem,
    KeyBinding, KeyConfigFrame, LoadingFrame, OptionLine, OptionsFooter, PlayFrame, Rebind,
    ResultFrame, SelectFrame, SelectMotion, SelectRow, SettingsFrame, ShutterGeometry,
    ShutterPanel, ShutterPhase, SizedTexture, SortMenu, TitleFrame, ToastAnchor, ToastFrame,
    ToastKind, KEY_MODES,
};
pub use screens::{draw_drop_overlay, draw_help_overlay, title_menu_labels, LAMP_COUNT};
pub use skin::{
    green_ms_to_px_per_sec, px_per_sec_to_green_ms, scratch_side_applies, ColorRgba, EightKForm,
    FieldPosition, JudgePosition, ScratchSide, SkinConfig, NOTE_SIZE_MAX, NOTE_SIZE_MIN,
    NOTE_SIZE_STEP,
};
pub use text::{Align, FontMetrics, TextEngine, TextStyle, Weight};
pub use ui::{ambient_time, Ui, AMBIENT_LOOP_SECS};
pub use video::{is_video_path, BgaVideoPlayer, VIDEO_EXTENSIONS};
pub use view::{
    snap_ui_scale, HitBurst, JudgeMark, ViewState, Viewport, JUDGE_TIMELINE_SECONDS,
    UI_SCALE_DEFAULT, UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP,
};

/// Length of the Result screen's staged reveal (see `reveal`). Callers (the
/// app's render loop) use this to know how long to keep forcing redraws past
/// the initial dirty-flag render before the screen is fully settled and static.
pub const RESULT_REVEAL_DURATION_SECONDS: f64 = reveal::REVEAL_SECONDS;
