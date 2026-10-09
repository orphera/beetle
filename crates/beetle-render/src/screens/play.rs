//! Gameplay screen on the `Ui` (Canvas + TextEngine + generated Skin).
//! Replaces both `gameplay.rs` (software) and `gameplay_gpu.rs` (D3D11 batcher)
//! — one implementation, typically one or two draw calls per frame
//! (a second only when a BGA texture is on screen).
//!
//! Note positions are derived only from `audio_time` (INV-1).

use crate::art::Skin;
use crate::backend::TextureId;
use crate::canvas::{Canvas, Rect};
use crate::motion::{ease_in_cubic, ease_out_back, ease_out_cubic, ease_out_quad};
use crate::view::{lane_index, HitBurst, Viewport, LANE_COUNT};
use crate::skin::{is_side_track, ColorRgba, FieldPosition, SkinConfig};
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption, thousands};
use crate::ui::Ui;
use super::widgets;
use beetle_core::{
    BmsChart, GaugeType, JudgeGrade, Lane, NoteType, PlayMode, PlayNote, ScoreTracker, TimingModel,
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
    /// Opacity of the BGA drawn underneath the playfield (0 = off).
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
}

const BURST_SECONDS: f64 = 0.28;
const JUDGE_SECONDS: f64 = 0.5;

