//! Gameplay screen on the `Ui` (Canvas + TextEngine + generated Skin).
//! Replaces both `gameplay.rs` (software) and `gameplay_gpu.rs` (D3D11 batcher)
//! — one implementation, typically one or two draw calls per frame
//! (a second only when a BGA texture is on screen).
//!
//! Note positions are derived only from `audio_time` (INV-1).

use super::widgets;
use crate::art::Skin;
use crate::backend::TextureId;
use crate::canvas::{Canvas, Rect};
use crate::hit::{HitId, HitSink};
use crate::motion::{ease_in_cubic, ease_out_back, ease_out_cubic, ease_out_quad};
use crate::skin::{
    is_side_track, ColorRgba, FieldPosition, JudgePosition, SkinConfig, JUDGE_WORD_BASELINE_720,
};
use crate::strings;
use crate::text::{Align, Fx, TextEngine, TextStyle};
use crate::theme::{self, caption, thousands};
use crate::ui::Ui;
use crate::view::{lane_index, HitBurst, JudgeMark, Viewport, JUDGE_TIMELINE_SECONDS, LANE_COUNT};
use beetle_core::{
    BmsChart, ClearType, GaugeType, JudgeGrade, Lane, NoteType, PlayMode, PlayNote, ScoreTracker,
    TimingModel,
};

/// A GPU texture with its pixel size (for aspect-correct fitting).
#[derive(Debug, Clone, Copy)]
pub struct SizedTexture {
    pub id: TextureId,
    pub width: u32,
    pub height: u32,
}

/// Everything the gameplay screen shows for one frame.
pub struct PlayFrame<'a> {
    pub viewport: &'a Viewport,
    pub layout: &'a SkinConfig,
    pub chart: &'a BmsChart,
    pub notes: &'a [PlayNote],
    pub timing: &'a TimingModel,
    pub score: &'a ScoreTracker,
    pub audio_time: f64,
    pub song_length: f64,
    pub visual_levels: &'a [f32; 16],
    /// Base BGA (already resolved: poor > video > image) and layer BGA.
    pub bga: Option<SizedTexture>,
    pub layer: Option<SizedTexture>,
    /// The song's stage image (STAGEFILE / BANNER). When the chart has no BGA
    /// of its own it fills the HUD column under the song block.
    pub jacket: Option<SizedTexture>,
    /// The BPM the scroll speed is set for: a note moves at `hi_speed` px/s
    /// at this BPM, faster above it and slower below (the hi-speed anchor).
    pub scroll_bpm: f64,
    /// The chart has BGA images or videos (not just the stage image standing
    /// in behind the field).
    pub chart_has_bga: bool,
    /// Opacity of the BGA backdrop behind the whole screen (0 = off).
    pub track_bga_opacity: f32,
    pub key_pressed: &'a [bool; LANE_COUNT],
    pub hit_bursts: &'a [HitBurst],
    pub last_judge: Option<(JudgeGrade, f64, f64)>,
    /// Key-binding hint line at the bottom of the HUD.
    pub hint: &'a str,
    /// Highlighted mode badge, e.g. "AUTO PLAY" / "REPLAY".
    pub badge: Option<&'a str>,
    /// `Some(selected option)` while the pause menu is open.
    pub pause: Option<usize>,
    /// The first note to hit, in audio seconds (`first_note_time`), or `None`
    /// when the play has none. Fixed when the play starts.
    pub first_note: Option<f64>,
    /// Opacity of the key hint line (0 = hidden).
    pub key_hint_alpha: f32,
    /// Readout over the lane top (for example "그린 500 ms") and the audio
    /// time it appeared at. Shown for `READOUT_SECONDS`.
    pub readout: Option<(&'a str, f64)>,
    /// End banner: the clear lamp of the finished play, and its progress
    /// through the banner (0 = started, 1 = done).
    pub banner: Option<(ClearType, f32)>,
    /// Judgements of the last few seconds, oldest first.
    pub judge_marks: &'a [JudgeMark],
    /// Visual offset in ms: where the notes are drawn, not when they are
    /// judged. It shifts the note clock below and nothing else.
    pub visual_offset_ms: f64,
    /// Reduce flashes: no full-field danger flash, dimmer hit bursts, no banner
    /// light sweep and no FAILED shake.
    pub reduce_flashes: bool,
    /// High-contrast notes: outlined heads in stronger colours, a thicker
    /// judge line that does not pulse, and brighter lane dividers.
    pub high_contrast: bool,
}

const BURST_SECONDS: f64 = 0.28;
const JUDGE_SECONDS: f64 = 0.5;
/// How long the combo number stays after the last judgement; the count itself
/// is kept and shows again on the next one.
const COMBO_SECONDS: f64 = 1.0;
/// READY ends this long before the first note (audio seconds).
const READY_LEAD: f64 = 1.0;
/// READY fades out over this long, ending at `READY_LEAD` before the first note.
const READY_FADE: f64 = 0.5;
/// A first note sooner than this gets no READY label at all.
const READY_MIN_FIRST_NOTE: f64 = 1.5;
/// The count-in (3-2-1) needs a first note at least this late, and the three
/// beats it fills must fit after the song start.
const COUNT_MIN_FIRST_NOTE: f64 = 2.0;
/// The count-in fills this many beats before the first note: one number a beat.
const COUNT_BEATS: f64 = 3.0;
/// GO! flashes from the first note and fades over this long (audio seconds).
const GO_SECONDS: f64 = 0.6;
/// The readout stays this long; its last 0.3 s fade out.
pub const READOUT_SECONDS: f64 = 1.0;

pub fn draw_gameplay(ui: &mut Ui, f: &PlayFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = f.viewport.scale;
    let l = f.layout;
    let skin_field = Rect::new(
        l.playfield_x,
        l.playfield_y,
        l.playfield_width,
        l.playfield_height,
    );
    let lane_field = Rect::new(
        l.playfield_x,
        f.viewport.y,
        l.playfield_width,
        f.viewport.height,
    );
    // With flashes reduced the danger stays red but stops blinking.
    let danger = is_danger(f.score) && (f.reduce_flashes || (f.audio_time * 6.0).sin() > 0.0);

    let hud = hud_layout(f.viewport, skin_field, l.effective_position(), s);

    backdrop(c, &sk, f, lite);
    playfield(c, &sk, f, lane_field, danger, s);
    gauge(c, t, &sk, f, hud.gauge, danger, s);
    ready(c, t, &sk, f, lane_field, s);
    // The end banner replaces the combo and the last judgement.
    if f.banner.is_none() {
        combo_and_judge(c, t, &sk, f, lane_field, s);
    }
    readout(c, t, &sk, f, lane_field, s);
    hud_panels(c, t, &sk, f, &hud, s);
    end_banner(c, t, &sk, f, lane_field, s);
    if let Some(selected) = f.pause {
        let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
        pause_menu(c, t, &sk, f, selected, s, &mut hs);
    }
}

/// Opacity of READY at `audio_time` before a first note at `first_note`
/// (both audio seconds). Full from the song start, fading out over the
/// half second that ends `READY_LEAD` before the note. A first note sooner
/// than `READY_MIN_FIRST_NOTE` gets no READY.
pub(crate) fn ready_alpha(audio_time: f64, first_note: f64) -> f32 {
    if first_note < READY_MIN_FIRST_NOTE || audio_time < 0.0 {
        return 0.0;
    }
    ((first_note - READY_LEAD - audio_time) / READY_FADE).clamp(0.0, 1.0) as f32
}

/// What the start cue shows over the lane at one audio time. The cue is a
/// pure function of the audio clock and the first note (INV-1): it never
/// moves a note or delays the song.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum StartCue {
    /// Nothing: no notes, a first note too soon, or before the song.
    None,
    /// Short lead-in: READY at this opacity (it fades out before the note).
    Ready(f32),
    /// Count-in: the number (3, 2 or 1) and its progress through its beat (0..1).
    Count { number: u8, progress: f32 },
    /// GO! at the first note; progress through its flash (0..1).
    Go(f32),
}

/// The start cue at `audio_time` for a first note at `first_note` seconds, on
/// the chart's timing (see `start_cue`).
pub(crate) fn start_cue_on(timing: &TimingModel, audio_time: f64, first_note: f64) -> StartCue {
    start_cue(
        audio_time,
        first_note,
        timing.time_to_beat_position(first_note),
        timing.time_to_beat_position(audio_time),
    )
}

/// The start cue at `audio_time`, given the first note in audio seconds and
/// both moments as beat positions on the chart (`first_beat` for the note,
/// `now_beat` for the audio clock).
///
/// The count-in is counted in beats, so a tempo change or a STOP before the
/// note moves it with the chart: 3, 2, 1 over the last three beats before the
/// note, one number a beat, then GO! on the note (GO! itself follows the
/// audio seconds, INV-1). A shorter lead-in, or one under three beats in,
/// falls back to READY, then GO!. A first note sooner than
/// `READY_MIN_FIRST_NOTE` gets nothing at all.
pub(crate) fn start_cue(
    audio_time: f64,
    first_note: f64,
    first_beat: f64,
    now_beat: f64,
) -> StartCue {
    if first_note < READY_MIN_FIRST_NOTE || audio_time < 0.0 {
        return StartCue::None;
    }
    if audio_time >= first_note {
        let e = audio_time - first_note;
        return if e < GO_SECONDS {
            StartCue::Go((e / GO_SECONDS) as f32)
        } else {
            StartCue::None
        };
    }
    let count_fits =
        first_beat.is_finite() && first_beat >= COUNT_BEATS && first_note >= COUNT_MIN_FIRST_NOTE;
    if !count_fits {
        let a = ready_alpha(audio_time, first_note);
        return if a > 0.0 {
            StartCue::Ready(a)
        } else {
            StartCue::None
        };
    }
    let window_start = first_beat - COUNT_BEATS;
    // NaN fails this test too: a broken clock shows nothing rather than a count.
    if !(now_beat >= window_start) {
        return StartCue::None;
    }
    let beats = now_beat - window_start;
    let beat = (beats.floor() as usize).min(2);
    StartCue::Count {
        number: 3 - beat as u8,
        progress: (beats - beat as f64).clamp(0.0, 1.0) as f32,
    }
}

/// Scale of a punch-in at progress `p` through its `span` (0..1): starts
/// `amount` bigger and settles to 1 with a small overshoot.
pub(crate) fn punch(p: f32, amount: f32, span: f32) -> f32 {
    1.0 + amount * (1.0 - ease_out_back((p / span).clamp(0.0, 1.0)))
}

/// Opacity of a count-in number at progress `p` through its beat: full, then
/// fading out over the last 30% so the next number takes over cleanly.
pub(crate) fn count_alpha(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    if p < 0.7 {
        1.0
    } else {
        1.0 - ease_in_cubic((p - 0.7) / 0.3)
    }
}

/// Vertical offset (before the screen scale) of the FAILED banner at progress
/// `p`: a fast shake that eases out to exactly 0 at the end of the banner.
pub(crate) fn fail_shake(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    shake_envelope(p) * (p * 52.0).sin()
}

/// The largest shake `fail_shake` makes at progress `p` (it decays cubically).
pub(crate) fn shake_envelope(p: f32) -> f32 {
    let inv = 1.0 - p.clamp(0.0, 1.0);
    16.0 * inv * inv * inv
}

/// Darkening over the playfield behind a FAILED banner (0..=0.6), easing in.
pub(crate) fn fail_vignette(p: f32) -> f32 {
    0.6 * ease_out_quad((p / 0.3).clamp(0.0, 1.0))
}

/// Position 0..1 of the light sweep across a FULL COMBO / PERFECT banner, or
/// `None` before and after it (it crosses the banner's middle part only).
pub(crate) fn banner_sweep(p: f32) -> Option<f32> {
    (0.18..=0.68)
        .contains(&p)
        .then(|| ((p - 0.18) / 0.5).clamp(0.0, 1.0))
}

/// Overall opacity of the end banner at progress `p`: it pops in over the
/// first fifth, holds, and fades over the last quarter.
pub(crate) fn banner_fade(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    if p < 0.2 {
        p / 0.2
    } else if p > 0.75 {
        1.0 - (p - 0.75) / 0.25
    } else {
        1.0
    }
}

/// Fill colour of the end banner for a clear lamp: gold for the top clears,
/// red for FAILED, cyan for every other clear.
pub(crate) fn banner_color(lamp: ClearType) -> ColorRgba {
    match lamp {
        ClearType::Failed => theme::RED,
        ClearType::Perfect | ClearType::FullCombo => theme::GOLD,
        _ => theme::CYAN,
    }
}

/// The earliest note the player has to hit: not yet judged (a practice start
/// skips the notes before its measure), and not a landmine or a long-note end.
///
/// Called once when a play starts, after the practice fast-forward; the result
/// is kept on the play (`PlayFrame::first_note`), so it does not move to the
/// next note once this one is hit.
pub fn first_note_time(notes: &[PlayNote]) -> Option<f64> {
    notes
        .iter()
        .filter(|n| !n.is_judged)
        .filter(|n| {
            !matches!(
                n.note_event.note_type,
                NoteType::Landmine | NoteType::LongNoteEnd
            )
        })
        .map(|n| n.target_time_seconds)
        .fold(None, |best, t| Some(best.map_or(t, |b: f64| b.min(t))))
}

/// Index of the first note that can still be on screen: the first one whose
/// end (a long note's tail, or the note itself) is not before `cutoff`.
///
/// The notes are sorted by head, not by end, so the ends are not monotonic:
/// a long note that is still running can sit before notes that already ended.
/// A binary search over the ends would skip it, so this scans instead. It stops
/// at the first visible note, so the cost is the number of notes already gone.
pub(crate) fn first_visible_note(notes: &[PlayNote], cutoff: f64) -> usize {
    notes
        .iter()
        .position(|n| n.end_target_time_seconds >= cutoff)
        .unwrap_or(notes.len())
}

/// Close to failing mid-song. Only survival gauges (Hard / Hazard) can end
/// the stage early; Easy / Groove are judged at the end, so a low value there
/// is not a danger worth flashing red.
fn is_danger(score: &ScoreTracker) -> bool {
    match score.gauge_type {
        GaugeType::Hard => score.gauge < 30.0,
        GaugeType::Hazard => score.gauge < 100.0,
        GaugeType::Easy | GaugeType::Groove => false,
    }
}

