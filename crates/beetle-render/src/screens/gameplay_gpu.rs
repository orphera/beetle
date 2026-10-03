use crate::backend::{BlendMode, FontAtlas, GpuBackend, SpriteBatcher, TextureId};
use crate::renderer::{lane_index, HitBurst, Viewport, LANE_COUNT};
use crate::skin::{ColorRgba, SkinConfig};
use beetle_core::{BmsChart, GaugeType, JudgeGrade, NoteType, PlayNote, ScoreTracker, TimingModel};

/// Draws a note body with the same cheap 3-band "glossy pill" bevel as the
/// software path's `draw_glossy_note_rect` (screens/gameplay.rs) — bright
/// top highlight, base color, dark bottom edge — using only flat rects so
/// it stays inside the untextured-rect batching pass (no gradient/path-fill
/// support on the GPU SpriteBatcher). See
/// docs/plans/2026-10-03-pulse-redesign.md.
#[allow(clippy::too_many_arguments)]
fn draw_glossy_note_rect_gpu(
    backend: &mut dyn GpuBackend,
    batcher: &mut SpriteBatcher,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: ColorRgba,
    s: f32,
) {
    batcher.draw_rect(backend, x, y, w, h, color.to_f32_array());
    let highlight_h = (h * 0.4).max(1.0);
    batcher.draw_rect(
        backend,
        x,
        y,
        w,
        highlight_h,
        color.lighten(0.35).to_f32_array(),
    );
    let shadow_h = (1.0 * s).max(1.0).min(h);
    batcher.draw_rect(
        backend,
        x,
        y + h - shadow_h,
        w,
        shadow_h,
        color.darken(0.4).to_f32_array(),
    );
}