pub fn draw_gameplay(ui: &mut Ui, f: &PlayFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = f.viewport.scale;
    let l = f.layout;
    let field = Rect::new(l.playfield_x, l.playfield_y, l.playfield_width, l.playfield_height);
    let danger = is_danger(f.score) && (f.audio_time * 6.0).sin() > 0.0;

    let sides = sides(f, field, s);

    backdrop(c, &sk, f, lite);
    playfield(c, &sk, f, field, danger, s);
    gauge(c, t, &sk, f, sides.gauge, danger, s);
    combo_and_judge(c, t, &sk, f, field, s);
    hud(c, t, &sk, f, &sides, s);
    if let Some(selected) = f.pause {
        pause_menu(c, t, &sk, f, selected, s);
    }
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

/// Where the gauge and the HUD columns go around the playfield.
struct Sides {
    gauge: Rect,
    /// Song header and score panel.
    info: Rect,
    /// BGA, spectrum and key hint: a column of their own on the other side
    /// of a centered playfield, otherwise under the info in its column.
    media: Option<Rect>,
}

fn sides(f: &PlayFrame, field: Rect, s: f32) -> Sides {
    let vp = f.viewport;
    let (left, right) = (vp.x + 24.0 * s, vp.x + vp.width - 24.0 * s);
    let column = |x0: f32, x1: f32| Rect::from_ltrb(x0, field.y, x1.max(x0), field.bottom());
    let gauge_at = |x: f32| Rect::new(x, field.y + 18.0 * s, 18.0 * s, field.h - 52.0 * s);
    let gauge_right = gauge_at(field.right() + 16.0 * s);
    match f.layout.effective_position() {
        FieldPosition::Left => Sides { gauge: gauge_right, info: column(field.right() + 56.0 * s, right), media: None },
        FieldPosition::Right => Sides { gauge: gauge_at(field.x - 34.0 * s), info: column(left, field.x - 56.0 * s), media: None },
        FieldPosition::Center => Sides {
            gauge: gauge_right,
            info: column(left, field.x - 56.0 * s),
            media: Some(column(field.right() + 56.0 * s, right)),
        },
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

fn backdrop(c: &mut Canvas, sk: &Skin, f: &PlayFrame, lite: bool) {
    let vp = f.viewport;
    if lite {
        return; // the frame clear color is the backdrop
    }
    let full = Rect::new(vp.x, vp.y, vp.width, vp.height);
    c.fill_rect_vgradient(full, theme::BG, theme::SURF1);
    // Decorative full-screen layers: each costs a full-screen of fill, which
    // the WARP fallback cannot afford (see `Ui::lite`).
    c.set_additive(true);
    let ambient = if f.score.current_combo >= 100 { theme::CYAN } else { theme::BLUE };
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

    // Field body, optional BGA underlay, top fade.
    c.fill_rect(field, theme::PLAYFIELD.with_alpha(238));
    if f.track_bga_opacity > 0.0 {
        let a = (f.track_bga_opacity.clamp(0.0, 1.0) * 255.0) as u8;
        for tex in [f.bga, f.layer].into_iter().flatten() {
            c.image(tex.id, field, cover_uv(tex, field), theme::WHITE.with_alpha(a));
        }
    }
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
    let sep = if f.score.current_combo >= 100 {
        theme::CYAN.with_alpha(55)
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
    c.fill_rect(Rect::new(field.x - 2.0 * hair, field.y, 2.0 * hair, field.h), edge);
    c.fill_rect(Rect::new(field.right(), field.y, 2.0 * hair, field.h), edge);
    if let Some(gap) = dp_gap {
        c.fill_rect(Rect::new(gap.x, gap.y, 2.0 * hair, gap.h), edge);
        c.fill_rect(Rect::new(gap.right() - 2.0 * hair, gap.y, 2.0 * hair, gap.h), edge);
    }
    if danger {
        c.set_additive(true);
        for x in [field.x, field.right()] {
            c.sprite_centered(sk.glow, x, field.y + field.h / 2.0, 40.0 * s, field.h * 1.2, theme::RED.with_alpha(70));
        }
        c.set_additive(false);
    }

    // Scroll by beat distance, not time: hi_speed is px/s at the chart's first
    // BPM, so a BPM change speeds the notes up/down and a STOP freezes them.
    let speed = l.hi_speed * s;
    let px_per_beat = speed as f64 * 60.0 / f.timing.initial_bpm().max(1.0);
    let now_beat = f.timing.time_to_beat_position(f.audio_time);
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
            c.fill_rect(Rect::new(field.x, y.round(), field.w, hair), theme::WHITE.with_alpha(34));
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

    // Notes.
    let note_h = sk.note.region.h as f32;
    let visible_beats = (judge_y - field.y + 100.0 * s) as f64 / px_per_beat.max(1.0);
    let first = f.notes.partition_point(|n| n.end_target_time_seconds < f.audio_time - 2.0);
    for note in &f.notes[first..] {
        if f.timing.time_to_beat_position(note.target_time_seconds) > now_beat + visible_beats {
            break;
        }
        let lane = note.note_event.lane;
        let (x, w) = (l.lane_x(lane) + 2.0 * hair, l.lane_width(lane) - 4.0 * hair);
        let col = l.lane_color(lane);
        let head_y = y_at(note.target_time_seconds);
        match note.note_event.note_type {
            // A judged note is gone: it does not linger on the line while it scrolls past.
            NoteType::Tap if !note.is_judged => draw_note(c, sk, x, head_y - note_h, w, note_h, col),
            // A mine: a slim red bar with a dark core, gone once it has gone off.
            NoteType::Landmine if !note.is_judged => {
                let h = note_h * 0.7;
                let y = head_y - (note_h + h) / 2.0;
                draw_note(c, sk, x, y, w, h, theme::RED);
                c.fill_rect(Rect::new(x + w * 0.2, y + h * 0.35, w * 0.6, h * 0.3), theme::BG.with_alpha(200));
            }
            NoteType::LongNoteStart => {
                let tail_y = y_at(note.end_target_time_seconds);
                let held = note.is_holding;
                // Head judged but not held and not yet at its end: missed or let go
                // early. What is left of it keeps scrolling, dimmed, instead of
                // sticking to the judge line like a held note.
                let broken = note.is_judged && !held && f.audio_time < note.end_target_time_seconds - 0.15;
                if note.is_judged && !held && !broken {
                    continue;
                }
                let head_edge = if broken { head_y } else { head_y.min(judge_y) };
                let (top, bottom) = (tail_y.max(field.y), head_edge.min(judge_y));
                let body_col = if broken { col.with_alpha(40) } else { col.with_alpha(120) };
                if bottom > top {
                    c.nine(&sk.ln_body, Rect::from_ltrb(x + 3.0 * s, top, x + w - 3.0 * s, bottom), body_col);
                }
                if !broken {
                    draw_note(c, sk, x, head_edge - note_h, w, note_h, col);
                }
                draw_note(c, sk, x, tail_y - note_h, w, note_h, if broken { col.with_alpha(70) } else { col });
            }
            _ => {}
        }
    }
    c.pop_clip();

    // Lane cover (SUDDEN+).
    if l.lane_cover_ratio > 0.0 {
        let h = field.h * l.lane_cover_ratio.clamp(0.0, 0.85);
        let cover = Rect::new(field.x, field.y, field.w, h);
        c.fill_rect_vgradient(cover, theme::SURF1, theme::SURF2);
        c.fill_rect(Rect::new(field.x, cover.bottom() - 2.0 * hair, field.w, 2.0 * hair), theme::CYAN);
    }

    // Judge line + glow.
    let line_col = match f.score.current_combo {
        c if c >= 200 => theme::GOLD,
        c if c >= 50 => theme::CYAN,
        _ => theme::MAGENTA,
    };
    c.fill_rect(Rect::new(field.x, judge_y, field.w, 3.0 * hair), line_col);
    c.set_additive(true);
    c.sprite(sk.glow, Rect::new(field.x - 16.0 * s, judge_y - 12.0 * s, field.w + 32.0 * s, 27.0 * s), line_col.with_alpha(80));
    c.set_additive(false);

    // Key indicators below the judge line.
    let key_top = judge_y + 8.0 * s;
    let key_h = (field.bottom() - key_top - 8.0 * s).max(6.0 * s);
    c.fill_rect_vgradient(Rect::from_ltrb(field.x, judge_y + 3.0 * hair, field.right(), field.bottom()), theme::SURF1, theme::BG);
    // 8K side tracks: a wide strip under the six keys.
    let side_h = if l.eight_k_triggers() { (key_h * 0.34).max(6.0 * s) } else { 0.0 };
    for &lane in l.active_lanes() {
        let pressed = f.key_pressed[lane_index(lane)];
        let mut r = Rect::new(l.lane_x(lane) + 4.0 * s, key_top, l.lane_width(lane) - 8.0 * s, key_h);
        if side_h > 0.0 {
            if is_side_track(lane) {
                r.y = key_top + key_h - side_h;
                r.h = side_h;
            } else {
                r.h = key_h - side_h - 4.0 * s;
            }
        }
        let col = l.lane_color(lane);
        c.nine(&sk.panel_sm, r, if pressed { col } else { col.with_alpha(26) });
        if pressed {
            c.set_additive(true);
            c.sprite_centered(sk.glow, r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 2.0, r.h * 2.5, col.with_alpha(90));
            c.set_additive(false);
        }
    }

    if let Some(gap) = dp_gap {
        c.fill_rect(gap.inset_by(crate::canvas::Insets::new(2.0 * hair, 0.0, 2.0 * hair, 0.0)), theme::BG);
    }

    hit_bursts(c, sk, f, judge_y, s);
}

fn draw_note(c: &mut Canvas, sk: &Skin, x: f32, y: f32, w: f32, h: f32, col: ColorRgba) {
    let r = Rect::new(x, y, w, h);
    c.nine(&sk.note, r, col);
    c.set_additive(true);
    c.nine(&sk.note_gloss, r, theme::WHITE);
    c.set_additive(false);
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
        let fade = 1.0 - ease_in_cubic(p);
        let col = theme::judge_color(burst.grade);
        let (lx, lw) = (l.lane_x(burst.lane), l.lane_width(burst.lane));
        let cx = lx + lw / 2.0;
        let a = |k: f32| (255.0 * fade * k).clamp(0.0, 255.0) as u8;

        if burst.grade == JudgeGrade::PerfectGreat {
            let h = (150.0 * s).min(judge_y - l.playfield_y);
            c.fill_rect_vgradient(Rect::new(lx, judge_y - h, lw, h), col.with_alpha(0), col.with_alpha(a(0.22)));
        }
        let ring = lerp(30.0, 88.0, ease_out_cubic(p)) * s;
        c.sprite_centered(sk.ring, cx, judge_y, ring, ring, col.with_alpha(a(0.9)));
        let flare = lerp(130.0, 60.0, ease_out_cubic(p)) * s;
        c.sprite_centered(sk.flare, cx, judge_y, flare, flare, theme::WHITE.with_alpha(a(0.85)));
        let dist = ease_out_quad(p) * 46.0 * s;
        for k in 0..6 {
            let ang = std::f32::consts::PI * (0.12 + 0.152 * k as f32);
            let (dx, dy) = (ang.cos() * dist, -ang.sin() * dist);
            let size = (10.0 - 5.0 * p) * s;
            c.sprite_centered(sk.glow, cx + dx, judge_y + dy, size, size, col.with_alpha(a(1.0)));
        }
    }
    c.set_additive(false);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn gauge(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, r: Rect, danger: bool, s: f32) {
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
        let seg_col = if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) && i >= 40 && on {
            theme::MAGENTA
        } else {
            col
        };
        c.fill_rect(Rect::new(inner.x, y + 1.0, inner.w, seg_h - 1.0), if on { seg_col } else { seg_col.with_alpha(28) });
    }
    if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
        let y = inner.bottom() - inner.h * 0.8;
        c.fill_rect(Rect::new(r.x - 4.0 * s, y - s, r.w + 8.0 * s, 2.0 * s), theme::GOLD);
    }
    let pct = format!("{:.0}%", score.gauge.floor());
    t.draw_in(
        c,
        &pct,
        Rect::new(r.x - 12.0 * s, r.bottom() + 6.0 * s, r.w + 24.0 * s, 20.0 * s),
        Align::Center,
        &TextStyle::new(13.0 * s).bold().color(if danger { theme::RED } else { theme::TEXT }),
    );
}

fn combo_and_judge(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, field: Rect, s: f32) {
    let cx = field.x + field.w / 2.0;
    let anchor = f.layout.judge_line_y - 230.0 * s;
    let since = f.last_judge.map(|(_, at, _)| f.audio_time - at);

    if f.score.current_combo > 0 || since.is_some_and(|e| (0.0..JUDGE_SECONDS).contains(&e)) {
        // Soft dark pool so text stays legible over notes.
        c.sprite_centered(sk.glow, cx, anchor + 30.0 * s, field.w * 0.9, 190.0 * s, theme::BLACK.with_alpha(150));
    }

    if f.score.current_combo > 0 {
        let pulse = match since {
            Some(e) if (0.0..0.12).contains(&e) => 1.0 + 0.12 * (1.0 - ease_out_cubic((e / 0.12) as f32)),
            _ => 1.0,
        };
        let txt = thousands(f.score.current_combo);
        let st = TextStyle::new(52.0 * s).bold().color(theme::TEXT);
        let w = t.measure(c, &txt, &st) * pulse;
        t.draw_scaled(c, &txt, cx - w / 2.0, anchor, &st, pulse);
        let cap = caption(11.0, s).tracking(3.0 * s).color(theme::MUTED);
        let cw = t.measure(c, "COMBO", &cap);
        t.draw(c, "COMBO", cx - cw / 2.0, anchor + 18.0 * s, &cap);
    }

    let Some((grade, at, delta_ms)) = f.last_judge else { return };
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
    let y = anchor + 68.0 * s;
    let st = TextStyle::new(34.0 * s).bold().tracking(3.0 * s).color(col.with_alpha(a));
    let w = t.measure(c, label, &st) * pop;
    if grade == JudgeGrade::PerfectGreat {
        c.set_additive(true);
        c.sprite_centered(sk.glow, cx, y - 12.0 * s, w * 1.5, 80.0 * s, theme::CYAN.with_alpha((alpha * 110.0) as u8));
        c.set_additive(false);
    }
    let st = if grade == JudgeGrade::PerfectGreat {
        st.color(theme::WHITE.with_alpha(a))
    } else {
        st
    };
    t.draw_scaled(c, label, cx - w / 2.0, y, &st, pop);

    if grade != JudgeGrade::Miss && delta_ms.abs() >= 4.0 {
        let (txt, fs_col) = if delta_ms < 0.0 {
            (format!("FAST {:.0}ms", delta_ms.abs()), theme::FAST)
        } else {
            (format!("SLOW {:.0}ms", delta_ms), theme::SLOW)
        };
        let st = caption(12.0, s).color(fs_col.with_alpha(a));
        let w = t.measure(c, &txt, &st);
        t.draw(c, &txt, cx - w / 2.0, y + 22.0 * s, &st);
    }
}

fn hud(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, sides: &Sides, s: f32) {
    let min_w = 160.0 * s;
    let info = sides.info;
    if info.w >= min_w {
        let bottom = info_column(c, t, sk, f, info, sides.media.is_some(), s);
        if sides.media.is_none() {
            media_column(c, t, sk, f, Rect::from_ltrb(info.x, bottom, info.right(), info.bottom()), s);
        }
    }
    if let Some(media) = sides.media.filter(|m| m.w >= min_w) {
        media_column(c, t, sk, f, media, s);
    }
}

/// Song header and score panel from the top of `col`; returns where they end.
#[allow(clippy::too_many_arguments)]
fn info_column(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, col: Rect, score_at_bottom: bool, s: f32) -> f32 {
    let (x, w, right) = (col.x, col.w, col.right());
    let score = f.score;
    let header = &f.chart.header;
    let mut y = col.y;

    // Song header
    let (tier, tier_col) = theme::level_tier(header.play_level);
    let tier_txt = format!("{tier}  {}", header.play_level);
    t.draw(c, &tier_txt, x, y + 14.0 * s, &caption(11.0, s).color(tier_col));
    if let Some(badge) = f.badge {
        let st = caption(11.0, s).color(theme::ON_ACCENT);
        let bw = t.measure(c, badge, &st) + 20.0 * s;
        let chip = Rect::new(right - bw, y, bw, 20.0 * s);
        c.nine(&sk.panel_lg, chip, theme::CYAN);
        t.draw_in(c, badge, chip, Align::Center, &st);
    }
    let title_st = TextStyle::new(24.0 * s).bold().color(theme::TEXT);
    let title = t.fit(c, &header.title, w, &title_st).into_owned();
    t.draw(c, &title, x, y + 44.0 * s, &title_st);
    let sub = format!("{}  ·  BPM {}", header.artist, header.bpm.round());
    let sub_st = TextStyle::new(13.0 * s).color(theme::MUTED);
    let sub = t.fit(c, &sub, w, &sub_st).into_owned();
    t.draw(c, &sub, x, y + 66.0 * s, &sub_st);
    // Song progress
    let prog = Rect::new(x, y + 80.0 * s, w, 3.0 * s);
    c.fill_rect(prog, theme::LINE);
    let ratio = if f.song_length > 0.0 { (f.audio_time / f.song_length).clamp(0.0, 1.0) as f32 } else { 0.0 };
    c.fill_rect_hgradient(Rect::new(prog.x, prog.y, prog.w * ratio, prog.h), theme::CYAN, theme::MAGENTA);
    // Alone in its column (centered playfield) the score panel sits at the
    // bottom, level with the judge line; otherwise it follows the header.
    y = if score_at_bottom { col.bottom() - 196.0 * s } else { y + 104.0 * s };

    // Score panel
    let panel = Rect::new(x, y, w, 196.0 * s);
    c.halo(&sk.shadow, panel, theme::WHITE.with_alpha(170));
    c.nine(&sk.cut_panel, panel, theme::SURF1.with_alpha(240));
    c.nine(&sk.cut_outline, panel, theme::LINE);
    let inner = panel.inset(20.0 * s);
    t.draw(c, "EX SCORE", inner.x, inner.y + 10.0 * s, &caption(10.0, s));
    let ex = thousands(score.ex_score);
    let ex_st = TextStyle::new(44.0 * s).bold().color(theme::TEXT);
    let ex_w = t.draw(c, &ex, inner.x, inner.y + 56.0 * s, &ex_st);
    let max = format!("/ {}", thousands(score.max_ex_score()));
    t.draw(c, &max, inner.x + ex_w + 10.0 * s, inner.y + 56.0 * s, &TextStyle::new(13.0 * s).color(theme::MUTED2));

    // Rank, accuracy and pacemaker are all relative to the notes judged so
    // far (ScoreTracker::accuracy_rate divides by the whole chart, which
    // would show "F" for most of a perfect run).
    let judged = score.pgreat_count + score.great_count + score.good_count + score.bad_count + score.poor_count + score.miss_count;
    let acc = if judged > 0 {
        score.ex_score as f64 / (judged as f64 * 2.0) * 100.0
    } else {
        100.0
    };
    let (rank, rank_col) = theme::rank(acc);
    let rank_box = Rect::new(inner.right() - 110.0 * s, inner.y, 110.0 * s, 60.0 * s);
    t.draw_in(c, rank, rank_box, Align::Right, &TextStyle::new(38.0 * s).bold().color(rank_col));
    let target = (judged as f64 * 2.0 * 8.0 / 9.0).round() as i64;
    let diff = score.ex_score as i64 - target;
    let pace = format!("AAA {}{}   {:.2}%", if diff >= 0 { "+" } else { "" }, diff, acc);
    t.draw(c, &pace, inner.x, inner.y + 80.0 * s, &TextStyle::new(13.0 * s).bold().color(if diff >= 0 { theme::CYAN } else { theme::MAGENTA }));

    // Judge distribution bar + legend
    let bar = Rect::new(inner.x, inner.y + 94.0 * s, inner.w, 6.0 * s);
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
                c.fill_rect(Rect::new(bx, bar.y, (sw - s).max(s), bar.h), theme::judge_color(g));
            }
            bx += sw;
        }
    }
    let cols = if inner.w > 360.0 * s { 3 } else { 2 };
    let col_w = inner.w / cols as f32;
    for (i, (g, n)) in counts.iter().enumerate() {
        let lx = inner.x + (i % cols) as f32 * col_w;
        let ly = bar.bottom() + 24.0 * s + (i / cols) as f32 * 22.0 * s;
        let dot = 12.0 * s;
        c.sprite(sk.icons.dot, Rect::new(lx - 3.0 * s, ly - 10.0 * s, dot, dot), theme::judge_color(*g));
        t.draw(c, theme::judge_label(*g), lx + 12.0 * s, ly, &caption(10.0, s).color(theme::MUTED));
        t.draw_in(c, &thousands(*n), Rect::new(lx, ly - 13.0 * s, col_w - 16.0 * s, 16.0 * s), Align::Right, &TextStyle::new(13.0 * s).bold().color(theme::TEXT));
    }
    panel.bottom() + 20.0 * s
}