/// Where the gauge and the HUD sit around the playfield (screen pixels).
///
/// The HUD is two slim blocks: `song` (title, level, progress) and `score`
/// (EX score, judgement pills, timeline, spectrum). A centred playfield puts
/// `song` on the left and `score` on the right; a playfield at either edge
/// puts both in one column on the opposite side, `song` on top. `hint` is the
/// key hint, along the bottom of whichever column holds `song`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hud {
    pub gauge: Rect,
    pub song: Rect,
    pub score: Rect,
    pub hint: Rect,
    /// The free part of the song column between the song block and the key
    /// hint (empty when the song shares its column with the score).
    pub art: Rect,
}

/// Widest a HUD column gets, in 720p pixels: slim, so the field stays the focus.
const HUD_COLUMN_MAX: f32 = 300.0;
/// Space between the playfield (or its gauge) and a HUD column (720p pixels).
const HUD_GAP: f32 = 48.0;
/// Margin between the viewport edge and a HUD column (720p pixels).
const HUD_EDGE: f32 = 24.0;
/// Height of the song block, from its top (720p pixels).
const HUD_SONG_H: f32 = 116.0;
/// Height of the key hint strip along the bottom (720p pixels).
const HUD_HINT_H: f32 = 36.0;
/// Space between the song block and the score block when they share a column.
const HUD_STACK_GAP: f32 = 14.0;

/// Pure layout of the HUD for a playfield `field` inside `vp` (the 16:9
/// viewport, so letterboxed and pillarboxed windows get the same design).
/// `position` is the playfield's effective position (Double Play is `Left`).
pub(crate) fn hud_layout(vp: &Viewport, field: Rect, position: FieldPosition, s: f32) -> Hud {
    let gauge_x = match position {
        FieldPosition::Right => field.x - 34.0 * s,
        _ => field.right() + 16.0 * s,
    };
    let gauge = Rect::new(gauge_x, field.y + 18.0 * s, 18.0 * s, field.h - 52.0 * s);

    let max_w = HUD_COLUMN_MAX * s;
    let left_edge = vp.x + HUD_EDGE * s;
    let right_edge = vp.x + vp.width - HUD_EDGE * s;
    // Columns beside the field, from the field's own edge outwards.
    let left_col = |x1: f32| {
        let x0 = (x1 - max_w).max(left_edge);
        (x0, (x1 - x0).max(0.0))
    };
    let right_col = |x0: f32| {
        let x1 = (x0 + max_w).min(right_edge);
        (x0, (x1 - x0).max(0.0))
    };
    let (left_x, left_w) = left_col(field.x - HUD_GAP * s);
    let (right_x, right_w) = right_col(field.right() + HUD_GAP * s);

    let song_h = HUD_SONG_H * s;
    let hint_h = HUD_HINT_H * s;
    match position {
        FieldPosition::Center => {
            let art_y = field.y + song_h + 2.0 * HUD_STACK_GAP * s;
            let art_bottom = field.bottom() - hint_h - 2.0 * HUD_STACK_GAP * s;
            Hud {
                gauge,
                song: Rect::new(left_x, field.y, left_w, song_h),
                hint: Rect::new(left_x, field.bottom() - hint_h, left_w, hint_h),
                score: Rect::new(right_x, field.y, right_w, field.h),
                art: Rect::new(left_x, art_y, left_w, (art_bottom - art_y).max(0.0)),
            }
        }
        // The field is at one edge: every HUD piece in one column on the other side.
        FieldPosition::Left | FieldPosition::Right => {
            let (x, w) = match position {
                FieldPosition::Right => (left_x, left_w),
                _ => (right_x, right_w),
            };
            let score_y = field.y + song_h + HUD_STACK_GAP * s;
            let score_bottom = field.bottom() - hint_h - HUD_STACK_GAP * s;
            Hud {
                gauge,
                song: Rect::new(x, field.y, w, song_h),
                score: Rect::new(x, score_y, w, (score_bottom - score_y).max(0.0)),
                hint: Rect::new(x, field.bottom() - hint_h, w, hint_h),
                art: Rect::new(x, score_y, 0.0, 0.0),
            }
        }
    }
}

/// Fits a `w`×`h` image into `dst` by cropping (cover) and returns the UVs.
pub(crate) fn cover_uv(tex: SizedTexture, dst: Rect) -> [f32; 4] {
    let (iw, ih) = (tex.width.max(1) as f32, tex.height.max(1) as f32);
    let (ia, da) = (iw / ih, dst.w / dst.h.max(1.0));
    if ia > da {
        let u = da / ia;
        [(1.0 - u) / 2.0, 0.0, (1.0 + u) / 2.0, 1.0]
    } else {
        let v = ia / da;
        [0.0, (1.0 - v) / 2.0, 1.0, (1.0 + v) / 2.0]
    }
}

/// Whether the chart's BGA is drawn behind the playfield (a dimmed,
/// cover-fitted backdrop at the track BGA opacity, so the lanes sit over it).
fn bga_backdrop_visible(f: &PlayFrame) -> bool {
    f.track_bga_opacity > 0.0 && f.bga.is_some()
}

/// The chart's BGA (and layer) over `full` at `alpha`.
fn draw_bga(c: &mut Canvas, f: &PlayFrame, full: Rect, alpha: f32) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    for tex in [f.bga, f.layer].into_iter().flatten() {
        c.image(
            tex.id,
            full,
            cover_uv(tex, full),
            theme::WHITE.with_alpha(a),
        );
    }
}

fn backdrop(c: &mut Canvas, sk: &Skin, f: &PlayFrame, lite: bool) {
    let vp = f.viewport;
    let full = Rect::new(vp.x, vp.y, vp.width, vp.height);
    let opacity = f.track_bga_opacity.clamp(0.0, 1.0);
    // The BGA is the chart's content, not a decoration, so it stays under
    // `lite` too (one quad). Without a BGA the generated backdrop shows.
    if lite {
        if bga_backdrop_visible(f) {
            draw_bga(c, f, full, opacity);
        }
        return; // the frame clear color is the backdrop
    }
    if bga_backdrop_visible(f) {
        // The BGA goes down opaque, then the gradient over it at the rest of
        // the opacity: the BGA shows at the track opacity over the backdrop,
        // and the frame stays at one batch per texture (BGA, then atlas).
        draw_bga(c, f, full, 1.0);
        let veil = ((1.0 - opacity) * 255.0) as u8;
        c.fill_rect_vgradient(
            full,
            theme::BG.with_alpha(veil),
            theme::SURF1.with_alpha(veil),
        );
    } else {
        c.fill_rect_vgradient(full, theme::BG, theme::SURF1);
    }
    // Decorative full-screen layers: each costs a full-screen of fill, which
    // the WARP fallback cannot afford (see `Ui::lite`).
    c.set_additive(true);
    let ambient = theme::BLUE;
    c.sprite_centered(
        sk.glow,
        vp.x + vp.width * 0.8,
        vp.y + vp.height * 0.15,
        vp.width * 0.9,
        vp.height * 1.1,
        ambient.with_alpha(28),
    );
    c.tile(sk.noise, full, theme::WHITE.with_alpha(6));
    c.set_additive(false);
    c.sprite(sk.vignette, full, theme::WHITE.with_alpha(200));
}

fn playfield(c: &mut Canvas, sk: &Skin, f: &PlayFrame, field: Rect, danger: bool, s: f32) {
    let l = f.layout;
    let judge_y = l.judge_line_y;
    let hair = s.max(1.0);

    // Field body (see-through over a BGA, so the notes stay readable on the
    // dark lanes), then the top fade.
    let body_alpha = if bga_backdrop_visible(f) { 196 } else { 238 };
    c.fill_rect(field, theme::PLAYFIELD.with_alpha(body_alpha));
    c.fill_rect_vgradient(
        Rect::new(field.x, field.y, field.w, 160.0 * s),
        theme::PLAYFIELD,
        theme::PLAYFIELD.with_alpha(0),
    );

    // Double play: show the gap between the 1P and 2P sides as background.
    let dp_gap = match l.play_mode {
        PlayMode::Keys10 => Some(Lane::Key5),
        PlayMode::Keys14 => Some(Lane::Key7),
        _ => None,
    }
    .map(|last_1p| {
        let x0 = l.lane_x(last_1p) + l.lane_width(last_1p);
        Rect::from_ltrb(x0, field.y, l.lane_x(Lane::P2Key1), field.bottom())
    });
    if let Some(gap) = dp_gap {
        c.fill_rect(gap, theme::BG);
    }

    // Lane separators and frame.
    let sep = if f.high_contrast {
        theme::MUTED.with_alpha(230)
    } else {
        theme::LINE.with_alpha(150)
    };
    // A separator on each lane's left edge but the field's own (the scratch
    // may be on either side, so that is not always the first lane).
    for &lane in l.active_lanes() {
        let x = l.lane_x(lane);
        if x > field.x + 0.5 {
            c.fill_rect(Rect::new(x, field.y, hair, judge_y - field.y), sep);
        }
    }
    let edge = if danger { theme::RED } else { theme::LINE };
    c.fill_rect(
        Rect::new(field.x - 2.0 * hair, field.y, 2.0 * hair, field.h),
        edge,
    );
    c.fill_rect(Rect::new(field.right(), field.y, 2.0 * hair, field.h), edge);
    if let Some(gap) = dp_gap {
        c.fill_rect(Rect::new(gap.x, gap.y, 2.0 * hair, gap.h), edge);
        c.fill_rect(
            Rect::new(gap.right() - 2.0 * hair, gap.y, 2.0 * hair, gap.h),
            edge,
        );
    }
    if danger && !f.reduce_flashes {
        c.set_additive(true);
        for x in [field.x, field.right()] {
            c.sprite_centered(
                sk.glow,
                x,
                field.y + field.h / 2.0,
                40.0 * s,
                field.h * 1.2,
                theme::RED.with_alpha(70),
            );
        }
        c.set_additive(false);
    }

    // Scroll by beat distance, not time: hi_speed is px/s at `scroll_bpm`
    // (the chosen anchor BPM), so a BPM change speeds the notes up/down and a
    // STOP freezes them.
    let speed = l.hi_speed * s;
    let px_per_beat = speed as f64 * 60.0 / f.scroll_bpm.max(1.0);
    let now_beat = f
        .timing
        .time_to_beat_position(note_clock(f.audio_time, f.visual_offset_ms));
    let y_at_beat = |beat: f64| judge_y - ((beat - now_beat) * px_per_beat) as f32;
    let y_at = |time: f64| y_at_beat(f.timing.time_to_beat_position(time));

    // Measure lines.
    c.push_clip(Rect::from_ltrb(field.x, field.y, field.right(), judge_y));
    let (beat_measure, _) = f.timing.time_to_beat(f.audio_time);
    for measure in beat_measure.saturating_sub(1)..=f.chart.max_measure + 1 {
        let y = y_at_beat(f.timing.beat_position(measure, 0.0));
        if y < field.y {
            break;
        }
        if y <= judge_y {
            c.fill_rect(
                Rect::new(field.x, y.round(), field.w, hair),
                theme::WHITE.with_alpha(34),
            );
        }
    }

    // Key beams (under notes).
    c.set_additive(true);
    for &lane in l.active_lanes() {
        if f.key_pressed[lane_index(lane)] {
            let beam_h = (judge_y - field.y) * 0.55;
            let col = l.lane_color(lane);
            c.sprite(
                sk.beam,
                Rect::new(l.lane_x(lane), judge_y - beam_h, l.lane_width(lane), beam_h),
                col.with_alpha(if col == theme::NOTE_WHITE { 90 } else { 150 }),
            );
        }
    }
    c.set_additive(false);

    // Notes. Long-note stripes flow at two periods per beat; the beat comes
    // from the timing model, so the flow follows the audio clock (INV-1).
    let note_h = sk.note.region.h as f32;
    let flow_phase = (now_beat * 2.0).rem_euclid(1.0) as f32;
    let visible_beats = (judge_y - field.y + 100.0 * s) as f64 / px_per_beat.max(1.0);
    let first = first_visible_note(f.notes, f.audio_time - 2.0);
    // The 8K trigger form's side-track notes are wide bars over half the
    // field: draw them first so a key note at the same time sits on top
    // instead of being hidden under the bar.
    for side_pass in [true, false] {
        for note in &f.notes[first..] {
            if f.timing.time_to_beat_position(note.target_time_seconds) > now_beat + visible_beats {
                break;
            }
            let lane = note.note_event.lane;
            if (l.eight_k_triggers() && is_side_track(lane)) != side_pass {
                continue;
            }
            let (x, w) = (l.lane_x(lane) + 2.0 * hair, l.lane_width(lane) - 4.0 * hair);
            // Scratch heads run a little wider than the lane, like the bar they are.
            let (x, w, head_kind) = if is_scratch_lane(lane) {
                (x - 2.0 * s, w + 4.0 * s, Head::Scratch)
            } else {
                (x, w, Head::Key)
            };
            let col = l.lane_color(lane);
            // High-contrast colours of this lane's heads, bodies and tails: `Some`
            // with the scratch flag when the setting is on.
            let contrast = f.high_contrast.then_some(head_kind == Head::Scratch);
            let head_y = y_at(note.target_time_seconds);
            match note.note_event.note_type {
                // A judged note is gone: it does not linger on the line while it scrolls past.
                NoteType::Tap if !note.is_judged => draw_head(
                    c,
                    sk,
                    head_kind,
                    Rect::new(x, head_y - note_h, w, note_h),
                    col,
                    contrast,
                ),
                // A mine: a slim red bar with a dark core, gone once it has gone off.
                NoteType::Landmine if !note.is_judged => {
                    let h = note_h * 0.7;
                    let y = head_y - (note_h + h) / 2.0;
                    draw_head(c, sk, Head::Key, Rect::new(x, y, w, h), theme::RED, None);
                    c.fill_rect(
                        Rect::new(x + w * 0.2, y + h * 0.35, w * 0.6, h * 0.3),
                        theme::BG.with_alpha(200),
                    );
                }
                NoteType::LongNoteStart => {
                    let tail_y = y_at(note.end_target_time_seconds);
                    let held = note.is_holding;
                    // Head judged but not held and not yet at its end: missed or let go
                    // early. What is left of it keeps scrolling, dimmed, instead of
                    // sticking to the judge line like a held note.
                    let broken = note.is_judged
                        && !held
                        && f.audio_time < note.end_target_time_seconds - 0.15;
                    if note.is_judged && !held && !broken {
                        continue;
                    }
                    let head_edge = if broken { head_y } else { head_y.min(judge_y) };
                    let (top, bottom) = (tail_y.max(field.y), head_edge.min(judge_y));
                    let body = Rect::from_ltrb(x + 3.0 * s, top, x + w - 3.0 * s, bottom);
                    // Bodies and tails take the heads' high-contrast colours too.
                    let body_col = match contrast {
                        Some(scratch) => high_contrast_note(col, scratch).0,
                        None => col,
                    };
                    if bottom > top {
                        if broken {
                            c.nine(&sk.ln_body, body, body_col.with_alpha(40));
                        } else {
                            ln_body(c, sk, body, body_col, held, flow_phase);
                        }
                    }
                    if !broken {
                        draw_head(
                            c,
                            sk,
                            head_kind,
                            Rect::new(x, head_edge - note_h, w, note_h),
                            col,
                            contrast,
                        );
                    }
                    draw_head(
                        c,
                        sk,
                        Head::Tail,
                        Rect::new(x, tail_y - note_h, w, note_h),
                        if broken { col.with_alpha(70) } else { col },
                        contrast,
                    );
                }
                _ => {}
            }
        }
    }
    c.pop_clip();

    // Lane cover (SUDDEN+).
    if l.lane_cover_ratio > 0.0 {
        let h = field.h * l.lane_cover_ratio.clamp(0.0, 0.85);
        let cover = Rect::new(field.x, field.y, field.w, h);
        c.fill_rect_vgradient(cover, theme::SURF1, theme::SURF2);
        c.fill_rect(
            Rect::new(field.x, cover.bottom() - 2.0 * hair, field.w, 2.0 * hair),
            theme::CYAN,
        );
    }

    // Judge line: a bright core with a hot top edge, and a glow that swells
    // a little on each beat. The pulse only touches the additive glow, so it
    // never covers a note.
    let line_col = match f.score.current_combo {
        c if c >= 200 => theme::GOLD,
        c if c >= 50 => theme::CYAN,
        _ => theme::MAGENTA,
    };
    // High contrast: a thicker line that does not pulse.
    let (pulse, core) = if f.high_contrast {
        (0.0, 5.0)
    } else {
        (beat_pulse(now_beat), 3.0)
    };
    c.fill_rect(Rect::new(field.x, judge_y, field.w, core * hair), line_col);
    c.fill_rect(
        Rect::new(field.x, judge_y, field.w, hair),
        theme::WHITE.with_alpha(230),
    );
    let glow_h = 27.0 * s * (1.0 + 0.25 * pulse);
    c.set_additive(true);
    c.sprite(
        sk.glow,
        Rect::new(
            field.x - 16.0 * s,
            judge_y + 1.5 * hair - glow_h / 2.0,
            field.w + 32.0 * s,
            glow_h,
        ),
        line_col.with_alpha((80.0 + 70.0 * pulse) as u8),
    );
    c.set_additive(false);

    // Key indicators below the judge line.
    let key_top = judge_y + 8.0 * s;
    let key_h = (field.bottom() - key_top - 8.0 * s).max(6.0 * s);
    c.fill_rect_vgradient(
        Rect::from_ltrb(field.x, judge_y + 3.0 * hair, field.right(), field.bottom()),
        theme::SURF1,
        theme::BG,
    );
    // 8K side tracks: a wide strip under the six keys.
    let side_h = if l.eight_k_triggers() {
        (key_h * 0.34).max(6.0 * s)
    } else {
        0.0
    };
    for &lane in l.active_lanes() {
        let pressed = f.key_pressed[lane_index(lane)];
        let mut r = Rect::new(
            l.lane_x(lane) + 4.0 * s,
            key_top,
            l.lane_width(lane) - 8.0 * s,
            key_h,
        );
        if side_h > 0.0 {
            if is_side_track(lane) {
                r.y = key_top + key_h - side_h;
                r.h = side_h;
            } else {
                r.h = key_h - side_h - 4.0 * s;
            }
        }
        let col = l.lane_color(lane);
        c.nine(
            &sk.panel_sm,
            r,
            if pressed { col } else { col.with_alpha(26) },
        );
        if pressed {
            c.set_additive(true);
            c.sprite_centered(
                sk.glow,
                r.x + r.w / 2.0,
                r.y + r.h / 2.0,
                r.w * 2.0,
                r.h * 2.5,
                col.with_alpha(90),
            );
            c.set_additive(false);
        }
    }

    if let Some(gap) = dp_gap {
        c.fill_rect(
            gap.inset_by(crate::canvas::Insets::new(2.0 * hair, 0.0, 2.0 * hair, 0.0)),
            theme::BG,
        );
    }

    hit_bursts(c, sk, f, judge_y, s);
}

