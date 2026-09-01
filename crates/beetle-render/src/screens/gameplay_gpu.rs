use crate::backend::{BlendMode, FontAtlas, GpuBackend, SpriteBatcher, TextureId};
use crate::renderer::{lane_index, HitBurst, Viewport};
use crate::skin::{ColorRgba, SkinConfig};
use beetle_core::{BmsChart, GaugeType, JudgeGrade, NoteType, PlayNote, ScoreTracker, TimingModel};

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
    key_pressed: &[bool; 8],
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
        batcher.draw_rect(backend, x, skin.playfield_y, line_w, skin.playfield_height, line_col_f32);
    }
    let right_x = skin.playfield_x + skin.playfield_width;
    batcher.draw_rect(backend, right_x, skin.playfield_y, line_w, skin.playfield_height, line_col_f32);

    // Danger pulsing border around playfield
    let is_danger = (score.gauge < 30.0 && matches!(score.gauge_type, GaugeType::Hard | GaugeType::Groove))
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
            batcher.draw_rect(backend, px, bar_y - bar_line_h * 0.5, pw, bar_line_h, bar_line_col);
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
        let note_col_f32 = note_col.to_f32_array();

        match note.note_event.note_type {
            NoteType::Tap => {
                if note_y + note_h >= top_y && note_y - note_h <= judge_y + 40.0 * s {
                    batcher.draw_rect(backend, lane_x, note_y - note_h, lane_w, note_h, note_col_f32);
                }
            }
            NoteType::LongNoteStart => {
                let end_delta = note.end_target_time_seconds - audio_time_seconds;
                let end_y = judge_y - (end_delta as f32 * effective_speed);
                let body_top = end_y.max(top_y);
                let body_bottom = note_y.min(judge_y);

                if body_bottom > body_top {
                    let body_color = note_col.with_alpha(140).to_f32_array();
                    batcher.draw_rect(
                        backend,
                        lane_x + 3.0 * s,
                        body_top,
                        lane_w - 6.0 * s,
                        body_bottom - body_top,
                        body_color,
                    );
                }
                if note_y + note_h >= top_y && note_y <= judge_y + 40.0 * s {
                    batcher.draw_rect(backend, lane_x, note_y - note_h, lane_w, note_h, note_col_f32);
                }
                if end_y + note_h >= top_y && end_y <= judge_y + 40.0 * s {
                    batcher.draw_rect(backend, lane_x, end_y - note_h, lane_w, note_h, note_col_f32);
                }
            }
            _ => (),
        }
    }

    // 7. Lane Cover
    if skin.lane_cover_ratio > 0.0 {
        let ratio = skin.lane_cover_ratio.clamp(0.0, 0.85);
        let cover_h = skin.playfield_height * ratio;
        let cover_col = ColorRgba::new(12, 12, 18, 255).to_f32_array();
        let border_col = ColorRgba::new(80, 140, 255, 255).to_f32_array();
        batcher.draw_rect(backend, skin.playfield_x, skin.playfield_y, skin.playfield_width, cover_h, cover_col);
        batcher.draw_rect(backend, skin.playfield_x, skin.playfield_y + cover_h - 2.0 * s, skin.playfield_width, 2.0 * s, border_col);
    }

    // 8. Core Judge Line
    let judge_line_h = (2.0 * s).max(2.0);
    batcher.draw_rect(backend, px, judge_y, pw, judge_line_h, skin.judge_line_color.to_f32_array());

    // 9. Gauge Bar Box & Fill
    let gauge_x = skin.playfield_x + skin.playfield_width + 16.0 * s;
    let gauge_y = skin.playfield_y;
    let gauge_w = 22.0 * s;
    let gauge_h = skin.playfield_height;

    batcher.draw_rect(backend, gauge_x, gauge_y, gauge_w, gauge_h, ColorRgba::new(20, 20, 28, 255).to_f32_array());

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
    batcher.draw_rect(backend, gauge_x, fill_y, gauge_w, fill_h, fill_color.to_f32_array());

    let b_border_col = if danger_blink {
        ColorRgba::new(255, 60, 60, 255).to_f32_array()
    } else {
        ColorRgba::new(80, 80, 100, 255).to_f32_array()
    };
    let b_line = (1.0 * s).max(1.0);
    batcher.draw_rect(backend, gauge_x, gauge_y, gauge_w, b_line, b_border_col);
    batcher.draw_rect(backend, gauge_x, gauge_y + gauge_h - b_line, gauge_w, b_line, b_border_col);
    batcher.draw_rect(backend, gauge_x, gauge_y, b_line, gauge_h, b_border_col);
    batcher.draw_rect(backend, gauge_x + gauge_w - b_line, gauge_y, b_line, gauge_h, b_border_col);

    if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
        let line_y = gauge_y + gauge_h * 0.2;
        batcher.draw_rect(backend, gauge_x - 3.0 * s, line_y, gauge_w + 6.0 * s, 2.0 * s, ColorRgba::new(255, 220, 50, 255).to_f32_array());
    }

    // 10. BGA Frame container
    let side_x = skin.playfield_x + skin.playfield_width + 48.0 * s;
    let bga_y = skin.playfield_y + 240.0 * s;
    let max_w = (viewport.x + viewport.width - side_x - 24.0 * s).max(100.0);
    let bga_w = (520.0 * s).min(max_w);
    let bga_h = (bga_w * 9.0 / 16.0).round();

    batcher.draw_rect(backend, side_x - 2.0 * s, bga_y - 2.0 * s, bga_w + 4.0 * s, bga_h + 4.0 * s, ColorRgba::new(50, 60, 80, 255).to_f32_array());
    batcher.draw_rect(backend, side_x, bga_y, bga_w, bga_h, ColorRgba::new(8, 8, 12, 255).to_f32_array());

    // PASS 2: BGA Sprites (Texture: BGA Texture, Blend: Alpha)
    if let Some(tex) = bga_texture {
        batcher.draw_sprite(backend, tex, side_x, bga_y, bga_w, bga_h, [1.0, 1.0, 1.0, 1.0]);
    }
    if let Some(tex) = layer_texture {
        batcher.draw_sprite(backend, tex, side_x, bga_y, bga_w, bga_h, [1.0, 1.0, 1.0, 1.0]);
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
            batcher.draw_rect_with_blend(backend, x, skin.playfield_y, w, beam_h, beam_color, BlendMode::Additive);
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
    batcher.draw_rect_with_blend(backend, px, judge_y - 3.0 * s, pw, 7.0 * s, glow_color, BlendMode::Additive);

    // 3. Hit Bursts
    let burst_duration = 0.22;
    for burst in hit_bursts {
        let elapsed = (audio_time_seconds - burst.spawn_time).max(0.0);
        if elapsed >= burst_duration {
            continue;
        }
        let progress = (elapsed / burst_duration) as f32;
        let alpha = (1.0 - progress).clamp(0.0, 1.0);

        let lx = skin.lane_x(burst.lane) + skin.lane_width(burst.lane) / 2.0;
        let (r, g, b) = match burst.grade {
            JudgeGrade::PerfectGreat => (1.0, 0.9, 0.3),
            JudgeGrade::Great => (1.0, 0.65, 0.2),
            JudgeGrade::Good => (0.24, 0.86, 0.47),
            _ => (0.63, 0.63, 0.7),
        };

        if burst.grade == JudgeGrade::PerfectGreat {
            let flash_alpha = alpha * 0.2;
            let lane_x = skin.lane_x(burst.lane);
            let lane_w = skin.lane_width(burst.lane);
            batcher.draw_rect_with_blend(
                backend,
                lane_x,
                skin.playfield_y,
                lane_w,
                judge_y - skin.playfield_y,
                [r, g, b, flash_alpha],
                BlendMode::Additive,
            );
        }

        let spark_size = (18.0 * (1.0 - progress * 0.5) * s).max(4.0);
        let spark_col = [r, g, b, alpha];
        let dist = progress * 40.0 * s;
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
            ColorRgba::new(255, 90, 90, 220).to_f32_array()
        } else if level > 0.4 {
            ColorRgba::new(255, 210, 60, 200).to_f32_array()
        } else {
            ColorRgba::new(60, 180, 255, 180).to_f32_array()
        };
        batcher.draw_rect_with_blend(backend, bx, by, single_bar_w, bar_h, col, BlendMode::Additive);
    }

    // PASS 4: Font Atlas Batched Text (Texture: FontAtlas, Blend: Alpha)
    // 1. Gauge percentage text below bar
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

    if score.current_combo > 0 {
        let combo_str = format!("{}", score.current_combo);
        let pulse_offset = if let Some((_, judge_time, _)) = last_judge {
            let elapsed = audio_time_seconds - judge_time;
            if elapsed >= 0.0 && elapsed < 0.12 {
                ((1.0 - (elapsed / 0.12)) * 6.0 * s as f64) as f32
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

        font_atlas.draw_ascii_text_centered(
            batcher,
            backend,
            "COMBO",
            center_x,
            combo_y + 24.0 * s,
            (s * 0.9).round().max(1.0),
            ColorRgba::new(180, 180, 200, 255),
        );
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

            font_atlas.draw_ascii_text_centered(
                batcher,
                backend,
                text,
                center_x,
                judge_center_y + 8.0 * s,
                (2.0 * s).round().max(1.0),
                color,
            );

            if grade != JudgeGrade::Miss && delta_ms.abs() >= 4.0 {
                let (fs_str, fs_col) = if delta_ms < 0.0 {
                    (format!("FAST {:.0}ms", delta_ms), ColorRgba::new(80, 210, 255, 255))
                } else {
                    (format!("SLOW +{:.0}ms", delta_ms), ColorRgba::new(255, 140, 60, 255))
                };
                font_atlas.draw_ascii_text_centered(
                    batcher,
                    backend,
                    &fs_str,
                    center_x,
                    judge_center_y + 28.0 * s,
                    (s * 0.9).round().max(1.0),
                    fs_col,
                );
            }
        }
    }

    // 3. HUD Info (Title, Artist, BPM, Scores)
    let hud_x = skin.playfield_x + skin.playfield_width + 48.0 * s;
    let mut hud_y = skin.playfield_y;
    let font_scale = (s * 0.9).round().max(1.0);

    font_atlas.draw_ascii_text(batcher, backend, &chart.header.title, hud_x, hud_y, (2.0 * s).round().max(1.0), ColorRgba::new(255, 255, 255, 255));
    hud_y += 22.0 * s;

    font_atlas.draw_ascii_text(batcher, backend, &chart.header.artist, hud_x, hud_y, font_scale, ColorRgba::new(160, 160, 180, 255));
    hud_y += 28.0 * s;

    let bpm_str = format!("BPM: {:.1}", chart.header.bpm);
    font_atlas.draw_ascii_text(batcher, backend, &bpm_str, hud_x, hud_y, font_scale, ColorRgba::new(200, 200, 220, 255));
    hud_y += 16.0 * s;

    let lvl_str = format!("LEVEL: {}", chart.header.play_level);
    font_atlas.draw_ascii_text(batcher, backend, &lvl_str, hud_x, hud_y, font_scale, ColorRgba::new(200, 200, 220, 255));
    hud_y += 26.0 * s;

    let ex_str = format!("EX SCORE: {} / {}", score.ex_score, score.max_ex_score());
    font_atlas.draw_ascii_text(batcher, backend, &ex_str, hud_x, hud_y, font_scale, ColorRgba::new(255, 230, 100, 255));
    hud_y += 16.0 * s;

    let acc_str = format!("ACCURACY: {:.2}%", score.accuracy_rate());
    font_atlas.draw_ascii_text(batcher, backend, &acc_str, hud_x, hud_y, font_scale, ColorRgba::new(100, 220, 255, 255));
    hud_y += 18.0 * s;

    let played_notes = score.pgreat_count + score.great_count + score.good_count + score.bad_count + score.poor_count + score.miss_count;
    let max_so_far = played_notes * 2;
    let aaa_target = ((max_so_far as f64) * 8.0 / 9.0).round() as i32;
    let pace_diff = score.ex_score as i32 - aaa_target;
    let (pace_str, pace_col) = if pace_diff >= 0 {
        (format!("PACEMAKER (AAA): +{}", pace_diff), ColorRgba::new(100, 255, 120, 255))
    } else {
        (format!("PACEMAKER (AAA): {}", pace_diff), ColorRgba::new(255, 90, 90, 255))
    };
    font_atlas.draw_ascii_text(batcher, backend, &pace_str, hud_x, hud_y, font_scale, pace_col);
    hud_y += 22.0 * s;

    let counts = [
        ("PGREAT", score.pgreat_count, ColorRgba::new(255, 230, 80, 255)),
        ("GREAT ", score.great_count, ColorRgba::new(255, 170, 50, 255)),
        ("GOOD  ", score.good_count, ColorRgba::new(60, 220, 120, 255)),
        ("BAD   ", score.bad_count, ColorRgba::new(180, 70, 240, 255)),
        ("POOR  ", score.poor_count, ColorRgba::new(240, 50, 50, 255)),
        ("MISS  ", score.miss_count, ColorRgba::new(140, 140, 140, 255)),
    ];
    for (label, count, col) in counts {
        let row = format!("{}: {:>4}", label, count);
        font_atlas.draw_ascii_text(batcher, backend, &row, hud_x, hud_y, font_scale, col);
        hud_y += 14.0 * s;
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

        let key_pressed = [true, false, false, true, false, false, false, false];
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