/// High-performance GPU batched rendering for gameplay screen.
///
/// Dispatches zero-CPU-allocation indexed 2D quad batches directly to the underlying `GpuBackend`,
/// eliminating per-pixel software rasterization and full-frame VRAM texture uploads.
#[allow(clippy::too_many_arguments)]
pub fn render_gameplay_gpu(
    backend: &mut dyn GpuBackend,
    batcher: &mut SpriteBatcher,
    viewport: &Viewport,
    skin: &SkinConfig,
    font_atlas: &FontAtlas,
    chart: &BmsChart,
    notes: &[PlayNote],
    audio_time_seconds: f64,
    score: &ScoreTracker,
    visual_levels: &[f32; 16],
    bga_texture: Option<TextureId>,
    layer_texture: Option<TextureId>,
    track_bga_opacity: f32,
    timing: &TimingModel,
    key_pressed: &[bool; LANE_COUNT],
    hit_bursts: &[HitBurst],
    last_judge: Option<(JudgeGrade, f64, f64)>,
) {
    batcher.begin();

    let s = viewport.scale;

    // PASS 1: Base Playfield & Alpha Primitives (Texture: None, Blend: Alpha)
    // 1. Viewport background (margins are automatically handled by swapchain clear)
    batcher.draw_rect(
        backend,
        viewport.x,
        viewport.y,
        viewport.width,
        viewport.height,
        skin.bg_color.to_f32_array(),
    );

    // 2. Playfield main background box
    batcher.draw_rect(
        backend,
        skin.playfield_x,
        skin.playfield_y,
        skin.playfield_width,
        skin.playfield_height,
        skin.playfield_bg_color.to_f32_array(),
    );

    // 3. Track BGA underlay
    if track_bga_opacity > 0.0 {
        if let Some(tex) = bga_texture {
            batcher.draw_sub_sprite(
                backend,
                tex,
                skin.playfield_x,
                skin.playfield_y,
                skin.playfield_width,
                skin.playfield_height,
                0.0,
                0.0,
                1.0,
                1.0,
                [1.0, 1.0, 1.0, track_bga_opacity],
                BlendMode::Alpha,
            );
        }
        if let Some(tex) = layer_texture {
            batcher.draw_sub_sprite(
                backend,
                tex,
                skin.playfield_x,
                skin.playfield_y,
                skin.playfield_width,
                skin.playfield_height,
                0.0,
                0.0,
                1.0,
                1.0,
                [1.0, 1.0, 1.0, track_bga_opacity],
                BlendMode::Alpha,
            );
        }
    }

    // 4. Lane vertical separator lines
    let line_w = (1.0 * s).max(1.0);
    let line_color = if score.current_combo >= 100 {
        ColorRgba::new(50, 100, 180, 220)
    } else {
        skin.lane_line_color
    };
    let line_col_f32 = line_color.to_f32_array();

    for &lane in skin.active_lanes() {
        let x = skin.lane_x(lane);
        batcher.draw_rect(
            backend,
            x,
            skin.playfield_y,
            line_w,
            skin.playfield_height,
            line_col_f32,
        );
    }
    let right_x = skin.playfield_x + skin.playfield_width;
    batcher.draw_rect(
        backend,
        right_x,
        skin.playfield_y,
        line_w,
        skin.playfield_height,
        line_col_f32,
    );

    // Danger pulsing border around playfield
    let is_danger = (score.gauge < 30.0
        && matches!(score.gauge_type, GaugeType::Hard | GaugeType::Groove))
        || (score.gauge_type == GaugeType::Hazard && score.gauge < 100.0);
    let danger_blink = is_danger && ((audio_time_seconds * 6.0).sin() > 0.0);
    if danger_blink {
        let px = skin.playfield_x;
        let py = skin.playfield_y;
        let pw = skin.playfield_width;
        let ph = skin.playfield_height;
        let b_thick = (2.0 * s).max(2.0);
        let danger_f32 = ColorRgba::new(255, 40, 40, 220).to_f32_array();
        batcher.draw_rect(backend, px, py, pw, b_thick, danger_f32);
        batcher.draw_rect(backend, px, py + ph - b_thick, pw, b_thick, danger_f32);
        batcher.draw_rect(backend, px, py, b_thick, ph, danger_f32);
        batcher.draw_rect(backend, px + pw - b_thick, py, b_thick, ph, danger_f32);
    }

    // 5. Measure Bar Lines
    let effective_speed = skin.hi_speed * s;
    let judge_y = skin.judge_line_y;
    let top_y = skin.playfield_y;
    let px = skin.playfield_x;
    let pw = skin.playfield_width;
    let bar_line_h = (1.0 * s).max(1.0);
    let bar_line_col = ColorRgba::new(200, 210, 225, 90).to_f32_array();

    let visible_duration = (judge_y - top_y + 50.0 * s) as f64 / effective_speed.max(1.0) as f64;
    let max_time = audio_time_seconds + visible_duration;
    let max_measure = chart.max_measure + 2;
    let start_measure = {
        let (m, _) = timing.time_to_beat(audio_time_seconds);
        m.saturating_sub(2)
    };

    for measure in start_measure..=max_measure {
        let measure_time = timing.beat_to_time_seconds(measure, 0.0);
        let delta_t = measure_time - audio_time_seconds;
        let bar_y = judge_y - (delta_t as f32 * effective_speed);
        if bar_y > judge_y + 20.0 * s {
            continue;
        }
        if measure_time > max_time && bar_y < top_y - 20.0 * s {
            break;
        }
        if bar_y >= top_y && bar_y <= judge_y {
            batcher.draw_rect(
                backend,
                px,
                bar_y - bar_line_h * 0.5,
                pw,
                bar_line_h,
                bar_line_col,
            );
        }
    }

    // 6. Notes & Long Notes
    let note_h = skin.note_height;
    let note_vis_dur = (judge_y - top_y + 100.0 * s) as f64 / effective_speed.max(1.0) as f64;
    let min_note_time = audio_time_seconds - 2.0;
    let max_note_time = audio_time_seconds + note_vis_dur;

    let start_idx = notes.partition_point(|n| n.end_target_time_seconds < min_note_time);

    for note in &notes[start_idx..] {
        if note.target_time_seconds > max_note_time {
            break;
        }

        let delta_t = note.target_time_seconds - audio_time_seconds;
        let note_y = judge_y - (delta_t as f32 * effective_speed);
        let lane = note.note_event.lane;
        let lane_x = skin.lane_x(lane) + 1.0;
        let lane_w = skin.lane_width(lane) - 2.0;
        let note_col = skin.lane_color(lane);

        match note.note_event.note_type {
            NoteType::Tap => {
                if note_y + note_h >= top_y && note_y - note_h <= judge_y + 40.0 * s {
                    draw_glossy_note_rect_gpu(
                        backend,
                        batcher,
                        lane_x,
                        note_y - note_h,
                        lane_w,
                        note_h,
                        note_col,
                        s,
                    );
                }
            }
            NoteType::LongNoteStart => {
                let end_delta = note.end_target_time_seconds - audio_time_seconds;
                let end_y = judge_y - (end_delta as f32 * effective_speed);
                let body_top = end_y.max(top_y);
                let body_bottom = note_y.min(judge_y);

                if body_bottom > body_top {
                    let body_color = note_col.with_alpha(130).to_f32_array();
                    batcher.draw_rect(
                        backend,
                        lane_x + 3.0 * s,
                        body_top,
                        lane_w - 6.0 * s,
                        body_bottom - body_top,
                        body_color,
                    );
                    let core_w = (2.0 * s).max(1.0);
                    let core_x = lane_x + lane_w / 2.0 - core_w / 2.0;
                    batcher.draw_rect(
                        backend,
                        core_x,
                        body_top,
                        core_w,
                        body_bottom - body_top,
                        note_col.with_alpha(210).to_f32_array(),
                    );
                }
                if note_y + note_h >= top_y && note_y <= judge_y + 40.0 * s {
                    draw_glossy_note_rect_gpu(
                        backend,
                        batcher,
                        lane_x,
                        note_y - note_h,
                        lane_w,
                        note_h,
                        note_col,
                        s,
                    );
                }
                if end_y + note_h >= top_y && end_y <= judge_y + 40.0 * s {
                    draw_glossy_note_rect_gpu(
                        backend,
                        batcher,
                        lane_x,
                        end_y - note_h,
                        lane_w,
                        note_h,
                        note_col,
                        s,
                    );
                }
            }
            _ => (),
        }
    }

    // 7. Lane Cover
    // Dark scrim behind the combo+judge cluster so the text doesn't sit
    // directly on top of falling notes / LN bodies with no separation.
    // Placed in the untextured-rect pass (not the text pass) to avoid a
    // texture switch; the actual text renders in Pass 4 below, on top of
    // this scrim like everything else. See
    // docs/plans/2026-10-03-pulse-redesign.md.
    if score.current_combo > 0 || last_judge.is_some() {
        let scrim_center_x = skin.playfield_x + (skin.playfield_width / 2.0);
        let scrim_judge_center_y = skin.judge_line_y - 120.0 * s;
        let scrim_w = 150.0 * s;
        let scrim_h = 130.0 * s;
        batcher.draw_rect(
            backend,
            scrim_center_x - scrim_w / 2.0,
            scrim_judge_center_y - 46.0 * s,
            scrim_w,
            scrim_h,
            [0.0, 0.0, 0.0, 120.0 / 255.0],
        );
    }

    if skin.lane_cover_ratio > 0.0 {
        let ratio = skin.lane_cover_ratio.clamp(0.0, 0.85);
        let cover_h = skin.playfield_height * ratio;
        let cover_col = ColorRgba::new(12, 12, 18, 255).to_f32_array();
        let border_col = ColorRgba::new(80, 140, 255, 255).to_f32_array();
        batcher.draw_rect(
            backend,
            skin.playfield_x,
            skin.playfield_y,
            skin.playfield_width,
            cover_h,
            cover_col,
        );
        batcher.draw_rect(
            backend,
            skin.playfield_x,
            skin.playfield_y + cover_h - 2.0 * s,
            skin.playfield_width,
            2.0 * s,
            border_col,
        );
    }

    // 8. Core Judge Line
    let judge_line_h = (2.0 * s).max(2.0);
    batcher.draw_rect(
        backend,
        px,
        judge_y,
        pw,
        judge_line_h,
        skin.judge_line_color.to_f32_array(),
    );

    // 9. Gauge Bar Box & Fill
    let gauge_x = skin.playfield_x + skin.playfield_width + 16.0 * s;
    let gauge_y = skin.playfield_y;
    let gauge_w = 22.0 * s;
    let gauge_h = skin.playfield_height;

    batcher.draw_rect(
        backend,
        gauge_x,
        gauge_y,
        gauge_w,
        gauge_h,
        ColorRgba::new(20, 20, 28, 255).to_f32_array(),
    );

    let fill_ratio = (score.gauge / 100.0).clamp(0.0, 1.0) as f32;
    let fill_h = gauge_h * fill_ratio;
    let fill_y = gauge_y + gauge_h - fill_h;

    let fill_color = match score.gauge_type {
        GaugeType::Easy => {
            if score.gauge >= 80.0 {
                ColorRgba::new(80, 255, 160, 255)
            } else {
                ColorRgba::new(60, 200, 240, 255)
            }
        }
        GaugeType::Groove => {
            if score.gauge >= 80.0 {
                ColorRgba::new(60, 240, 100, 255)
            } else if danger_blink {
                ColorRgba::new(255, 70, 70, 255)
            } else {
                ColorRgba::new(60, 140, 255, 255)
            }
        }
        GaugeType::Hard => {
            if danger_blink {
                ColorRgba::new(255, 40, 40, 255)
            } else if score.gauge < 30.0 {
                ColorRgba::new(255, 70, 70, 255)
            } else {
                ColorRgba::new(255, 180, 40, 255)
            }
        }
        GaugeType::Hazard => {
            if danger_blink {
                ColorRgba::new(255, 50, 50, 255)
            } else {
                ColorRgba::new(240, 40, 80, 255)
            }
        }
    };
    batcher.draw_rect(
        backend,
        gauge_x,
        fill_y,
        gauge_w,
        fill_h,
        fill_color.to_f32_array(),
    );

    // Bright core stripe down the middle of the fill (same "energy tube"
    // language as the LN body) so the bar reads as a lit meter instead of
    // a flat color block.
    if fill_h > 2.0 * s {
        let core_w = (gauge_w * 0.4).max(2.0);
        let core_x = gauge_x + (gauge_w - core_w) / 2.0;
        batcher.draw_rect(
            backend,
            core_x,
            fill_y,
            core_w,
            fill_h,
            fill_color.lighten(0.4).to_f32_array(),
        );
    }

    // Decile tick marks so the bar reads as a calibrated meter.
    let gauge_tick_col = ColorRgba::new(0, 0, 0, 90).to_f32_array();
    let gauge_tick_h = (1.0 * s).max(1.0);
    for i in 1..10 {
        let ty = gauge_y + gauge_h * (i as f32 / 10.0);
        batcher.draw_rect(backend, gauge_x, ty, gauge_w, gauge_tick_h, gauge_tick_col);
    }

    // Bright "water line" at the top edge of the fill.
    if fill_h > 0.0 {
        batcher.draw_rect(
            backend,
            gauge_x,
            fill_y,
            gauge_w,
            (2.0 * s).max(1.0),
            fill_color.lighten(0.65).to_f32_array(),
        );
    }

    let b_border_col = if danger_blink {
        ColorRgba::new(255, 60, 60, 255).to_f32_array()
    } else {
        ColorRgba::new(80, 80, 100, 255).to_f32_array()
    };
    let b_line = (1.0 * s).max(1.0);
    batcher.draw_rect(backend, gauge_x, gauge_y, gauge_w, b_line, b_border_col);
    batcher.draw_rect(
        backend,
        gauge_x,
        gauge_y + gauge_h - b_line,
        gauge_w,
        b_line,
        b_border_col,
    );
    batcher.draw_rect(backend, gauge_x, gauge_y, b_line, gauge_h, b_border_col);
    batcher.draw_rect(
        backend,
        gauge_x + gauge_w - b_line,
        gauge_y,
        b_line,
        gauge_h,
        b_border_col,
    );

    if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
        let line_y = gauge_y + gauge_h * 0.2;
        batcher.draw_rect(
            backend,
            gauge_x - 3.0 * s,
            line_y,
            gauge_w + 6.0 * s,
            2.0 * s,
            ColorRgba::new(255, 220, 50, 255).to_f32_array(),
        );
    }

    // 10. BGA Frame container
    let side_x = skin.playfield_x + skin.playfield_width + 48.0 * s;
    let bga_y = skin.playfield_y + 240.0 * s;
    let max_w = (viewport.x + viewport.width - side_x - 24.0 * s).max(100.0);
    let bga_w = max_w;
    let bga_h = (bga_w * 9.0 / 16.0).round();

    batcher.draw_rect(
        backend,
        side_x - 2.0 * s,
        bga_y - 2.0 * s,
        bga_w + 4.0 * s,
        bga_h + 4.0 * s,
        ColorRgba::new(50, 60, 80, 255).to_f32_array(),
    );
    batcher.draw_rect(
        backend,
        side_x,
        bga_y,
        bga_w,
        bga_h,
        ColorRgba::new(8, 8, 12, 255).to_f32_array(),
    );

    // PULSE judge breakdown bar: solid-color proportional segments, emitted
    // here (still inside the untextured-rect pass) so it doesn't reopen a
    // texture switch once the font-atlas text pass starts below. See
    // docs/plans/2026-10-03-pulse-redesign.md for why GPU-path PULSE chrome
    // stays rect-only (SpriteBatcher has no path-fill/gradient support).
    {
        let hud_x = skin.playfield_x + skin.playfield_width + 48.0 * s;
        let panel_w = (viewport.x + viewport.width - hud_x - 24.0 * s).max(100.0);
        let bar_y = skin.playfield_y + 170.0 * s;
        let bar_h = 8.0 * s;
        let counts = [
            score.pgreat_count,
            score.great_count,
            score.good_count,
            score.bad_count,
            score.poor_count,
            score.miss_count,
        ];
        let colors = [
            ColorRgba::new(255, 230, 80, 255),
            ColorRgba::new(255, 170, 50, 255),
            ColorRgba::new(60, 220, 120, 255),
            ColorRgba::new(200, 90, 240, 255),
            ColorRgba::new(240, 50, 50, 255),
            ColorRgba::new(150, 155, 175, 255),
        ];
        let total: u32 = counts.iter().sum::<u32>().max(1);
        let mut bar_x = hud_x;
        for i in 0..6 {
            let seg_w = panel_w * (counts[i] as f32 / total as f32);
            if seg_w > 0.0 {
                batcher.draw_rect(
                    backend,
                    bar_x,
                    bar_y,
                    seg_w,
                    bar_h,
                    colors[i].to_f32_array(),
                );
            }
            bar_x += seg_w;
        }
    }

    // PASS 2: BGA Sprites (Texture: BGA Texture, Blend: Alpha)
    if let Some(tex) = bga_texture {
        batcher.draw_sprite(
            backend,
            tex,
            side_x,
            bga_y,
            bga_w,
            bga_h,
            [1.0, 1.0, 1.0, 1.0],
        );
    }
    if let Some(tex) = layer_texture {
        batcher.draw_sprite(
            backend,
            tex,
            side_x,
            bga_y,
            bga_w,
            bga_h,
            [1.0, 1.0, 1.0, 1.0],
        );
    }

    // PASS 3: Additive Blended Beams & Glows (Texture: None, Blend: Additive)
    // 1. Key Beams
    for &lane in skin.active_lanes() {
        let idx = lane_index(lane);
        if key_pressed[idx] {
            let x = skin.lane_x(lane) + 1.0;
            let w = skin.lane_width(lane) - 1.0;
            let beam_h = skin.judge_line_y - skin.playfield_y;
            let beam_color = skin.key_beam_color(lane).to_f32_array();
            batcher.draw_rect_with_blend(
                backend,
                x,
                skin.playfield_y,
                w,
                beam_h,
                beam_color,
                BlendMode::Additive,
            );
        }
    }

    // 2. Judge Line Glow
    let glow_color = if score.current_combo >= 200 {
        ColorRgba::new(255, 215, 60, 90).to_f32_array()
    } else if score.current_combo >= 50 {
        ColorRgba::new(60, 200, 255, 80).to_f32_array()
    } else {
        ColorRgba::new(255, 70, 70, 70).to_f32_array()
    };
    batcher.draw_rect_with_blend(
        backend,
        px,
        judge_y - 3.0 * s,
        pw,
        7.0 * s,
        glow_color,
        BlendMode::Additive,
    );

    // 3. Hit Bursts
    let burst_duration = 0.22;
    for burst in hit_bursts {
        let elapsed = (audio_time_seconds - burst.spawn_time).max(0.0);
        if elapsed >= burst_duration {
            continue;
        }
        let progress = (elapsed / burst_duration) as f32;
        let eased_shrink = crate::motion::ease_out_cubic(progress);
        let eased_fade = crate::motion::ease_in_cubic(progress);
        let alpha = (1.0 - eased_fade).clamp(0.0, 1.0);

        let lx = skin.lane_x(burst.lane) + skin.lane_width(burst.lane) / 2.0;
        let (r, g, b) = match burst.grade {
            JudgeGrade::PerfectGreat => (1.0, 0.9, 0.3),
            JudgeGrade::Great => (1.0, 0.65, 0.2),
            JudgeGrade::Good => (0.24, 0.86, 0.47),
            _ => (0.63, 0.63, 0.7),
        };

        if burst.grade == JudgeGrade::PerfectGreat {
            // Confined to a strip near the judge line instead of the full
            // lane height — a full-height additive flash retriggering on
            // every PGREAT (the skilled-play state) was accumulating into a
            // persistent olive/muddy wash instead of reading as a flash.
            // See docs/plans/2026-10-03-pulse-redesign.md.
            let flash_alpha = alpha * 0.18;
            let lane_x = skin.lane_x(burst.lane);
            let lane_w = skin.lane_width(burst.lane);
            let flash_h = (140.0 * s).min(judge_y - skin.playfield_y);
            batcher.draw_rect_with_blend(
                backend,
                lane_x,
                judge_y - flash_h,
                lane_w,
                flash_h,
                [r, g, b, flash_alpha],
                BlendMode::Additive,
            );
        }

        let spark_size = (18.0 * (1.0 - eased_shrink * 0.5) * s).max(4.0);
        let spark_col = [r, g, b, alpha];
        let dist = crate::motion::ease_out_quad(progress) * 40.0 * s;
        let offsets = [
            (0.0, -dist),
            (0.0, dist),
            (-dist, 0.0),
            (dist, 0.0),
            (-dist * 0.7, -dist * 0.7),
            (dist * 0.7, -dist * 0.7),
            (-dist * 0.7, dist * 0.7),
            (dist * 0.7, dist * 0.7),
        ];

        for (dx, dy) in offsets {
            batcher.draw_rect_with_blend(
                backend,
                lx + dx - spark_size / 2.0,
                judge_y + dy - spark_size / 2.0,
                spark_size,
                spark_size,
                spark_col,
                BlendMode::Additive,
            );
        }
    }

    // 4. Visualizer Bars
    let vis_y = bga_y + bga_h + 16.0 * s;
    let vis_h = 32.0 * s;
    let bar_spacing = 3.0 * s;
    let total_spacing = bar_spacing * 15.0;
    let single_bar_w = ((bga_w - total_spacing) / 16.0).max(2.0);

    for (i, &lvl) in visual_levels.iter().enumerate() {
        let level = lvl.clamp(0.0, 1.0);
        let bar_h = (vis_h * level).max(2.0);
        let bx = side_x + (i as f32 * (single_bar_w + bar_spacing));
        let by = vis_y + vis_h - bar_h;

        let col = if level > 0.8 {
            ColorRgba::new(255, 90, 90, 220)
        } else if level > 0.4 {
            ColorRgba::new(255, 210, 60, 200)
        } else {
            ColorRgba::new(60, 180, 255, 180)
        };
        // Dim body + bright peak cap (VU-meter style) instead of a single
        // flat additive block, matching the software path's treatment.
        batcher.draw_rect_with_blend(
            backend,
            bx,
            by,
            single_bar_w,
            bar_h,
            col.darken(0.3).to_f32_array(),
            BlendMode::Additive,
        );
        let cap_h = 2.0_f32.max(1.0).min(bar_h);
        batcher.draw_rect_with_blend(
            backend,
            bx,
            by,
            single_bar_w,
            cap_h,
            col.lighten(0.3).to_f32_array(),
            BlendMode::Additive,
        );
    }

    // PASS 4: Font Atlas Batched Text (Texture: FontAtlas, Blend: Alpha)
    // 1. GAUGE label + percentage text below bar
    font_atlas.draw_ascii_text_centered(
        batcher,
        backend,
        "GAUGE",
        gauge_x + gauge_w / 2.0,
        gauge_y - 16.0 * s,
        (s * 0.72).round().max(1.0),
        ColorRgba::new(120, 128, 150, 255),
    );
    let gauge_str = format!("{:.1}%", score.gauge);
    let gauge_txt_col = if danger_blink {
        ColorRgba::new(255, 80, 80, 255)
    } else {
        ColorRgba::new(220, 220, 240, 255)
    };
    font_atlas.draw_ascii_text(
        batcher,
        backend,
        &gauge_str,
        gauge_x - 4.0 * s,
        gauge_y + gauge_h + 8.0 * s,
        (s * 0.9).round().max(1.0),
        gauge_txt_col,
    );

    // 2. Combo & Judge Popup
    let center_x = skin.playfield_x + (skin.playfield_width / 2.0);
    let judge_center_y = skin.judge_line_y - 120.0 * s;
    let gap = (6.0 * s).max(3.0);

    // Chains the judge-text anchor below the combo cluster using real
    // glyph heights instead of two independently-guessed offsets — see the
    // matching fix/comment in screens/gameplay.rs for why that overlapped
    // the COMBO label at some viewport scales.
    let mut judge_anchor_y = judge_center_y + 8.0 * s;
    if score.current_combo > 0 {
        let combo_str = format!("{}", score.current_combo);
        let pulse_offset = if let Some((_, judge_time, _)) = last_judge {
            let elapsed = audio_time_seconds - judge_time;
            if elapsed >= 0.0 && elapsed < 0.12 {
                let t = (elapsed / 0.12) as f32;
                (1.0 - crate::motion::ease_out_cubic(t)) * 8.0 * s
            } else {
                0.0
            }
        } else {
            0.0
        };

        let combo_scale = (3.0 * s).round().max(2.0);
        let combo_y = judge_center_y - 34.0 * s - pulse_offset;
        font_atlas.draw_bold_text_centered(
            batcher,
            backend,
            &combo_str,
            center_x,
            combo_y,
            combo_scale,
            ColorRgba::new(255, 255, 255, 255),
        );

        // Bold digits are 12 rows tall, vs. 7 for regular ASCII.
        let combo_glyph_h = 12.0 * combo_scale;
        let combo_label_y = combo_y + combo_glyph_h + gap;
        font_atlas.draw_ascii_text_centered(
            batcher,
            backend,
            "COMBO",
            center_x,
            combo_label_y,
            (s * 0.9).round().max(1.0),
            ColorRgba::new(180, 180, 200, 255),
        );

        let combo_label_h = 7.0 * (s * 0.9).round().max(1.0);
        judge_anchor_y = combo_label_y + combo_label_h + gap;
    }

    if let Some((grade, judge_time, delta_ms)) = last_judge {
        let elapsed = audio_time_seconds - judge_time;
        if elapsed >= 0.0 && elapsed < 0.5 {
            let (text, color) = match grade {
                JudgeGrade::PerfectGreat => ("PGREAT", ColorRgba::new(255, 230, 80, 255)),
                JudgeGrade::Great => ("GREAT", ColorRgba::new(255, 170, 50, 255)),
                JudgeGrade::Good => ("GOOD", ColorRgba::new(60, 220, 120, 255)),
                JudgeGrade::Bad => ("BAD", ColorRgba::new(180, 70, 240, 255)),
                JudgeGrade::Poor => ("POOR", ColorRgba::new(240, 50, 50, 255)),
                JudgeGrade::Miss => ("MISS", ColorRgba::new(140, 140, 140, 255)),
            };

            const POP_IN: f64 = 0.08;
            const FADE_OUT: f64 = 0.12;
            let pop_t = (elapsed / POP_IN).min(1.0) as f32;
            let pop_offset = (1.0 - crate::motion::ease_out_back(pop_t)) * 10.0 * s;
            let fade_start = 0.5 - FADE_OUT;
            let alpha = if elapsed > fade_start {
                let fade_t = ((elapsed - fade_start) / FADE_OUT) as f32;
                (255.0 * (1.0 - crate::motion::ease_in_cubic(fade_t))) as u8
            } else {
                255
            };

            let judge_scale = (2.0 * s).round().max(1.0);
            let judge_text_y = judge_anchor_y + pop_offset;
            font_atlas.draw_ascii_text_centered(
                batcher,
                backend,
                text,
                center_x,
                judge_text_y,
                judge_scale,
                color.with_alpha(alpha),
            );

            if grade != JudgeGrade::Miss && delta_ms.abs() >= 4.0 {
                let (fs_str, fs_col) = if delta_ms < 0.0 {
                    (
                        format!("FAST {:.0}ms", delta_ms),
                        ColorRgba::new(80, 210, 255, 255),
                    )
                } else {
                    (
                        format!("SLOW +{:.0}ms", delta_ms),
                        ColorRgba::new(255, 140, 60, 255),
                    )
                };
                let judge_glyph_h = 7.0 * judge_scale;
                let fast_slow_y = judge_text_y + judge_glyph_h + (gap / 2.0).max(2.0);
                font_atlas.draw_ascii_text_centered(
                    batcher,
                    backend,
                    &fs_str,
                    center_x,
                    fast_slow_y,
                    (s * 0.9).round().max(1.0),
                    fs_col.with_alpha(alpha),
                );
            }
        }
    }

    // 3. HUD Info (Title, Artist, BPM/Level, Score/Accuracy, Pacemaker, Judge breakdown)
    //
    // GPU batching note: `batcher` flushes whenever the bound texture changes
    // (solid rect = no texture, text = font atlas texture). Earlier passes in
    // this function already group every solid-color rect together ABOVE this
    // point; from here to the end of the frame everything is font-atlas text,
    // so this section stays text-only to avoid reintroducing a texture
    // round-trip that would blow the "1~3 draw calls per frame" budget
    // (see `test_render_gameplay_gpu_batched_draw_calls`).
    let hud_x = skin.playfield_x + skin.playfield_width + 48.0 * s;
    let mut hud_y = skin.playfield_y;
    let font_scale = (s * 0.9).round().max(1.0);
    let label_scale = (font_scale * 0.78).max(1.0);
    let label_col = ColorRgba::new(120, 128, 150, 255);

    font_atlas.draw_ascii_text(
        batcher,
        backend,
        &chart.header.title,
        hud_x,
        hud_y,
        (2.0 * s).round().max(1.0),
        ColorRgba::new(255, 255, 255, 255),
    );
    hud_y += 24.0 * s;

    font_atlas.draw_ascii_text(
        batcher,
        backend,
        &chart.header.artist,
        hud_x,
        hud_y,
        font_scale,
        ColorRgba::new(190, 195, 215, 255),
    );
    hud_y += 26.0 * s;

    // Small helper: a "LABEL" caption line followed by a bigger value line,
    // grouping related stats visually without needing a background box.
    macro_rules! stat_line {
        ($label:expr, $value:expr, $color:expr) => {{
            font_atlas.draw_ascii_text(
                batcher,
                backend,
                $label,
                hud_x,
                hud_y,
                label_scale,
                label_col,
            );
            hud_y += 11.0 * s;
            font_atlas.draw_ascii_text(batcher, backend, $value, hud_x, hud_y, font_scale, $color);
            hud_y += 17.0 * s;
        }};
    }

    let bpm_lvl = format!(
        "BPM {:.1}   LV {}",
        chart.header.bpm, chart.header.play_level
    );
    font_atlas.draw_ascii_text(
        batcher,
        backend,
        &bpm_lvl,
        hud_x,
        hud_y,
        font_scale,
        ColorRgba::new(200, 200, 220, 255),
    );
    hud_y += 22.0 * s;

    let ex_str = format!("{} / {}", score.ex_score, score.max_ex_score());
    stat_line!("EX SCORE", &ex_str, ColorRgba::new(255, 225, 80, 255));

    let acc_str = format!("{:.2}%", score.accuracy_rate());
    stat_line!("ACCURACY", &acc_str, ColorRgba::new(80, 210, 255, 255));

    let played_notes = score.pgreat_count
        + score.great_count
        + score.good_count
        + score.bad_count
        + score.poor_count
        + score.miss_count;
    let max_so_far = played_notes * 2;
    let aaa_target = ((max_so_far as f64) * 8.0 / 9.0).round() as i32;
    let pace_diff = score.ex_score as i32 - aaa_target;
    let pace_str = if pace_diff >= 0 {
        format!("+{}", pace_diff)
    } else {
        format!("{}", pace_diff)
    };
    let pace_col = if pace_diff >= 0 {
        ColorRgba::new(100, 255, 120, 255)
    } else {
        ColorRgba::new(255, 90, 90, 255)
    };
    stat_line!("PACEMAKER (AAA)", &pace_str, pace_col);
    hud_y += 6.0 * s;

    // Judge breakdown legend — the actual proportional bar was already
    // emitted earlier in the untextured-rect pass (see PULSE note above);
    // this is just the count labels underneath it.
    font_atlas.draw_ascii_text(
        batcher,
        backend,
        "JUDGE BREAKDOWN",
        hud_x,
        hud_y,
        label_scale,
        label_col,
    );
    hud_y += 20.0 * s;

    let counts = [
        (
            "PGREAT",
            score.pgreat_count,
            ColorRgba::new(255, 230, 80, 255),
        ),
        (
            "GREAT",
            score.great_count,
            ColorRgba::new(255, 170, 50, 255),
        ),
        ("GOOD", score.good_count, ColorRgba::new(60, 220, 120, 255)),
        ("BAD", score.bad_count, ColorRgba::new(200, 90, 240, 255)),
        ("POOR", score.poor_count, ColorRgba::new(240, 50, 50, 255)),
        ("MISS", score.miss_count, ColorRgba::new(150, 155, 175, 255)),
    ];
    let col_w = 110.0 * s;
    for row in 0..3 {
        let (l0, c0, col0) = counts[row * 2];
        let (l1, c1, col1) = counts[row * 2 + 1];
        let row0 = format!("{} {}", l0, c0);
        let row1 = format!("{} {}", l1, c1);
        font_atlas.draw_ascii_text(batcher, backend, &row0, hud_x, hud_y, font_scale, col0);
        font_atlas.draw_ascii_text(
            batcher,
            backend,
            &row1,
            hud_x + col_w,
            hud_y,
            font_scale,
            col1,
        );
        hud_y += 16.0 * s;
    }

    // 4. Footer text
    let footer_y = viewport.y + viewport.height - 30.0 * s;
    font_atlas.draw_ascii_text(
        batcher,
        backend,
        "[ESC] Pause / Exit    [F1] Help    [Tab] Options",
        hud_x,
        footer_y,
        font_scale,
        ColorRgba::new(140, 140, 160, 255),
    );

    // Flush all batched vertices to GPU in 1~3 draw calls!
    batcher.flush(backend);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SoftBackend;
    use beetle_core::{BmsHeader, JudgeEngine, Lane};

    #[test]
    fn test_render_gameplay_gpu_batched_draw_calls() {
        let mut backend = SoftBackend::new(1280, 720);
        let mut batcher = SpriteBatcher::new();
        let viewport = Viewport::new(1280, 720);
        let mut skin = SkinConfig::default();
        skin.update_layout(&viewport);
        let font_atlas = FontAtlas::new(&mut backend).expect("Font atlas creation");

        let chart = BmsChart {
            header: BmsHeader {
                title: "GPU Test Beat".to_string(),
                artist: "GPU Artist".to_string(),
                bpm: 150.0,
                ..Default::default()
            },
            max_measure: 50,
            ..Default::default()
        };

        let timing = TimingModel::from_chart(&chart);
        let judge = JudgeEngine::new(&chart, &timing, GaugeType::Groove);
        let mut score = ScoreTracker::new(10, 200.0, GaugeType::Groove);
        score.record_hit(JudgeGrade::PerfectGreat);
        score.record_hit(JudgeGrade::Great);

        let mut key_pressed = [false; LANE_COUNT];
        key_pressed[0] = true;
        key_pressed[3] = true;
        let hit_bursts = vec![HitBurst {
            lane: Lane::Key1,
            spawn_time: 1.0,
            grade: JudgeGrade::PerfectGreat,
        }];
        let visual_levels = [0.5f32; 16];

        render_gameplay_gpu(
            &mut backend,
            &mut batcher,
            &viewport,
            &skin,
            &font_atlas,
            &chart,
            judge.notes(),
            1.1,
            &score,
            &visual_levels,
            None,
            None,
            0.0,
            &timing,
            &key_pressed,
            &hit_bursts,
            Some((JudgeGrade::PerfectGreat, 1.0, 2.0)),
        );

        // Entire gameplay frame (quads, beams, bursts, notes, font atlas text) rendered in minimal draw batches!
        assert!(batcher.draw_call_count() >= 1 && batcher.draw_call_count() <= 3);
    }
}