/// Which note head sprite a playfield note uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Head {
    Key,
    Scratch,
    Tail,
}

fn is_scratch_lane(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::P2Scratch)
}

/// The clock the notes are drawn on: the audio time moved by the visual offset.
/// Only the drawing reads it. Judgement and the audio clock never do (INV-1).
pub(crate) fn note_clock(audio_time: f64, visual_offset_ms: f64) -> f64 {
    audio_time + visual_offset_ms / 1000.0
}

/// Brightness of a hit burst: half when flashes are reduced.
pub(crate) fn burst_gain(reduce_flashes: bool) -> f32 {
    if reduce_flashes {
        0.5
    } else {
        1.0
    }
}

/// Judge-line pulse at `beat` (a beat position from the timing model, so
/// it follows the audio clock and every BPM change). Full on each beat and
/// decaying to nothing before the next, so it reads as a soft beat, not a
/// flash. Always in 0..=1.
pub(crate) fn beat_pulse(beat: f64) -> f32 {
    let phase = beat.rem_euclid(1.0) as f32;
    (1.0 - phase).powi(4)
}

/// Outline width of a high-contrast note head, in 720-unit pixels.
const HC_OUTLINE_PX: f32 = 3.0;

/// The high-contrast body and outline colours for a note of colour `col`:
/// pale grey with a white outline on the white keys, a saturated sky blue
/// on the blue keys and a saturated magenta on the scratch, each with a
/// bright outline. Judgement and hit effects keep their own colours.
pub(crate) fn high_contrast_note(col: ColorRgba, scratch: bool) -> (ColorRgba, ColorRgba) {
    if scratch {
        (theme::NOTE_SCRATCH_CONTRAST, theme::WHITE)
    } else if u16::from(col.r) + u16::from(col.g) + u16::from(col.b) > 600 {
        (theme::NOTE_WHITE_CONTRAST, theme::WHITE)
    } else {
        (theme::NOTE_BLUE_CONTRAST, theme::WHITE)
    }
}

/// One note head: the shape for `head`, a gloss on the keys and scratch, and
/// the chevron mark on a scratch. The tail cap is lit from below and has no
/// gloss, so it reads as an end. `contrast` is `Some(scratch)` for the
/// high-contrast colours of a note in that lane kind.
fn draw_head(
    c: &mut Canvas,
    sk: &Skin,
    head: Head,
    r: Rect,
    col: ColorRgba,
    contrast: Option<bool>,
) {
    let shape = match head {
        Head::Key => &sk.note,
        Head::Scratch => &sk.note_scratch,
        Head::Tail => &sk.note_tail,
    };
    // The outline is a ring inside the lane: it takes the outer HC_OUTLINE_PX of
    // the head's rectangle, so it never reaches into the next lane.
    let (r, col) = match contrast {
        Some(scratch) => {
            let (body, outline) = high_contrast_note(col, scratch);
            let w = HC_OUTLINE_PX * sk.scale;
            c.nine(shape, r, outline.with_alpha(col.a));
            (r.inset(w), body.with_alpha(col.a))
        }
        None => (r, col),
    };
    c.nine(shape, r, col);
    if head != Head::Tail {
        c.set_additive(true);
        c.nine(&sk.note_gloss, r, theme::WHITE);
        c.set_additive(false);
    }
    if head == Head::Scratch {
        c.sprite_centered(
            sk.scratch_mark,
            r.x + r.w / 2.0,
            r.y + r.h / 2.0,
            sk.scratch_mark.w as f32,
            sk.scratch_mark.h as f32,
            theme::WHITE.with_alpha(190),
        );
    }
}

/// Long-note body: the crisp rails of `ln_body`, then the flowing stripes
/// tiled down the body. `phase` (0..1, one period per tile) moves the
/// stripes; a held body is brighter and gets an additive glow over it.
fn ln_body(c: &mut Canvas, sk: &Skin, body: Rect, col: ColorRgba, held: bool, phase: f32) {
    let base = if held { 200 } else { 140 };
    c.nine(&sk.ln_body, body, col.with_alpha(base));
    let tile = sk.ln_flow.h as f32;
    // Tiles in [body.y - tile, body.bottom]; `phase` shifts them down one tile per period.
    let mut top = body.y + phase * tile - tile;
    c.push_clip(body);
    while top < body.bottom() {
        c.sprite(
            sk.ln_flow,
            Rect::new(body.x, top, body.w, tile),
            col.with_alpha(base),
        );
        top += tile;
    }
    if held {
        c.set_additive(true);
        let mut top = body.y + phase * tile - tile;
        while top < body.bottom() {
            c.sprite(
                sk.ln_flow,
                Rect::new(body.x, top, body.w, tile),
                col.with_alpha(90),
            );
            top += tile;
        }
        c.set_additive(false);
    }
    c.pop_clip();
}

fn hit_bursts(c: &mut Canvas, sk: &Skin, f: &PlayFrame, judge_y: f32, s: f32) {
    let l = f.layout;
    c.set_additive(true);
    for burst in f.hit_bursts {
        let elapsed = f.audio_time - burst.spawn_time;
        if !(0.0..BURST_SECONDS).contains(&elapsed) {
            continue;
        }
        let p = (elapsed / BURST_SECONDS) as f32;
        let fade = (1.0 - ease_in_cubic(p)) * burst_gain(f.reduce_flashes);
        let col = theme::judge_color(burst.grade);
        let (lx, lw) = (l.lane_x(burst.lane), l.lane_width(burst.lane));
        let cx = lx + lw / 2.0;
        let a = |k: f32| (255.0 * fade * k).clamp(0.0, 255.0) as u8;

        if burst.grade == JudgeGrade::PerfectGreat {
            let h = (150.0 * s).min(judge_y - l.playfield_y);
            c.fill_rect_vgradient(
                Rect::new(lx, judge_y - h, lw, h),
                col.with_alpha(0),
                col.with_alpha(a(0.22)),
            );
        }
        let ring = lerp(30.0, 88.0, ease_out_cubic(p)) * s;
        c.sprite_centered(sk.ring, cx, judge_y, ring, ring, col.with_alpha(a(0.9)));
        let flare = lerp(130.0, 60.0, ease_out_cubic(p)) * s;
        c.sprite_centered(
            sk.flare,
            cx,
            judge_y,
            flare,
            flare,
            theme::WHITE.with_alpha(a(0.85)),
        );
        let dist = ease_out_quad(p) * 46.0 * s;
        for k in 0..6 {
            let ang = std::f32::consts::PI * (0.12 + 0.152 * k as f32);
            let (dx, dy) = (ang.cos() * dist, -ang.sin() * dist);
            let size = (10.0 - 5.0 * p) * s;
            c.sprite_centered(
                sk.glow,
                cx + dx,
                judge_y + dy,
                size,
                size,
                col.with_alpha(a(1.0)),
            );
        }
    }
    c.set_additive(false);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn gauge(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &PlayFrame,
    r: Rect,
    danger: bool,
    s: f32,
) {
    let score = f.score;
    c.nine(&sk.panel_sm, r, theme::SURF2);
    let col = match score.gauge_type {
        GaugeType::Easy | GaugeType::Groove if score.gauge >= 80.0 => theme::CYAN,
        GaugeType::Easy => theme::GREEN,
        GaugeType::Groove => theme::BLUE,
        GaugeType::Hard if danger || score.gauge < 30.0 => theme::RED,
        GaugeType::Hard => theme::ORANGE,
        GaugeType::Hazard => theme::MAGENTA,
    };
    let segs = 50;
    let inner = r.inset(3.0 * s);
    let seg_h = inner.h / segs as f32;
    let lit = ((score.gauge / 100.0).clamp(0.0, 1.0) * segs as f64).round() as usize;
    for i in 0..segs {
        let y = inner.bottom() - (i + 1) as f32 * seg_h;
        let on = i < lit;
        let seg_col =
            if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) && i >= 40 && on {
                theme::MAGENTA
            } else {
                col
            };
        c.fill_rect(
            Rect::new(inner.x, y + 1.0, inner.w, seg_h - 1.0),
            if on { seg_col } else { seg_col.with_alpha(28) },
        );
    }
    if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
        let y = inner.bottom() - inner.h * 0.8;
        c.fill_rect(
            Rect::new(r.x - 4.0 * s, y - s, r.w + 8.0 * s, 2.0 * s),
            theme::GOLD,
        );
    }
    let pct = format!("{:.0}%", score.gauge.floor());
    t.draw_in(
        c,
        &pct,
        Rect::new(
            r.x - 12.0 * s,
            r.bottom() + 6.0 * s,
            r.w + 24.0 * s,
            20.0 * s,
        ),
        Align::Center,
        &TextStyle::new(13.0 * s)
            .bold()
            .color(if danger { theme::RED } else { theme::TEXT }),
    );
}