/// BGA + visualizer filling `col`, with the key hint along its bottom.
fn media_column(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, col: Rect, s: f32) {
    let (x, w) = (col.x, col.w);
    let mut y = col.y;
    let hint_h = 26.0 * s;
    let vis_h = 40.0 * s;
    let avail_h = (col.bottom() - hint_h - vis_h - 12.0 * s - y).max(0.0);
    let bga_h = (w * 9.0 / 16.0).min(avail_h);
    let bga_w = bga_h * 16.0 / 9.0;
    if bga_h > 40.0 * s {
        let frame = Rect::new(x + (w - bga_w) / 2.0, y, bga_w, bga_h);
        c.nine(&sk.panel, frame.inset(-s), theme::LINE);
        c.fill_rect(frame, theme::BG);
        match f.bga {
            Some(tex) => c.image(tex.id, frame, cover_uv(tex, frame), theme::WHITE),
            None => {
                t.draw_in(c, "NO BGA", frame, Align::Center, &caption(11.0, s).color(theme::MUTED2));
            }
        }
        if let Some(tex) = f.layer {
            c.image(tex.id, frame, cover_uv(tex, frame), theme::WHITE);
        }
        y = frame.bottom() + 12.0 * s;

        // Spectrum
        let n = f.visual_levels.len() as f32;
        let gap = 3.0 * s;
        let bw = (bga_w - gap * (n - 1.0)) / n;
        c.set_additive(true);
        for (i, &lvl) in f.visual_levels.iter().enumerate() {
            let lvl = lvl.clamp(0.0, 1.0);
            let h = (vis_h * lvl).max(2.0 * s);
            let bx = frame.x + i as f32 * (bw + gap);
            let col = if lvl > 0.8 { theme::MAGENTA } else { theme::CYAN };
            c.fill_rect_vgradient(Rect::new(bx, y + vis_h - h, bw, h), col.with_alpha(200), col.with_alpha(30));
        }
        c.set_additive(false);
    }

    let hint_st = TextStyle::new(11.0 * s).color(theme::MUTED2);
    // Too long for a narrow column: break between the hint's groups (they
    // are four spaces apart) rather than inside one.
    let group_break = (t.measure(c, f.hint, &hint_st) > w)
        .then(|| f.hint.match_indices("    ").map(|(i, _)| i).filter(|&i| t.measure(c, &f.hint[..i], &hint_st) <= w).last())
        .flatten();
    let lines = match group_break {
        Some(i) => (f.hint[..i].to_string(), Some(t.fit(c, f.hint[i..].trim_start(), w, &hint_st).into_owned())),
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

fn pause_menu(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &PlayFrame, selected: usize, s: f32) {
    let vp = f.viewport;
    c.fill_rect(Rect::new(vp.x, vp.y, vp.width, vp.height), theme::BLACK.with_alpha(190));
    let (w, h) = (440.0 * s, 360.0 * s);
    let panel = Rect::new(vp.x + (vp.width - w) / 2.0, vp.y + (vp.height - h) / 2.0, w, h);
    c.halo(&sk.shadow, panel, theme::WHITE);
    c.nine(&sk.panel_lg, panel, theme::SURF1);
    c.fill_rect_hgradient(Rect::new(panel.x + 16.0 * s, panel.y, panel.w - 32.0 * s, 2.0 * s), theme::CYAN, theme::MAGENTA);
    let inner = panel.inset(28.0 * s);

    t.draw(c, "PAUSED", inner.x, inner.y + 26.0 * s, &TextStyle::new(28.0 * s).bold().tracking(4.0 * s).color(theme::TEXT));
    let title_st = TextStyle::new(14.0 * s).bold().color(theme::TEXT);
    let title = t.fit(c, &f.chart.header.title, inner.w, &title_st).into_owned();
    t.draw(c, &title, inner.x, inner.y + 56.0 * s, &title_st);
    let artist_st = TextStyle::new(12.0 * s).color(theme::MUTED);
    let artist = t.fit(c, &f.chart.header.artist, inner.w, &artist_st).into_owned();
    t.draw(c, &artist, inner.x, inner.y + 74.0 * s, &artist_st);

    let bar = Rect::new(inner.x, inner.y + 92.0 * s, inner.w, 4.0 * s);
    let ratio = if f.song_length > 0.0 { (f.audio_time / f.song_length).clamp(0.0, 1.0) as f32 } else { 0.0 };
    c.nine(&sk.panel_sm, bar, theme::LINE);
    c.fill_rect_hgradient(Rect::new(bar.x, bar.y, bar.w * ratio, bar.h), theme::CYAN, theme::MAGENTA);
    let time = format!("{} / {}", theme::clock(f.audio_time), theme::clock(f.song_length));
    t.draw_in(c, &time, Rect::new(inner.x, bar.bottom() + 4.0 * s, inner.w, 18.0 * s), Align::Right, &TextStyle::new(11.0 * s).color(theme::MUTED));

    let options = ["RESUME", "RESTART", "QUIT TO SONG SELECT"];
    let mut y = bar.bottom() + 36.0 * s;
    for (i, label) in options.iter().enumerate() {
        let row = Rect::new(inner.x, y, inner.w, 40.0 * s);
        let on = i == selected;
        if on {
            c.nine(&sk.panel, row, theme::CYAN.with_alpha(30));
            c.nine(&sk.panel_outline, row, theme::CYAN.with_alpha(200));
            c.sprite(sk.icons.chevron_right, Rect::new(row.x + 8.0 * s, row.y + 10.0 * s, 20.0 * s, 20.0 * s), theme::CYAN);
        }
        t.draw_in(
            c,
            label,
            Rect::new(row.x + 36.0 * s, row.y, row.w - 44.0 * s, row.h),
            Align::Left,
            &TextStyle::new(14.0 * s).bold().tracking(2.0 * s).color(if on { theme::TEXT } else { theme::MUTED }),
        );
        y += 46.0 * s;
    }
    t.draw_in(
        c,
        "↑↓ SELECT   ENTER CONFIRM   R RESTART",
        Rect::new(inner.x, panel.bottom() - 34.0 * s, inner.w, 20.0 * s),
        Align::Center,
        &caption(10.0, s),
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
        Fixture { vp, layout, chart, timing, judge, score }
    }

    #[test]
    fn whole_frame_is_one_batch_without_bga() {
        let fx = fixture();
        let mut keys = [false; LANE_COUNT];
        keys[lane_index(Lane::Key3)] = true;
        let bursts = [HitBurst { lane: Lane::Key3, spawn_time: 1.0, grade: JudgeGrade::PerfectGreat }];
        let mut ui = Ui::new(fx.vp.scale);
        for pause in [None, Some(1)] {
            ui.begin(1280, 720, fx.vp.scale);
            draw_gameplay(
                &mut ui,
                &PlayFrame {
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
                    layer: None,
                    track_bga_opacity: 0.3,
                    key_pressed: &keys,
                    hit_bursts: &bursts,
                    last_judge: Some((JudgeGrade::PerfectGreat, 1.0, -6.0)),
                    hint: "ESC pause",
                    badge: Some("AUTO PLAY"),
                    pause,
                },
            );
            assert_eq!(ui.canvas.debug_batches().len(), 1, "pause={pause:?}");
        }
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
        let tex = SizedTexture { id: TextureId(1), width: 512, height: 512 };
        let uv = cover_uv(tex, Rect::new(0.0, 0.0, 160.0, 90.0));
        assert!((uv[0] - 0.0).abs() < 1e-6 && (uv[2] - 1.0).abs() < 1e-6);
        assert!((uv[3] - uv[1] - 0.5625).abs() < 1e-4);
    }
}