fn combo_and_judge(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &PlayFrame,
    field: Rect,
    s: f32,
) {
    let cx = field.x + field.w / 2.0;
    let anchor = f.layout.judge_line_y - f.layout.judge_position.anchor_720() * s;
    let since = f.last_judge.map(|(_, at, _)| f.audio_time - at);
    let combo_shown =
        f.score.current_combo > 0 && since.is_some_and(|e| (0.0..COMBO_SECONDS).contains(&e));

    if combo_shown || since.is_some_and(|e| (0.0..JUDGE_SECONDS).contains(&e)) {
        // Soft dark pool so text stays legible over notes. Off the Upper
        // position it sits over the notes just above the judge line, so it is
        // much fainter there.
        let pool_alpha = if f.layout.judge_position == JudgePosition::Upper {
            150
        } else {
            60
        };
        c.sprite_centered(
            sk.glow,
            cx,
            anchor + 30.0 * s,
            field.w * 0.9,
            190.0 * s,
            theme::BLACK.with_alpha(pool_alpha),
        );
    }

    if combo_shown {
        let pulse = match since {
            Some(e) if (0.0..0.12).contains(&e) => {
                1.0 + 0.12 * (1.0 - ease_out_cubic((e / 0.12) as f32))
            }
            _ => 1.0,
        };
        let txt = thousands(f.score.current_combo);
        let st = TextStyle::new(52.0 * s).bold().color(theme::TEXT);
        let w = t.measure(c, &txt, &st) * pulse;
        let fx = Fx {
            outline: Some((theme::BLACK, 2.0 * s)),
            gradient: None,
        };
        t.draw_fx(c, &txt, cx - w / 2.0, anchor, &st, pulse, fx);
        let cap = caption(12.0, s).color(theme::MUTED);
        let cw = t.measure(c, strings::COMBO(), &cap);
        t.draw(c, strings::COMBO(), cx - cw / 2.0, anchor + 18.0 * s, &cap);
    }

    let Some((grade, at, delta_ms)) = f.last_judge else {
        return;
    };
    let e = f.audio_time - at;
    if !(0.0..JUDGE_SECONDS).contains(&e) {
        return;
    }
    let pop = 1.0 + 0.3 * (1.0 - ease_out_back((e / 0.09).min(1.0) as f32));
    let fade_from = JUDGE_SECONDS - 0.14;
    let alpha = if e > fade_from {
        1.0 - ease_in_cubic(((e - fade_from) / 0.14) as f32)
    } else {
        1.0
    };
    let a = (alpha * 255.0) as u8;
    let col = theme::judge_color(grade);
    let label = theme::judge_label(grade);
    let y = anchor + JUDGE_WORD_BASELINE_720 * s;
    let st = TextStyle::new(34.0 * s)
        .bold()
        .tracking(3.0 * s)
        .color(col.with_alpha(a));
    let w = t.measure(c, label, &st) * pop;
    // PERFECT GREAT is the one grade with a gradient fill (gold on top,
    // white at the foot); the others are one judgement color.
    let gradient = (grade == JudgeGrade::PerfectGreat)
        .then(|| (theme::GOLD.with_alpha(a), theme::WHITE.with_alpha(a)));
    if grade == JudgeGrade::PerfectGreat {
        c.set_additive(true);
        c.sprite_centered(
            sk.glow,
            cx,
            y - 12.0 * s,
            w * 1.5,
            80.0 * s,
            theme::GOLD.with_alpha((alpha * 90.0) as u8),
        );
        c.set_additive(false);
    }
    let fx = Fx {
        outline: Some((theme::BLACK.with_alpha(a), 2.0 * s)),
        gradient,
    };
    t.draw_fx(c, label, cx - w / 2.0, y, &st, pop, fx);

    // FAST / SLOW: a small horizontal tag right of the judgement word.
    if grade != JudgeGrade::Miss && delta_ms.abs() >= 4.0 {
        let (txt, fs_col) = if delta_ms < 0.0 {
            (format!("FAST {:.0}ms", delta_ms.abs()), theme::FAST)
        } else {
            (format!("SLOW {:.0}ms", delta_ms), theme::SLOW)
        };
        let st = TextStyle::new(12.0 * s).bold().color(fs_col.with_alpha(a));
        let tw = t.measure(c, &txt, &st);
        let pad = 8.0 * s;
        let tag = Rect::new(
            cx + w / 2.0 + 8.0 * s,
            y - 22.0 * s,
            tw + 2.0 * pad,
            18.0 * s,
        );
        c.nine(
            &sk.panel_sm,
            tag,
            theme::BLACK.with_alpha((alpha * 150.0) as u8),
        );
        c.nine(&sk.panel_outline, tag, fs_col.with_alpha(a));
        t.draw_in(c, &txt, tag, Align::Center, &st);
    }
}

/// Draws the song and score blocks and the key hint, each if its column is
/// wide enough to read.
fn hud_panels(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, hud: &Hud, s: f32) {
    let min_w = 160.0 * s;
    // A dark see-through scrim behind the text (no border), so the HUD still
    // reads over a bright BGA.
    let pad = 8.0 * s;
    for r in [hud.song, hud.score] {
        if r.w >= min_w {
            c.fill_rect(
                Rect::new(r.x - pad, r.y - pad, r.w + 2.0 * pad, r.h + 2.0 * pad),
                theme::BG.with_alpha(150),
            );
        }
    }
    if hud.song.w >= min_w {
        song_block(c, t, sk, f, hud.song, s);
    }
    // Without a BGA of the chart's own the column under the song block would be empty.
    if !f.chart_has_bga && hud.art.w >= min_w && hud.art.h >= 120.0 * s {
        jacket_card(c, t, sk, f, hud.art, s);
    }
    if hud.score.w >= min_w {
        score_block(c, t, sk, f, hud.score, s);
    }
    if hud.hint.w >= min_w && f.key_hint_alpha > 0.0 {
        key_hint(c, t, f, hud.hint, s);
    }
}

/// The song's stage image in `area` (4:3, at the top of it), or a card in the
/// level colour with the mode when the song has none.
fn jacket_card(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, area: Rect, s: f32) {
    let h = (area.w * 0.75).min(area.h);
    let w = h / 0.75;
    let r = Rect::new(area.x, area.y, w, h);
    c.halo(&sk.shadow, r, theme::WHITE.with_alpha(140));
    match f.jacket {
        Some(tex) => {
            c.fill_rect(r, theme::BG);
            c.image(tex.id, r, cover_uv(tex, r), theme::WHITE);
        }
        None => {
            let (_, tier_col) =
                theme::level_tier(f.chart.header.difficulty, f.chart.header.play_level);
            c.fill_rect_corners(
                r,
                [tier_col, theme::SURF3, theme::BG, tier_col.with_alpha(160)],
            );
            t.draw_in(
                c,
                theme::mode_label(f.layout.play_mode),
                r.inset(16.0 * s),
                Align::Left,
                &TextStyle::new(56.0 * s)
                    .bold()
                    .color(theme::WHITE.with_alpha(48)),
            );
        }
    }
    c.stroke_rect(r, s.max(1.0), theme::LINE);

    // Under the card, while there is room: the chart's numbers.
    let header = &f.chart.header;
    let rank = match header.rank {
        0 => "VERY HARD",
        1 => "HARD",
        3 => "EASY",
        _ => "NORMAL",
    };
    let total = if header.total > 0.0 {
        format!("{:.0}", header.total)
    } else {
        "-".to_string()
    };
    let rows = [
        ("NOTES", f.score.total_notes.to_string()),
        ("GAUGE", f.score.gauge_type.as_str().to_string()),
        ("JUDGE", rank.to_string()),
        ("TOTAL", total),
    ];
    let cap = caption(12.0, s).color(theme::MUTED);
    let val = TextStyle::new(13.0 * s).bold().color(theme::TEXT);
    let row_h = 26.0 * s;
    let mut y = r.bottom() + 24.0 * s;
    for (label, value) in rows {
        if y + row_h > area.bottom() {
            break;
        }
        t.draw(c, label, area.x, y + 16.0 * s, &cap);
        t.draw_in(
            c,
            &value,
            Rect::new(area.x, y, area.w, row_h),
            Align::Right,
            &val,
        );
        c.fill_rect(
            Rect::new(area.x, y + row_h, area.w, s.max(1.0)),
            theme::LINE,
        );
        y += row_h + 4.0 * s;
    }
}

/// Difficulty chip, title, artist and BPM, and the song progress bar with its
/// clock. The badge (AUTO PLAY, REPLAY) sits at the right end of the top row.
fn song_block(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, r: Rect, s: f32) {
    let header = &f.chart.header;
    let (tier, tier_col) = theme::level_tier(header.difficulty, header.play_level);
    let chip_txt = format!("{tier}  {}", header.play_level);
    let chip_st = TextStyle::new(12.0 * s).bold().color(theme::ON_ACCENT);
    let chip_w = t.measure(c, &chip_txt, &chip_st) + 16.0 * s;
    let chip = Rect::new(r.x, r.y, chip_w, 20.0 * s);
    c.nine(&sk.panel_lg, chip, tier_col);
    t.draw_in(c, &chip_txt, chip, Align::Center, &chip_st);

    if let Some(badge) = f.badge {
        let st = caption(12.0, s).color(theme::ON_ACCENT);
        let badge = t.fit(c, badge, r.w * 0.5, &st).into_owned();
        let bw = t.measure(c, &badge, &st) + 16.0 * s;
        let at = Rect::new(r.right() - bw, r.y, bw, 20.0 * s);
        c.nine(&sk.panel_lg, at, theme::CYAN);
        t.draw_in(c, &badge, at, Align::Center, &st);
    }

    let title_st = TextStyle::new(22.0 * s).bold().color(theme::TEXT);
    let title = t.fit(c, &header.title, r.w, &title_st).into_owned();
    t.draw(c, &title, r.x, r.y + 46.0 * s, &title_st);
    // The BPM now, and for a chart that changes its BPM, the range it plays
    // over the song (the same span as the clock).
    let now = f.timing.bpm_at_time(f.audio_time).round();
    let stats = f.timing.bpm_stats(f.song_length);
    let (lo, hi) = (stats.min.round(), stats.max.round());
    let bpm_txt = if lo == hi {
        format!("BPM {now}")
    } else {
        format!("BPM {now} ({lo}-{hi})")
    };
    let sub = format!("{}  /  {bpm_txt}", header.artist);
    let sub_st = TextStyle::new(13.0 * s).color(theme::MUTED);
    let sub = t.fit(c, &sub, r.w, &sub_st).into_owned();
    t.draw(c, &sub, r.x, r.y + 68.0 * s, &sub_st);

    let prog = Rect::new(r.x, r.y + 88.0 * s, r.w, 3.0 * s);
    c.fill_rect(prog, theme::LINE);
    let ratio = if f.song_length > 0.0 {
        (f.audio_time / f.song_length).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    c.fill_rect_hgradient(
        Rect::new(prog.x, prog.y, prog.w * ratio, prog.h),
        theme::CYAN,
        theme::MAGENTA,
    );
    let clock = format!(
        "{} / {}",
        theme::clock(f.audio_time),
        theme::clock(f.song_length)
    );
    t.draw_in(
        c,
        &clock,
        Rect::new(r.x, r.y + 96.0 * s, r.w, 16.0 * s),
        Align::Right,
        &caption(12.0, s).color(theme::MUTED),
    );
}

/// The EX score with the rank and the pace, the judgement counts as coloured
/// pills, the judgement timeline and the spectrum along the bottom. No boxes:
/// the numbers sit on the field's own dark.
fn score_block(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, r: Rect, s: f32) {
    let score = f.score;
    let judged = score.pgreat_count
        + score.great_count
        + score.good_count
        + score.bad_count
        + score.poor_count
        + score.miss_count;
    // Rank, accuracy and pacemaker are all relative to the notes judged so
    // far (ScoreTracker::accuracy_rate divides by the whole chart, which
    // would show "F" for most of a perfect run).
    let acc = if judged > 0 {
        score.ex_score as f64 / (judged as f64 * 2.0) * 100.0
    } else {
        100.0
    };
    let (rank, rank_col) = theme::rank(acc);

    // Caption row: the label, and the next rank's name with the EX still needed.
    t.draw(c, "EX SCORE", r.x, r.y + 14.0 * s, &caption(12.0, s));
    if let Some((name, need)) = next_rank(score.ex_score, judged) {
        let next = strings::fill(strings::NEXT_RANK(), &[&format!("{name} +{need}")]);
        t.draw_in(
            c,
            &next,
            Rect::new(r.x, r.y, r.w, 16.0 * s),
            Align::Right,
            &TextStyle::new(12.0 * s).bold().color(theme::CYAN),
        );
    }

    // The score itself, with the maximum beside it.
    let ex = thousands(score.ex_score);
    let ex_w = t.draw(
        c,
        &ex,
        r.x,
        r.y + 58.0 * s,
        &TextStyle::new(46.0 * s).bold().color(theme::TEXT),
    );
    let max = format!("/ {}", thousands(score.max_ex_score()));
    t.draw(
        c,
        &max,
        r.x + ex_w + 8.0 * s,
        r.y + 58.0 * s,
        &TextStyle::new(13.0 * s).color(theme::MUTED2),
    );

    // Rank letter, then the pacemaker difference and the accuracy.
    let rank_w = t.draw(
        c,
        rank,
        r.x,
        r.y + 92.0 * s,
        &TextStyle::new(30.0 * s).bold().color(rank_col),
    );
    let target = (judged as f64 * 2.0 * 8.0 / 9.0).round() as i64;
    let diff = score.ex_score as i64 - target;
    let pace_x = r.x + rank_w + 12.0 * s;
    let pace_w = t.draw(
        c,
        strings::PACE(),
        pace_x,
        r.y + 92.0 * s,
        &caption(12.0, s).color(theme::MUTED),
    );
    let diff_txt = format!("{}{}", if diff >= 0 { "+" } else { "" }, diff);
    t.draw(
        c,
        &diff_txt,
        pace_x + pace_w + 6.0 * s,
        r.y + 92.0 * s,
        &TextStyle::new(13.0 * s).bold().color(if diff >= 0 {
            theme::CYAN
        } else {
            theme::MAGENTA
        }),
    );
    t.draw_in(
        c,
        &format!("{acc:.2}%"),
        Rect::new(r.x, r.y + 80.0 * s, r.w, 18.0 * s),
        Align::Right,
        &TextStyle::new(13.0 * s).bold().color(theme::TEXT),
    );

    // Judgement counts: a thin distribution bar, then a 2×3 grid of pills.
    let bar = Rect::new(r.x, r.y + 102.0 * s, r.w, 4.0 * s);
    let counts = [
        (JudgeGrade::PerfectGreat, score.pgreat_count),
        (JudgeGrade::Great, score.great_count),
        (JudgeGrade::Good, score.good_count),
        (JudgeGrade::Bad, score.bad_count),
        (JudgeGrade::Poor, score.poor_count),
        (JudgeGrade::Miss, score.miss_count),
    ];
    let total = counts.iter().map(|c| c.1).sum::<u32>();
    c.fill_rect(bar, theme::LINE);
    if total > 0 {
        let mut bx = bar.x;
        for (g, n) in counts {
            let sw = bar.w * n as f32 / total as f32;
            if sw > 0.0 {
                c.fill_rect(
                    Rect::new(bx, bar.y, (sw - s).max(s), bar.h),
                    theme::judge_color(g),
                );
            }
            bx += sw;
        }
    }
    let gap = 6.0 * s;
    let pill_h = 26.0 * s;
    let pill_w = (r.w - gap) / 2.0;
    let grid_y = r.y + 116.0 * s;
    for (i, (g, n)) in counts.iter().enumerate() {
        let pill = Rect::new(
            r.x + (i % 2) as f32 * (pill_w + gap),
            grid_y + (i / 2) as f32 * (pill_h + gap),
            pill_w,
            pill_h,
        );
        let col = theme::judge_color(*g);
        c.nine(&sk.panel_sm, pill, col.with_alpha(40));
        c.sprite(
            sk.icons.dot,
            Rect::new(
                pill.x + 8.0 * s,
                pill.y + (pill_h - 8.0 * s) / 2.0,
                8.0 * s,
                8.0 * s,
            ),
            col,
        );
        t.draw_in(
            c,
            theme::judge_label(*g),
            Rect::new(pill.x + 22.0 * s, pill.y, pill.w - 30.0 * s, pill_h),
            Align::Left,
            &caption(12.0, s).color(theme::MUTED),
        );
        t.draw_in(
            c,
            &thousands(*n),
            Rect::new(pill.x, pill.y, pill.w - 8.0 * s, pill_h),
            Align::Right,
            &TextStyle::new(13.0 * s).bold().color(theme::TEXT),
        );
    }

    // The timeline is quiet: a caption and a hairline, no box. It only shows
    // when the column is tall enough to keep it clear of the spectrum.
    let strip_y = r.y + 232.0 * s;
    let vis_h = 32.0 * s;
    if strip_y + 44.0 * s <= r.bottom() - vis_h - 8.0 * s {
        t.draw(
            c,
            strings::TIMELINE_TITLE(),
            r.x,
            strip_y,
            &caption(12.0, s).color(theme::MUTED),
        );
        t.draw_in(
            c,
            strings::TIMELINE_RANGE(),
            Rect::new(r.x, strip_y - 12.0 * s, r.w, 14.0 * s),
            Align::Right,
            &caption(12.0, s).color(theme::MUTED2),
        );
        judge_strip(
            c,
            f,
            Rect::from_ltrb(r.x, strip_y + 10.0 * s, r.right(), strip_y + 44.0 * s),
            s,
        );
    }

    // Under the timeline, when the column has room: max combo, FAST / SLOW,
    // and the timing histogram, so the column does not sit empty.
    let stats_y = strip_y + 64.0 * s;
    if stats_y + 96.0 * s <= r.bottom() - vis_h - 16.0 * s {
        timing_stats(c, t, f, Rect::new(r.x, stats_y, r.w, 96.0 * s), s);
    }

    if r.h >= vis_h {
        spectrum(c, f, Rect::new(r.x, r.bottom() - vis_h, r.w, vis_h), s);
    }
}

/// MAX COMBO and FAST / SLOW on one line, then the timing histogram (early on
/// the left, late on the right) as bars over a centre line.
fn timing_stats(c: &mut Canvas, t: &mut TextEngine, f: &PlayFrame, r: Rect, s: f32) {
    let score = f.score;
    let cap = caption(12.0, s).color(theme::MUTED);
    let val = TextStyle::new(13.0 * s).bold().color(theme::TEXT);
    let third = r.w / 3.0;
    for (i, (label, value, col)) in [
        ("MAX COMBO", score.max_combo, theme::TEXT),
        ("FAST", score.fast_count, theme::CYAN),
        ("SLOW", score.slow_count, theme::MAGENTA),
    ]
    .into_iter()
    .enumerate()
    {
        let x = r.x + i as f32 * third;
        t.draw(c, label, x, r.y, &cap);
        t.draw(c, &value.to_string(), x, r.y + 20.0 * s, &val.color(col));
    }
    let plot = Rect::from_ltrb(r.x, r.y + 40.0 * s, r.right(), r.bottom());
    let bins = &score.timing_histogram;
    let peak = bins.iter().copied().max().unwrap_or(0).max(1) as f32;
    let n = bins.len() as f32;
    let gap = 2.0 * s;
    let bw = (plot.w - gap * (n - 1.0)) / n;
    c.fill_rect(
        Rect::new(plot.x, plot.bottom() - s.max(1.0), plot.w, s.max(1.0)),
        theme::LINE,
    );
    let mid = (bins.len() / 2) as f32;
    for (i, &count) in bins.iter().enumerate() {
        let h = (plot.h * count as f32 / peak).max(if count > 0 { 2.0 * s } else { 0.0 });
        let col = if (i as f32 - mid).abs() < 1.0 {
            theme::TEXT
        } else if (i as f32) < mid {
            theme::CYAN
        } else {
            theme::MAGENTA
        };
        let x = plot.x + i as f32 * (bw + gap);
        c.fill_rect(Rect::new(x, plot.bottom() - h, bw, h), col.with_alpha(180));
    }
}

/// The judgements of the last `JUDGE_TIMELINE_SECONDS` on `plot`. Time runs
/// right to left; the vertical position is the timing (early above the line,
/// late below it, at the edges for 150 ms).
fn judge_strip(c: &mut Canvas, f: &PlayFrame, plot: Rect, s: f32) {
    let mid = plot.y + plot.h / 2.0;
    let half = plot.h / 2.0 - 4.0 * s;
    c.fill_rect(Rect::new(plot.x, mid, plot.w, s.max(1.0)), theme::LINE);
    // Slim ticks: a judgement is a thin mark, so a run of them reads as a
    // timeline and not as a bar.
    let (tw, th) = (2.5 * s, 8.0 * s);
    for m in f.judge_marks {
        let e = f.audio_time - m.time;
        if !(0.0..JUDGE_TIMELINE_SECONDS).contains(&e) {
            continue;
        }
        let x = plot.x + plot.w * (1.0 - e / JUDGE_TIMELINE_SECONDS) as f32;
        let offset = (m.delta_ms / 150.0).clamp(-1.0, 1.0) as f32;
        let y = mid + offset * half;
        c.fill_rect(
            Rect::new(x - tw / 2.0, y - th / 2.0, tw, th),
            theme::judge_color(m.grade),
        );
    }
}

/// The visualizer bars filling `r`, one per level.
fn spectrum(c: &mut Canvas, f: &PlayFrame, r: Rect, s: f32) {
    let n = f.visual_levels.len() as f32;
    let gap = 3.0 * s;
    let bw = (r.w - gap * (n - 1.0)) / n;
    c.set_additive(true);
    for (i, &lvl) in f.visual_levels.iter().enumerate() {
        let lvl = lvl.clamp(0.0, 1.0);
        let h = (r.h * lvl).max(2.0 * s);
        let bx = r.x + i as f32 * (bw + gap);
        let col = if lvl > 0.8 {
            theme::MAGENTA
        } else {
            theme::CYAN
        };
        c.fill_rect_vgradient(
            Rect::new(bx, r.bottom() - h, bw, h),
            col.with_alpha(160),
            col.with_alpha(20),
        );
    }
    c.set_additive(false);
}

/// The key hint along the bottom of `col`, broken between its groups when a
/// narrow column cannot hold it on one line.
fn key_hint(c: &mut Canvas, t: &mut TextEngine, f: &PlayFrame, col: Rect, s: f32) {
    let (x, w) = (col.x, col.w);
    let hint_st = TextStyle::new(11.0 * s)
        .color(theme::MUTED2.with_alpha((f.key_hint_alpha.min(1.0) * 255.0) as u8));
    // Too long for a narrow column: break between the hint's groups (they
    // are four spaces apart) rather than inside one.
    let group_break = (t.measure(c, f.hint, &hint_st) > w)
        .then(|| {
            f.hint
                .match_indices("    ")
                .map(|(i, _)| i)
                .filter(|&i| t.measure(c, &f.hint[..i], &hint_st) <= w)
                .last()
        })
        .flatten();
    let lines = match group_break {
        Some(i) => (
            f.hint[..i].to_string(),
            Some(t.fit(c, f.hint[i..].trim_start(), w, &hint_st).into_owned()),
        ),
        None => widgets::wrap2(c, t, f.hint, w, &hint_st),
    };
    match lines {
        (line, None) => {
            t.draw(c, &line, x, col.bottom() - 6.0 * s, &hint_st);
        }
        (first, Some(second)) => {
            t.draw(c, &first, x, col.bottom() - 22.0 * s, &hint_st);
            t.draw(c, &second, x, col.bottom() - 6.0 * s, &hint_st);
        }
    }
}

/// The rank above the one `ex` EX score earns over `judged` notes, and the EX
/// score still needed for it. `None` with no notes judged, or at AAA. Uses the
/// same IIDX ninths as `theme::rank`: a rank is reached at `ex * 9 >= n * max`.
pub(crate) fn next_rank(ex: u32, judged: u32) -> Option<(&'static str, u32)> {
    if judged == 0 {
        return None;
    }
    // (ninths, rank name), from D up to AAA.
    const LADDER: [(u64, &str); 6] = [
        (3, "D"),
        (4, "C"),
        (5, "B"),
        (6, "A"),
        (7, "AA"),
        (8, "AAA"),
    ];
    let max = judged as u64 * 2;
    let ex = ex as u64;
    LADDER.iter().find_map(|&(n, name)| {
        // The smallest EX score with `ex * 9 >= n * max`, rounded up.
        let need = (n * max + 8) / 9;
        (ex < need).then(|| (name, (need - ex) as u32))
    })
}

/// The start cue over the lane before the first note (see `start_cue`).
fn ready(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, field: Rect, s: f32) {
    let Some(first) = f.first_note else {
        return;
    };
    match start_cue_on(f.timing, f.audio_time, first) {
        StartCue::None => {}
        StartCue::Ready(a) => ready_label(c, t, sk, field, a, s),
        StartCue::Count { number, progress } => count_in(c, t, sk, field, number, progress, s),
        StartCue::Go(p) => go_flash(c, t, sk, field, p, s),
    }
}

/// READY with its hint, in the upper third of the lane (the notes and the
/// combo are below it), at opacity `a`.
fn ready_label(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, field: Rect, a: f32, s: f32) {
    let cx = field.x + field.w / 2.0;
    let y = field.y + field.h * 0.22;
    let alpha = (a * 255.0) as u8;
    c.sprite_centered(
        sk.glow,
        cx,
        y + 30.0 * s,
        field.w * 0.9,
        150.0 * s,
        theme::BLACK.with_alpha((a * 150.0) as u8),
    );
    let st = TextStyle::new(40.0 * s)
        .bold()
        .tracking(4.0 * s)
        .color(theme::TEXT.with_alpha(alpha));
    let w = t.measure(c, strings::READY(), &st);
    t.draw(c, strings::READY(), cx - w / 2.0, y, &st);
    let hint = caption(13.0, s).color(theme::MUTED.with_alpha(alpha));
    let hw = t.measure(c, strings::READY_HINT(), &hint);
    t.draw(c, strings::READY_HINT(), cx - hw / 2.0, y + 52.0 * s, &hint);
}

/// One count-in number in the middle of the field: it punches in and fades
/// over its beat, and a soft ring swells out behind it.
fn count_in(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    field: Rect,
    number: u8,
    p: f32,
    s: f32,
) {
    let cx = field.x + field.w / 2.0;
    let cy = field.y + field.h * 0.4;
    let alpha = count_alpha(p);
    // The ring: starts at the number and swells, fading as it goes.
    let ring = lerp(70.0, 230.0, ease_out_cubic(p)) * s;
    c.set_additive(true);
    c.sprite_centered(
        sk.ring,
        cx,
        cy,
        ring,
        ring,
        theme::CYAN.with_alpha(((1.0 - p).powi(2) * 170.0) as u8),
    );
    c.set_additive(false);
    c.sprite_centered(
        sk.glow,
        cx,
        cy,
        field.w * 0.6,
        200.0 * s,
        theme::BLACK.with_alpha((alpha * 120.0) as u8),
    );
    let text = number.to_string();
    let size = 150.0 * s;
    let pop = punch(p, 0.45, 0.18);
    let a = (alpha * 255.0) as u8;
    let st = TextStyle::new(size).bold().color(theme::TEXT.with_alpha(a));
    let fx = Fx {
        outline: Some((theme::BLACK.with_alpha(a), 4.0 * s)),
        gradient: Some((theme::WHITE.with_alpha(a), theme::CYAN.with_alpha(a))),
    };
    let w = t.measure(c, &text, &st) * pop;
    t.draw_fx(c, &text, cx - w / 2.0, cy - size * 0.5 * pop, &st, pop, fx);
}

/// GO! at the first note: it punches in and fades over `GO_SECONDS`.
fn go_flash(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, field: Rect, p: f32, s: f32) {
    let cx = field.x + field.w / 2.0;
    let cy = field.y + field.h * 0.4;
    let alpha = 1.0 - ease_in_cubic(p.clamp(0.0, 1.0));
    let ring = lerp(90.0, 300.0, ease_out_cubic(p)) * s;
    c.set_additive(true);
    c.sprite_centered(
        sk.ring,
        cx,
        cy,
        ring,
        ring,
        theme::GOLD.with_alpha(((1.0 - p).powi(2) * 200.0) as u8),
    );
    c.set_additive(false);
    let pop = punch(p, 0.3, 0.3);
    let a = (alpha * 255.0) as u8;
    let size = 110.0 * s;
    let st = TextStyle::new(size)
        .bold()
        .tracking(4.0 * s)
        .color(theme::TEXT.with_alpha(a));
    let fx = Fx {
        outline: Some((theme::BLACK.with_alpha(a), 4.0 * s)),
        gradient: Some((theme::GOLD.with_alpha(a), theme::WHITE.with_alpha(a))),
    };
    let w = t.measure(c, strings::GO(), &st) * pop;
    t.draw_fx(
        c,
        strings::GO(),
        cx - w / 2.0,
        cy - size * 0.5 * pop,
        &st,
        pop,
        fx,
    );
}

/// The value the player just changed, near the top of the lane.
fn readout(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, field: Rect, s: f32) {
    let Some((text, at)) = f.readout else {
        return;
    };
    let e = f.audio_time - at;
    if !(0.0..READOUT_SECONDS).contains(&e) {
        return;
    }
    let fade_from = READOUT_SECONDS - 0.3;
    let alpha = if e > fade_from {
        1.0 - ease_in_cubic(((e - fade_from) / 0.3) as f32)
    } else {
        1.0
    };
    let cx = field.x + field.w / 2.0;
    let y = field.y + 22.0 * s;
    let st = TextStyle::new(20.0 * s)
        .bold()
        .color(theme::CYAN.with_alpha((alpha * 255.0) as u8));
    let w = t.measure(c, text, &st);
    c.sprite_centered(
        sk.glow,
        cx,
        y + 12.0 * s,
        w + 60.0 * s,
        44.0 * s,
        theme::BLACK.with_alpha((alpha * 150.0) as u8),
    );
    t.draw(c, text, cx - w / 2.0, y, &st);
}

/// The end banner over the middle of the playfield: the clear lamp of the
/// finished play (`ClearType::as_str`), in `banner_color`. It pops in, holds,
/// and fades (`banner_fade`). FULL COMBO and PERFECT add a gold burst and a
/// light sweep; FAILED shakes and darkens the field behind it.
fn end_banner(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, field: Rect, s: f32) {
    let Some((lamp, progress)) = f.banner else {
        return;
    };
    let p = progress.clamp(0.0, 1.0);
    let alpha = banner_fade(p);
    let a = (alpha * 255.0) as u8;
    let color = banner_color(lamp);
    let cx = field.x + field.w / 2.0;
    let failed = lamp == ClearType::Failed;
    let mut cy = field.y + field.h * 0.5;
    if failed {
        // Darken the playfield edges, and shake the banner down the lane.
        c.sprite(
            sk.vignette,
            field,
            theme::BLACK.with_alpha((fail_vignette(p) * alpha * 255.0) as u8),
        );
        c.fill_rect(
            field,
            theme::BLACK.with_alpha((fail_vignette(p) * 0.5 * alpha * 255.0) as u8),
        );
        if !f.reduce_flashes {
            cy += fail_shake(p) * s;
        }
    }
    let top = matches!(lamp, ClearType::FullCombo | ClearType::Perfect);
    c.sprite_centered(
        sk.glow,
        cx,
        cy,
        field.w * 0.95,
        160.0 * s,
        theme::BLACK.with_alpha((alpha * 170.0) as u8),
    );
    if top {
        c.set_additive(true);
        // A ring of gold sparks flies out from the word once, over the middle of the banner.
        let q = ((p - 0.2) / 0.4).clamp(0.0, 1.0);
        if (0.0..1.0).contains(&q) {
            let dist = ease_out_quad(q) * 190.0 * s;
            let size = (16.0 - 10.0 * q) * s;
            let col = theme::GOLD.with_alpha(((1.0 - q) * alpha * 255.0) as u8);
            for k in 0..12 {
                let ang = std::f32::consts::TAU * k as f32 / 12.0;
                c.sprite_centered(
                    sk.glow,
                    cx + ang.cos() * dist * 1.8,
                    cy - ang.sin() * dist * 0.5,
                    size,
                    size,
                    col,
                );
            }
        }
        // A light sweep crosses the word from left to right.
        if let Some(u) = banner_sweep(p).filter(|_| !f.reduce_flashes) {
            let x = field.x - 0.1 * field.w + u * 1.2 * field.w;
            let peak = (u * std::f32::consts::PI).sin();
            c.sprite_centered(
                sk.glow,
                x,
                cy,
                90.0 * s,
                150.0 * s,
                theme::WHITE.with_alpha((peak * alpha * 230.0) as u8),
            );
        }
        c.set_additive(false);
    }
    let pop = punch(p, 0.3, 0.2);
    let size = if top { 60.0 } else { 44.0 } * s;
    let text = lamp.as_str();
    let st = TextStyle::new(size)
        .bold()
        .tracking(4.0 * s)
        .color(color.with_alpha(a));
    // The word never runs past the playfield: it shrinks to fit the lane.
    let natural = t.measure(c, text, &st).max(1.0);
    let scale = pop.min(field.w * 0.94 / natural);
    let w = natural * scale;
    let fx = Fx {
        outline: Some((theme::BLACK.with_alpha(a), 2.0 * s)),
        gradient: (lamp == ClearType::FullCombo)
            .then(|| (theme::GOLD.with_alpha(a), theme::WHITE.with_alpha(a))),
    };
    t.draw_fx(c, text, cx - w / 2.0, cy - 30.0 * s, &st, scale, fx);
}

fn pause_menu(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &PlayFrame,
    selected: usize,
    s: f32,
    hs: &mut HitSink,
) {
    let vp = f.viewport;
    c.fill_rect(
        Rect::new(vp.x, vp.y, vp.width, vp.height),
        theme::BLACK.with_alpha(190),
    );
    let (w, h) = (440.0 * s, 360.0 * s);
    let panel = Rect::new(
        vp.x + (vp.width - w) / 2.0,
        vp.y + (vp.height - h) / 2.0,
        w,
        h,
    );
    c.halo(&sk.shadow, panel, theme::WHITE);
    c.nine(&sk.panel_lg, panel, theme::SURF1);
    c.fill_rect_hgradient(
        Rect::new(panel.x + 16.0 * s, panel.y, panel.w - 32.0 * s, 2.0 * s),
        theme::CYAN,
        theme::MAGENTA,
    );
    let inner = panel.inset(28.0 * s);

    t.draw(
        c,
        strings::PAUSED(),
        inner.x,
        inner.y + 26.0 * s,
        &TextStyle::new(28.0 * s).bold().color(theme::TEXT),
    );
    let title_st = TextStyle::new(14.0 * s).bold().color(theme::TEXT);
    let title = t
        .fit(c, &f.chart.header.title, inner.w, &title_st)
        .into_owned();
    t.draw(c, &title, inner.x, inner.y + 56.0 * s, &title_st);
    let artist_st = TextStyle::new(12.0 * s).color(theme::MUTED);
    let artist = t
        .fit(c, &f.chart.header.artist, inner.w, &artist_st)
        .into_owned();
    t.draw(c, &artist, inner.x, inner.y + 74.0 * s, &artist_st);

    let bar = Rect::new(inner.x, inner.y + 92.0 * s, inner.w, 4.0 * s);
    let ratio = if f.song_length > 0.0 {
        (f.audio_time / f.song_length).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    c.nine(&sk.panel_sm, bar, theme::LINE);
    c.fill_rect_hgradient(
        Rect::new(bar.x, bar.y, bar.w * ratio, bar.h),
        theme::CYAN,
        theme::MAGENTA,
    );
    let time = format!(
        "{} / {}",
        theme::clock(f.audio_time),
        theme::clock(f.song_length)
    );
    t.draw_in(
        c,
        &time,
        Rect::new(inner.x, bar.bottom() + 4.0 * s, inner.w, 18.0 * s),
        Align::Right,
        &TextStyle::new(11.0 * s).color(theme::MUTED),
    );

    let options = [
        strings::PAUSE_RESUME(),
        strings::PAUSE_RESTART(),
        strings::PAUSE_QUIT(),
    ];
    let mut y = bar.bottom() + 36.0 * s;
    for (i, label) in options.iter().enumerate() {
        let row = Rect::new(inner.x, y, inner.w, 40.0 * s);
        hs.add(row, HitId::PauseRow(i));
        let on = i == selected || hs.hovered(row);
        if on {
            c.nine(&sk.panel, row, theme::CYAN.with_alpha(30));
            c.nine(&sk.panel_outline, row, theme::CYAN.with_alpha(200));
            c.sprite(
                sk.icons.chevron_right,
                Rect::new(row.x + 8.0 * s, row.y + 10.0 * s, 20.0 * s, 20.0 * s),
                theme::CYAN,
            );
        }
        t.draw_in(
            c,
            label,
            Rect::new(row.x + 36.0 * s, row.y, row.w - 44.0 * s, row.h),
            Align::Left,
            &TextStyle::new(14.0 * s)
                .bold()
                .color(if on { theme::TEXT } else { theme::MUTED }),
        );
        y += 46.0 * s;
    }
    t.draw_in(
        c,
        strings::PAUSE_HINT(),
        Rect::new(inner.x, panel.bottom() - 34.0 * s, inner.w, 20.0 * s),
        Align::Center,
        &caption(12.0, s),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{BmsHeader, JudgeEngine};

    struct Fixture {
        vp: Viewport,
        layout: SkinConfig,
        chart: BmsChart,
        timing: TimingModel,
        judge: JudgeEngine,
        score: ScoreTracker,
    }

    fn fixture() -> Fixture {
        let vp = Viewport::new(1280, 720);
        let mut layout = SkinConfig::default();
        layout.update_layout(&vp);
        let chart = BmsChart {
            header: BmsHeader {
                title: "Test Beat".into(),
                artist: "Tester".into(),
                bpm: 150.0,
                play_level: 12,
                ..Default::default()
            },
            max_measure: 50,
            ..Default::default()
        };
        let timing = TimingModel::from_chart(&chart);
        let judge = JudgeEngine::new(&chart, &timing, GaugeType::Groove, beetle_core::Ruleset::CN);
        let mut score = ScoreTracker::new(100, 200.0, GaugeType::Groove);
        for _ in 0..30 {
            score.record_hit(JudgeGrade::PerfectGreat);
        }
        score.record_hit(JudgeGrade::Great);
        Fixture {
            vp,
            layout,
            chart,
            timing,
            judge,
            score,
        }
    }

    #[test]
    fn whole_frame_is_one_batch_without_bga() {
        let fx = fixture();
        let mut keys = [false; LANE_COUNT];
        keys[lane_index(Lane::Key3)] = true;
        let bursts = [HitBurst {
            lane: Lane::Key3,
            spawn_time: 1.0,
            grade: JudgeGrade::PerfectGreat,
        }];
        let marks = [JudgeMark {
            time: 0.9,
            grade: JudgeGrade::Good,
            delta_ms: 40.0,
        }];
        let mut ui = Ui::new(fx.vp.scale);
        // Plain frames, then a frame with every HUD piece on at once
        // (READY, readout, judge marks, key hint, end banner).
        let cases = [
            (None, false, None, None),
            (Some(1), false, None, None),
            (
                None,
                true,
                Some(("그린 500 ms", 1.0)),
                Some((ClearType::FullCombo, 0.4)),
            ),
        ];
        for (pause, busy, readout, banner) in cases {
            ui.begin(1280, 720, fx.vp.scale);
            draw_gameplay(
                &mut ui,
                &PlayFrame {
                    first_note: None,
                    viewport: &fx.vp,
                    layout: &fx.layout,
                    chart: &fx.chart,
                    notes: fx.judge.notes(),
                    timing: &fx.timing,
                    score: &fx.score,
                    audio_time: 1.05,
                    song_length: 120.0,
                    visual_levels: &[0.5; 16],
                    bga: None,
                    jacket: None,
                    chart_has_bga: false,
                    scroll_bpm: 120.0,
                    layer: None,
                    track_bga_opacity: 0.3,
                    key_pressed: &keys,
                    hit_bursts: &bursts,
                    last_judge: Some((JudgeGrade::PerfectGreat, 1.0, -6.0)),
                    hint: "ESC 일시정지",
                    badge: Some(strings::AUTO_PLAY()),
                    pause,
                    key_hint_alpha: if busy { 0.5 } else { 1.0 },
                    readout,
                    banner,
                    judge_marks: if busy { &marks } else { &[] },
                    visual_offset_ms: 0.0,
                    reduce_flashes: false,
                    high_contrast: false,
                },
            );
            assert_eq!(
                ui.canvas.debug_batches().len(),
                1,
                "pause={pause:?} busy={busy}"
            );
        }
    }

    #[test]
    fn high_contrast_colours_differ_from_the_default_ones() {
        let (body, outline) = high_contrast_note(theme::NOTE_WHITE, false);
        assert_eq!(
            (body, outline),
            (theme::NOTE_WHITE_CONTRAST, theme::WHITE),
            "white keys: pale body, white outline"
        );
        assert_ne!(body, theme::NOTE_WHITE);
        let (body, outline) = high_contrast_note(theme::NOTE_BLUE, false);
        assert_eq!((body, outline), (theme::NOTE_BLUE_CONTRAST, theme::WHITE));
        assert_ne!(body, theme::NOTE_BLUE);
        let (body, outline) = high_contrast_note(theme::NOTE_SCRATCH, true);
        assert_eq!(
            (body, outline),
            (theme::NOTE_SCRATCH_CONTRAST, theme::WHITE)
        );
        assert_ne!(body, theme::NOTE_SCRATCH);
    }

    #[test]
    fn busy_frame_with_many_notes_and_a_milestone_is_one_batch() {
        let fx = fixture();
        let lanes = [
            Lane::Scratch,
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
            Lane::Key6,
            Lane::Key7,
        ];
        // Dense: a note every 45 ms over 8 lanes, every fourth one a long note,
        // and a few long notes held.
        let notes: Vec<PlayNote> = (0..400u32)
            .map(|i| {
                let t0 = 1.0 + i as f64 * 0.045;
                let mut n = play_note(t0, t0);
                n.note_event.lane = lanes[(i % 8) as usize];
                if i % 4 == 0 {
                    n.note_event.note_type = NoteType::LongNoteStart;
                    n.end_target_time_seconds = t0 + 0.5;
                    n.is_holding = i % 8 == 0;
                }
                n
            })
            .collect();
        // 100 PERFECT GREATs in a row: combo 100 is a milestone.
        let mut score = ScoreTracker::new(100, 200.0, GaugeType::Groove);
        for _ in 0..100 {
            score.record_hit(JudgeGrade::PerfectGreat);
        }
        let keys = [true; LANE_COUNT];
        let marks: Vec<JudgeMark> = (0..12)
            .map(|i| JudgeMark {
                time: 0.5 + i as f64 * 0.04,
                grade: JudgeGrade::Great,
                delta_ms: i as f64 - 6.0,
            })
            .collect();
        let bursts: Vec<HitBurst> = lanes
            .iter()
            .map(|&lane| HitBurst {
                lane,
                spawn_time: 1.9,
                grade: JudgeGrade::PerfectGreat,
            })
            .collect();
        let mut ui = Ui::new(fx.vp.scale);
        // Lite (WARP) and high-contrast notes each keep the frame in one batch.
        for (lite, high_contrast) in [(false, false), (true, false), (false, true)] {
            ui.lite = lite;
            ui.begin(1280, 720, fx.vp.scale);
            draw_gameplay(
                &mut ui,
                &PlayFrame {
                    first_note: None,
                    viewport: &fx.vp,
                    layout: &fx.layout,
                    chart: &fx.chart,
                    notes: &notes,
                    timing: &fx.timing,
                    score: &score,
                    audio_time: 2.0,
                    song_length: 120.0,
                    visual_levels: &[0.5; 16],
                    bga: None,
                    jacket: None,
                    chart_has_bga: false,
                    scroll_bpm: 120.0,
                    layer: None,
                    track_bga_opacity: 0.0,
                    key_pressed: &keys,
                    hit_bursts: &bursts,
                    last_judge: Some((JudgeGrade::PerfectGreat, 1.98, -6.0)),
                    hint: "ESC 일시정지",
                    badge: None,
                    pause: None,
                    key_hint_alpha: 1.0,
                    readout: None,
                    banner: None,
                    judge_marks: &marks,
                    visual_offset_ms: 0.0,
                    reduce_flashes: false,
                    high_contrast,
                },
            );
            assert_eq!(
                ui.canvas.debug_batches().len(),
                1,
                "lite={lite} high_contrast={high_contrast}"
            );
            let verts = ui.canvas.debug_vertices().len();
            assert!(verts < 8192, "lite={lite}: {verts} vertices fit one batch");
        }
    }

    #[test]
    fn beat_pulse_peaks_on_the_beat_and_decays_between_beats() {
        assert_eq!(beat_pulse(3.0), 1.0);
        assert_eq!(beat_pulse(4.0), 1.0, "every beat, not only the first");
        assert!(beat_pulse(3.5) < 0.1);
        assert!(beat_pulse(3.9) < beat_pulse(3.5));
        for k in 0..200 {
            let v = beat_pulse(k as f64 * 0.0371 - 2.0);
            assert!((0.0..=1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn ready_waits_for_a_second_before_the_first_note() {
        // Full from the start, gone one second before the note.
        assert_eq!(ready_alpha(0.0, 5.0), 1.0);
        assert_eq!(ready_alpha(3.5, 5.0), 1.0);
        assert!(ready_alpha(3.75, 5.0) > 0.0 && ready_alpha(3.75, 5.0) < 1.0);
        assert_eq!(ready_alpha(4.0, 5.0), 0.0);
        assert_eq!(ready_alpha(4.5, 5.0), 0.0);
        // A first note sooner than 1.5 s gets no READY at all.
        assert_eq!(ready_alpha(0.0, 1.4), 0.0);
        assert_eq!(ready_alpha(-0.5, 5.0), 0.0);
    }

    #[test]
    fn first_note_ignores_landmines_and_long_note_ends() {
        assert_eq!(first_note_time(&[]), None);
        let mut landmine = play_note(1.0, 1.0);
        landmine.note_event.note_type = NoteType::Landmine;
        let mut ln_end = play_note(1.5, 1.5);
        ln_end.note_event.note_type = NoteType::LongNoteEnd;
        assert_eq!(first_note_time(&[landmine, ln_end]), None);
        let tap = play_note(3.0, 3.0);
        assert_eq!(first_note_time(&[tap]), Some(3.0));
    }

    #[test]
    fn first_note_skips_notes_a_practice_start_already_passed() {
        // A practice start judged everything before 5 s: the first note left is 5 s.
        let mut passed = play_note(2.0, 2.0);
        passed.is_judged = true;
        let mut judged_later = play_note(4.0, 4.0);
        judged_later.is_judged = true;
        let notes = [
            passed,
            judged_later,
            play_note(5.0, 5.0),
            play_note(6.0, 6.0),
        ];
        assert_eq!(first_note_time(&notes), Some(5.0));
    }

    /// A note at `head` seconds that ends at `end` (a tap when they are equal).
    fn play_note(head: f64, end: f64) -> PlayNote {
        PlayNote {
            note_event: beetle_core::NoteEvent {
                measure: 0,
                fraction: 0.0,
                lane: Lane::Key1,
                wav_id: None,
                note_type: NoteType::Tap,
            },
            target_time_seconds: head,
            end_target_time_seconds: end,
            is_judged: false,
            is_holding: false,
            tail_index: None,
            head_index: None,
        }
    }

    #[test]
    fn a_long_note_still_running_is_on_screen_past_the_taps_after_it() {
        // Sorted by head: a long note from 2 s to 30 s, then taps that already ended.
        let mut notes = vec![play_note(2.0, 30.0)];
        notes.extend((3..=10).map(|t| play_note(t as f64, t as f64)));
        // At 8 s the long note is still on screen, so nothing before it may be cut.
        assert_eq!(first_visible_note(&notes, 6.0), 0);
        // Once the long note is past the window, the ended taps are skipped.
        assert_eq!(first_visible_note(&notes, 29.0), 0);
        assert_eq!(first_visible_note(&notes, 31.0), notes.len());
        // Taps that ended before the cutoff come off the front.
        assert_eq!(first_visible_note(&notes[1..], 6.0), 3);
    }

    #[test]
    fn only_survival_gauges_signal_danger() {
        let at = |gauge_type, gauge| {
            let mut s = ScoreTracker::new(100, 200.0, gauge_type);
            s.gauge = gauge;
            is_danger(&s)
        };
        assert!(!at(GaugeType::Groove, 2.0));
        assert!(!at(GaugeType::Easy, 2.0));
        assert!(at(GaugeType::Hard, 20.0));
        assert!(!at(GaugeType::Hard, 60.0));
        assert!(at(GaugeType::Hazard, 98.0));
    }

    #[test]
    fn cover_uv_crops_the_long_axis() {
        let tex = SizedTexture {
            id: TextureId(1),
            width: 512,
            height: 512,
        };
        let uv = cover_uv(tex, Rect::new(0.0, 0.0, 160.0, 90.0));
        assert!((uv[0] - 0.0).abs() < 1e-6 && (uv[2] - 1.0).abs() < 1e-6);
        assert!((uv[3] - uv[1] - 0.5625).abs() < 1e-4);
    }

    /// The viewport, the playfield and the HUD for a single play or Double
    /// Play at `w`×`h` (the viewport is letterboxed or pillarboxed to 16:9).
    fn placed(w: u32, h: u32, mode: PlayMode, position: FieldPosition) -> (Viewport, Rect, Hud) {
        let vp = Viewport::new(w, h);
        let mut skin = SkinConfig::default();
        skin.update_layout(&vp);
        skin.set_play_mode(mode);
        skin.set_field_layout(position, crate::skin::ScratchSide::Left);
        let field = Rect::new(
            skin.playfield_x,
            skin.playfield_y,
            skin.playfield_width,
            skin.playfield_height,
        );
        let hud = hud_layout(&vp, field, skin.effective_position(), vp.scale);
        (vp, field, hud)
    }

    const SIZES: [(u32, u32); 4] = [(1280, 720), (1920, 1080), (2560, 1080), (1024, 768)];

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    fn inside(r: Rect, vp: &Viewport) -> bool {
        const EPS: f32 = 0.01;
        r.x >= vp.x - EPS
            && r.y >= vp.y - EPS
            && r.right() <= vp.x + vp.width + EPS
            && r.bottom() <= vp.y + vp.height + EPS
    }

    #[test]
    fn hud_never_overlaps_the_playfield_or_itself() {
        for (w, h) in SIZES {
            for position in [
                FieldPosition::Left,
                FieldPosition::Center,
                FieldPosition::Right,
            ] {
                for mode in [PlayMode::Keys7, PlayMode::Keys14] {
                    let (vp, field, hud) = placed(w, h, mode, position);
                    let case = format!("{w}x{h} {position:?} {mode:?}");
                    let rects = [hud.gauge, hud.song, hud.score, hud.hint];
                    for r in rects {
                        assert!(inside(r, &vp), "{case}: {r:?} leaves the viewport");
                        assert!(!overlaps(r, field), "{case}: {r:?} covers the playfield");
                    }
                    for (i, a) in rects.iter().enumerate() {
                        for b in &rects[i + 1..] {
                            assert!(!overlaps(*a, *b), "{case}: {a:?} overlaps {b:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn hud_never_overlaps_at_the_largest_note_size() {
        for (w, h) in SIZES {
            let vp = Viewport::new(w, h);
            for position in [
                FieldPosition::Left,
                FieldPosition::Center,
                FieldPosition::Right,
            ] {
                for mode in [
                    PlayMode::Keys5,
                    PlayMode::Keys7,
                    PlayMode::Keys8,
                    PlayMode::Keys9,
                    PlayMode::Keys14,
                ] {
                    let mut skin = SkinConfig::default();
                    skin.update_layout(&vp);
                    skin.set_play_mode(mode);
                    skin.set_note_size(crate::skin::NOTE_SIZE_MAX);
                    skin.set_field_layout(position, crate::skin::ScratchSide::Left);
                    let field = Rect::new(
                        skin.playfield_x,
                        skin.playfield_y,
                        skin.playfield_width,
                        skin.playfield_height,
                    );
                    let case = format!("{w}x{h} {position:?} {mode:?} at max note size");
                    assert!(
                        inside(field, &vp),
                        "{case}: field {field:?} leaves the viewport"
                    );
                    let hud = hud_layout(&vp, field, skin.effective_position(), vp.scale);
                    let rects = [hud.gauge, hud.song, hud.score, hud.hint];
                    for r in rects {
                        assert!(inside(r, &vp), "{case}: {r:?} leaves the viewport");
                        assert!(!overlaps(r, field), "{case}: {r:?} covers the playfield");
                    }
                    for (i, a) in rects.iter().enumerate() {
                        for b in &rects[i + 1..] {
                            assert!(!overlaps(*a, *b), "{case}: {a:?} overlaps {b:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn visual_offset_moves_the_note_clock_and_nothing_else() {
        // The drawn notes read `note_clock`; judgement and the audio time do not.
        assert_eq!(note_clock(2.0, 0.0), 2.0);
        assert!((note_clock(2.0, 50.0) - 2.05).abs() < 1e-12);
        assert!((note_clock(2.0, -100.0) - 1.9).abs() < 1e-12);
    }

    #[test]
    fn reduced_flashes_halve_the_bursts_and_keep_the_danger_red_without_blinking() {
        assert_eq!(burst_gain(false), 1.0);
        assert_eq!(burst_gain(true), 0.5);
    }

    #[test]
    fn hud_columns_are_wide_enough_to_read_at_every_size() {
        for (w, h) in SIZES {
            for position in [
                FieldPosition::Left,
                FieldPosition::Center,
                FieldPosition::Right,
            ] {
                let (vp, _, hud) = placed(w, h, PlayMode::Keys7, position);
                let min = 160.0 * vp.scale;
                assert!(
                    hud.song.w >= min,
                    "{w}x{h} {position:?}: song {}",
                    hud.song.w
                );
                assert!(
                    hud.score.w >= min,
                    "{w}x{h} {position:?}: score {}",
                    hud.score.w
                );
                assert!(hud.score.h >= 330.0 * vp.scale, "{w}x{h} {position:?}");
            }
        }
    }

    #[test]
    fn centred_playfield_is_about_38_percent_with_hud_on_both_sides() {
        for (w, h) in SIZES {
            let (vp, field, hud) = placed(w, h, PlayMode::Keys7, FieldPosition::Center);
            let share = field.w / vp.width;
            assert!((0.36..=0.40).contains(&share), "{w}x{h}: {share}");
            let centre = field.x + field.w / 2.0;
            assert!((centre - (vp.x + vp.width / 2.0)).abs() < 1.0, "{w}x{h}");
            assert!(hud.song.right() <= field.x, "{w}x{h}: song on the left");
            assert!(hud.score.x >= field.right(), "{w}x{h}: score on the right");
            assert!(hud.gauge.x >= field.right(), "{w}x{h}: gauge stays right");
        }
    }

    #[test]
    fn an_edge_playfield_moves_every_hud_piece_to_the_other_side() {
        for (w, h) in SIZES {
            let (_, field, hud) = placed(w, h, PlayMode::Keys7, FieldPosition::Left);
            for r in [hud.song, hud.score, hud.hint] {
                assert!(r.x >= field.right(), "{w}x{h} LEFT: {r:?}");
            }
            assert!(hud.gauge.x >= field.right(), "{w}x{h} LEFT gauge");

            let (_, field, hud) = placed(w, h, PlayMode::Keys7, FieldPosition::Right);
            for r in [hud.song, hud.score, hud.hint] {
                assert!(r.right() <= field.x, "{w}x{h} RIGHT: {r:?}");
            }
            assert!(hud.gauge.right() <= field.x, "{w}x{h} RIGHT gauge");
        }
    }

    #[test]
    fn a_long_title_is_fitted_into_the_song_column() {
        let (vp, _, hud) = placed(1024, 768, PlayMode::Keys7, FieldPosition::Center);
        let mut ui = Ui::new(vp.scale);
        ui.begin(1024, 768, vp.scale);
        let st = TextStyle::new(22.0 * vp.scale).bold();
        let title = "Extremely long song title for the fitting test ".repeat(8);
        let fitted = ui
            .text
            .fit(&mut ui.canvas, &title, hud.song.w, &st)
            .into_owned();
        assert!(fitted.len() < title.len(), "the title was shortened");
        let w = ui.text.measure(&mut ui.canvas, &fitted, &st);
        assert!(w <= hud.song.w + 0.5, "{w} > {}", hud.song.w);
    }

    #[test]
    fn next_rank_is_the_first_rank_not_yet_reached() {
        assert_eq!(next_rank(0, 0), None);
        // 10 notes judged: a rank needs `ex * 9 >= n * 20`.
        assert_eq!(next_rank(0, 10), Some(("D", 7)));
        assert_eq!(next_rank(7, 10), Some(("C", 2)));
        assert_eq!(next_rank(17, 10), Some(("AAA", 1)));
        assert_eq!(next_rank(20, 10), None);
    }

    #[test]
    fn busy_frame_with_a_bga_is_at_most_two_batches() {
        let fx = fixture();
        let notes: Vec<PlayNote> = (0..200u32)
            .map(|i| play_note(1.0 + i as f64 * 0.05, 1.0 + i as f64 * 0.05))
            .collect();
        let bga = SizedTexture {
            id: TextureId(7),
            width: 320,
            height: 180,
        };
        let mut ui = Ui::new(fx.vp.scale);
        ui.begin(1280, 720, fx.vp.scale);
        draw_gameplay(
            &mut ui,
            &PlayFrame {
                first_note: None,
                viewport: &fx.vp,
                layout: &fx.layout,
                chart: &fx.chart,
                notes: &notes,
                timing: &fx.timing,
                score: &fx.score,
                audio_time: 2.0,
                song_length: 120.0,
                visual_levels: &[0.5; 16],
                bga: Some(bga),
                jacket: None,
                chart_has_bga: false,
                scroll_bpm: 120.0,
                layer: None,
                track_bga_opacity: 0.5,
                key_pressed: &[false; LANE_COUNT],
                hit_bursts: &[],
                last_judge: None,
                hint: "ESC 일시정지",
                badge: None,
                pause: None,
                key_hint_alpha: 1.0,
                readout: None,
                banner: None,
                judge_marks: &[],
                visual_offset_ms: 0.0,
                reduce_flashes: false,
                high_contrast: false,
            },
        );
        // The backdrop image is one batch, the frame the other.
        assert!(ui.canvas.debug_batches().len() <= 2);
        let verts = ui.canvas.debug_vertices().len();
        assert!(verts < 8192, "{verts} vertices fit one batch");
    }

    /// `start_cue` at a constant tempo: a beat is `beat_seconds`, so beat
    /// positions are the times divided by it.
    fn start_cue(audio_time: f64, first: f64, beat_seconds: f64) -> StartCue {
        super::start_cue(
            audio_time,
            first,
            first / beat_seconds,
            audio_time / beat_seconds,
        )
    }

    /// A chart whose opening tempo is `bpm` and whose timing events are `events`.
    fn timing_of(bpm: f64, events: Vec<beetle_core::bms::TimingEvent>) -> TimingModel {
        let chart = beetle_core::BmsChart {
            header: BmsHeader {
                bpm,
                ..Default::default()
            },
            timing_events: events,
            ..Default::default()
        };
        TimingModel::from_chart(&chart)
    }

    #[test]
    fn the_count_follows_a_tempo_change_before_the_first_note() {
        use beetle_core::bms::{TimingEvent, TimingEventKind};
        // 120 BPM for measure 0 (2 s, 4 beats), then 60 BPM: the first note at
        // measure 2 is beat 8, at 6 s. The count covers beats 5 to 8, which
        // are 3 s to 6 s, one number a second at 60 BPM.
        let timing = timing_of(
            120.0,
            vec![TimingEvent {
                measure: 1,
                fraction: 0.0,
                kind: TimingEventKind::BpmChange(60.0),
            }],
        );
        let first = timing.beat_to_time_seconds(2, 0.0);
        assert_eq!(first, 6.0);
        assert_eq!(start_cue_on(&timing, 2.9, first), StartCue::None);
        assert_eq!(
            start_cue_on(&timing, 3.0, first),
            StartCue::Count {
                number: 3,
                progress: 0.0
            }
        );
        assert!(matches!(
            start_cue_on(&timing, 4.0, first),
            StartCue::Count { number: 2, .. }
        ));
        assert!(matches!(
            start_cue_on(&timing, 5.0, first),
            StartCue::Count { number: 1, .. }
        ));
        assert_eq!(start_cue_on(&timing, 6.0, first), StartCue::Go(0.0));
    }

    #[test]
    fn a_stop_inside_the_count_freezes_its_number() {
        use beetle_core::bms::{TimingEvent, TimingEventKind};
        // 120 BPM with a 2 s STOP at beat 6 (time 3 s). The note is at beat 8,
        // 5 s + 1 s = 6 s. The count starts at beat 5 (2.5 s), and the STOP
        // holds the beat at 6 until 5 s, so "2" stays up through the STOP.
        let timing = timing_of(
            120.0,
            vec![TimingEvent {
                measure: 1,
                fraction: 0.5,
                kind: TimingEventKind::StopMeasures(1.0),
            }],
        );
        let first = timing.beat_to_time_seconds(2, 0.0);
        assert_eq!(first, 6.0);
        assert_eq!(
            start_cue_on(&timing, 2.5, first),
            StartCue::Count {
                number: 3,
                progress: 0.0
            }
        );
        assert_eq!(
            start_cue_on(&timing, 3.5, first),
            StartCue::Count {
                number: 2,
                progress: 0.0
            }
        );
        assert_eq!(
            start_cue_on(&timing, 4.9, first),
            StartCue::Count {
                number: 2,
                progress: 0.0
            }
        );
        assert!(matches!(
            start_cue_on(&timing, 5.5, first),
            StartCue::Count { number: 1, .. }
        ));
        assert_eq!(start_cue_on(&timing, 6.0, first), StartCue::Go(0.0));
    }

    #[test]
    fn the_count_needs_three_beats_before_the_note() {
        // A note two beats in (2 s at 60 BPM): no count, READY fades instead.
        let timing = timing_of(60.0, vec![]);
        assert!(matches!(
            start_cue_on(&timing, 0.0, 2.0),
            StartCue::Ready(a) if a > 0.0
        ));
        assert!(!matches!(
            start_cue_on(&timing, 1.0, 2.0),
            StartCue::Count { .. }
        ));
    }

    #[test]
    fn count_in_runs_over_the_last_three_beats() {
        // First note at 4 s, 120 BPM (a beat is 0.5 s): 3, 2, 1, then GO!.
        let (first, beat) = (4.0, 0.5);
        assert_eq!(start_cue(2.49, first, beat), StartCue::None);
        assert_eq!(
            start_cue(2.5, first, beat),
            StartCue::Count {
                number: 3,
                progress: 0.0
            }
        );
        assert_eq!(
            start_cue(3.25, first, beat),
            StartCue::Count {
                number: 2,
                progress: 0.5
            }
        );
        assert!(matches!(
            start_cue(3.75, first, beat),
            StartCue::Count { number: 1, .. }
        ));
        assert_eq!(start_cue(4.0, first, beat), StartCue::Go(0.0));
        assert_eq!(
            start_cue(4.0 + GO_SECONDS + 0.01, first, beat),
            StartCue::None
        );
    }

    #[test]
    fn a_very_long_lead_in_is_silent_until_its_last_three_beats() {
        assert_eq!(start_cue(0.0, 60.0, 0.5), StartCue::None);
        assert_eq!(start_cue(30.0, 60.0, 0.5), StartCue::None);
        assert!(matches!(
            start_cue(58.75, 60.0, 0.5),
            StartCue::Count { number: 3, .. }
        ));
    }

    #[test]
    fn a_short_lead_in_falls_back_to_ready_then_go() {
        // 1.8 s to the first note is under the 2.0 s floor for numbers, so
        // READY fades out and GO! lands on the note (no numbers).
        assert_eq!(start_cue(0.0, 1.8, 0.5), StartCue::Ready(1.0));
        assert_eq!(start_cue(1.2, 1.8, 0.5), StartCue::None);
        assert_eq!(start_cue(1.8, 1.8, 0.5), StartCue::Go(0.0));
        // Two seconds with 60 BPM: three beats (3 s) do not fit, so READY.
        assert_eq!(start_cue(0.0, 2.0, 1.0), StartCue::Ready(1.0));
    }

    #[test]
    fn no_cue_for_a_first_note_too_soon_or_before_the_song() {
        assert_eq!(start_cue(0.0, 0.0, 0.5), StartCue::None);
        assert_eq!(start_cue(0.0, 1.4, 0.5), StartCue::None);
        assert_eq!(start_cue(-1.0, 4.0, 0.5), StartCue::None);
        // A broken tempo never panics or counts: it falls back to READY.
        assert_eq!(start_cue(0.0, 4.0, 0.0), StartCue::Ready(1.0));
        assert_eq!(start_cue(0.0, 4.0, f64::NAN), StartCue::Ready(1.0));
    }

    #[test]
    fn the_start_cue_never_goes_back() {
        for &(first, beat) in &[(4.0, 0.5), (2.0, 0.5), (1.8, 0.5), (60.0, 0.5), (2.5, 1.0)] {
            let mut last = 0;
            for k in 0..(first * 200.0) as u32 + 100 {
                let rank = match start_cue(k as f64 * 0.005, first, beat) {
                    StartCue::None => continue,
                    StartCue::Ready(_) | StartCue::Count { number: 3, .. } => 1,
                    StartCue::Count { number: 2, .. } => 2,
                    StartCue::Count { .. } => 3,
                    StartCue::Go(_) => 4,
                };
                assert!(rank >= last, "first {first}, beat {beat}: went back at {k}");
                last = rank;
            }
            assert_eq!(last, 4, "first {first}: ends on GO!");
        }
    }

    #[test]
    fn count_alpha_holds_then_fades() {
        assert_eq!(count_alpha(0.0), 1.0);
        assert_eq!(count_alpha(0.7), 1.0);
        assert!(count_alpha(0.85) < 1.0);
        assert!(count_alpha(1.0) < 0.001);
        let mut prev = 1.0;
        for k in 0..=100 {
            let v = count_alpha(k as f32 / 100.0);
            assert!(v <= prev + 1e-6 && (0.0..=1.0).contains(&v));
            prev = v;
        }
    }

    #[test]
    fn punch_starts_big_and_settles_to_one() {
        assert!((punch(0.0, 0.3, 0.2) - 1.3).abs() < 1e-5);
        assert!((punch(0.2, 0.3, 0.2) - 1.0).abs() < 1e-5);
        assert!((punch(0.9, 0.3, 0.2) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_fail_shake_decays_to_nothing() {
        assert_eq!(fail_shake(0.0), 0.0);
        assert_eq!(fail_shake(1.0), 0.0);
        assert!(shake_envelope(0.0) > shake_envelope(0.5));
        assert!(shake_envelope(0.5) > shake_envelope(1.0));
        assert_eq!(shake_envelope(1.0), 0.0);
        for k in 0..=200 {
            let p = k as f32 / 200.0;
            assert!(fail_shake(p).abs() <= shake_envelope(p) + 1e-4, "p={p}");
        }
        assert!(fail_shake(0.95).abs() < 0.1);
    }

    #[test]
    fn banner_fades_in_holds_and_fades_out() {
        assert_eq!(banner_fade(0.0), 0.0);
        assert!((banner_fade(0.1) - 0.5).abs() < 1e-6);
        assert_eq!(banner_fade(0.5), 1.0);
        assert_eq!(banner_fade(1.0), 0.0);
        assert_eq!(banner_fade(2.0), 0.0, "clamped past the end");
    }

    #[test]
    fn the_banner_sweep_crosses_the_middle_only() {
        assert_eq!(banner_sweep(0.1), None);
        assert_eq!(banner_sweep(0.18), Some(0.0));
        assert_eq!(banner_sweep(0.68), Some(1.0));
        assert_eq!(banner_sweep(0.7), None);
    }

    #[test]
    fn fail_vignette_darkens_and_stays_bounded() {
        assert_eq!(fail_vignette(0.0), 0.0);
        let mut prev = 0.0;
        for k in 0..=100 {
            let v = fail_vignette(k as f32 / 100.0);
            assert!((0.0..=0.6).contains(&v) && v + 1e-6 >= prev);
            prev = v;
        }
    }

    #[test]
    fn each_lamp_has_its_banner_colour() {
        assert_eq!(banner_color(ClearType::Failed), theme::RED);
        assert_eq!(banner_color(ClearType::FullCombo), theme::GOLD);
        assert_eq!(banner_color(ClearType::Perfect), theme::GOLD);
        for lamp in [ClearType::Clear, ClearType::Easy, ClearType::Hard] {
            assert_eq!(banner_color(lamp), theme::CYAN, "{lamp:?}");
        }
    }

    /// One frame with the notes and the banner at `audio_time`: asserts it is
    /// one batch and returns its vertex count.
    fn one_frame(
        ui: &mut Ui,
        fx: &Fixture,
        notes: &[PlayNote],
        audio_time: f64,
        banner: Option<(ClearType, f32)>,
    ) -> usize {
        ui.begin(1280, 720, fx.vp.scale);
        draw_gameplay(
            ui,
            &PlayFrame {
                first_note: None,
                viewport: &fx.vp,
                layout: &fx.layout,
                chart: &fx.chart,
                notes,
                timing: &fx.timing,
                score: &fx.score,
                audio_time,
                song_length: 120.0,
                visual_levels: &[0.0; 16],
                bga: None,
                jacket: None,
                chart_has_bga: false,
                scroll_bpm: 120.0,
                layer: None,
                track_bga_opacity: 0.0,
                key_pressed: &[false; LANE_COUNT],
                hit_bursts: &[],
                last_judge: None,
                hint: "ESC 일시정지",
                badge: None,
                pause: None,
                key_hint_alpha: 1.0,
                readout: None,
                banner,
                judge_marks: &[],
                visual_offset_ms: 0.0,
                reduce_flashes: false,
                high_contrast: false,
            },
        );
        assert_eq!(ui.canvas.debug_batches().len(), 1, "t={audio_time}");
        ui.canvas.debug_vertices().len()
    }

    #[test]
    fn start_cues_and_end_banners_are_one_batch_each() {
        let fx = fixture();
        let mut ui = Ui::new(fx.vp.scale);
        let late = [play_note(4.0, 4.0)];
        let short = [play_note(1.8, 1.8)];
        let mut max = 0;
        // Count-in 3, 2, 1, GO!, then the READY fallback, with nothing on top.
        for (notes, t) in [
            (&late[..], 2.9),
            (&late[..], 3.3),
            (&late[..], 3.7),
            (&late[..], 4.1),
            (&short[..], 0.3),
            (&short[..], 1.9),
        ] {
            max = max.max(one_frame(&mut ui, &fx, notes, t, None));
        }
        // Every end lamp, early, mid and late in the banner.
        for lamp in [
            ClearType::Failed,
            ClearType::Easy,
            ClearType::Clear,
            ClearType::Hard,
            ClearType::FullCombo,
            ClearType::Perfect,
        ] {
            for p in [0.05, 0.4, 0.9] {
                max = max.max(one_frame(&mut ui, &fx, &late, 9.0, Some((lamp, p))));
            }
        }
        eprintln!("start cue and end banner frames: max {max} vertices");
        assert!(max < 8192, "{max} vertices fit one batch");
    }
}
