//! Loading screen on the `Ui` (Canvas + TextEngine + generated Skin).
//! Replaces `render_loading_screen` in `modals.rs`.
//!
//! The loader reports no progress (it only sends the finished result), so
//! the bar is indeterminate. Everything animates from `elapsed`, and the app
//! redraws every frame while loading (INV-5: the UI never stalls).

use super::widgets::{self, wrap2};
use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::jacket::{jacket_seed, THUMB_PX};
use crate::motion::ease_out_cubic;
use crate::screens::play::{cover_uv, SizedTexture};
use crate::skin::ColorRgba;
use crate::strings;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption, thousands};
use crate::ui::Ui;
use crate::view::Viewport;
use beetle_core::SongMetadata;
use std::time::Instant;

pub struct LoadingFrame<'a> {
    pub viewport: &'a Viewport,
    /// The frame time: the backdrop crossfade is measured against it.
    pub now: Instant,
    pub song: &'a SongMetadata,
    /// Notes under the rule the play is judged under (what the HUD and result count).
    pub notes: usize,
    pub jacket: Option<SizedTexture>,
    /// Dominant color of the jacket (ambient light).
    pub ambient: Option<ColorRgba>,
    /// Seconds since loading started.
    pub elapsed: f64,
    pub status: &'a str,
    /// How far the current stage is, 0 to 1, when it counts items; `None`
    /// sweeps the bar instead.
    pub fraction: Option<f32>,
    /// Play options in effect ("그린 500", ...).
    pub option_chips: &'a [String],
    /// "AUTO PLAY" / "REPLAY" when the run is not a normal play.
    pub badge: Option<&'a str>,
}

const ENTER_SECONDS: f64 = 0.35;

pub fn draw_loading(ui: &mut Ui, f: &LoadingFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let art = ui.backdrop.mix(f.now);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let jackets = &mut ui.jackets;
    let vp = f.viewport;
    let s = vp.scale;
    let song = f.song;
    let (tier, tier_col) = theme::level_tier(song.difficulty, song.play_level);

    widgets::backdrop_art(
        c,
        &sk,
        vp,
        f.ambient.map_or(tier_col, |a| theme::vivid(a, tier_col)),
        ui.clock,
        lite,
        art,
    );

    // Entrance: jacket and text slide in and fade up.
    let p = ease_out_cubic((f.elapsed / ENTER_SECONDS).clamp(0.0, 1.0) as f32);
    let a = |max: u8| (max as f32 * p) as u8;
    let slide = (1.0 - p) * 32.0 * s;

    // Jacket
    let jacket = Rect::new(
        vp.x + 160.0 * s - slide,
        vp.y + 148.0 * s,
        400.0 * s,
        300.0 * s,
    );
    c.halo(&sk.shadow, jacket, theme::WHITE.with_alpha(a(200)));
    match f.jacket {
        Some(tex) => {
            c.fill_rect(jacket, theme::BG.with_alpha(a(255)));
            c.image(
                tex.id,
                jacket,
                cover_uv(tex, jacket),
                theme::WHITE.with_alpha(a(255)),
            );
        }
        None => match jackets.lookup(song.picture_key(), jacket_seed(&song.title, &song.artist)) {
            // No stage image: the song's placeholder (or thumbnail), cropped to 4:3.
            Some((region, _)) => {
                c.fill_rect(jacket, theme::SURF3.with_alpha(a(255)));
                c.sprite_sub(
                    region,
                    Rect::new(0.0, 12.0, THUMB_PX as f32, 72.0),
                    jacket,
                    theme::WHITE.with_alpha(a(255)),
                );
            }
            None => {
                c.fill_rect_corners(
                    jacket,
                    [
                        tier_col.with_alpha(a(255)),
                        theme::SURF3.with_alpha(a(255)),
                        theme::BG.with_alpha(a(255)),
                        tier_col.with_alpha(a(160)),
                    ],
                );
                c.set_additive(true);
                c.sprite_centered(
                    sk.flare,
                    jacket.x + jacket.w * 0.7,
                    jacket.y + jacket.h * 0.3,
                    260.0 * s,
                    260.0 * s,
                    theme::WHITE.with_alpha(a(90)),
                );
                c.set_additive(false);
                t.draw_in(
                    c,
                    theme::mode_label(song.play_mode),
                    jacket.inset(20.0 * s),
                    Align::Left,
                    &TextStyle::new(72.0 * s)
                        .bold()
                        .color(theme::WHITE.with_alpha(a(40))),
                );
            }
        },
    }
    c.stroke_rect(jacket, s.max(1.0), theme::LINE.with_alpha(a(255)));

    // Song info
    let x = jacket.right() + 48.0 * s + slide * 2.0;
    let w = vp.x + vp.width - 160.0 * s - (jacket.right() + 48.0 * s);
    let mut y = jacket.y + 18.0 * s;
    let now = caption(12.0, s).color(theme::CYAN.with_alpha(a(255)));
    let nw = t.draw(c, strings::LOADING(), x, y, &now);
    if let Some(badge) = f.badge {
        let st = caption(12.0, s).color(theme::ON_ACCENT);
        let bw = t.measure(c, badge, &st) + 20.0 * s;
        let chip = Rect::new(x + nw + 16.0 * s, y - 15.0 * s, bw, 20.0 * s);
        c.nine(&sk.panel_lg, chip, theme::CYAN.with_alpha(a(255)));
        t.draw_in(
            c,
            badge,
            chip,
            Align::Center,
            &st.color(theme::ON_ACCENT.with_alpha(a(255))),
        );
    }
    y += 40.0 * s;
    t.draw(
        c,
        &format!("{tier} {}", song.play_level),
        x,
        y,
        &caption(12.0, s).color(tier_col.with_alpha(a(255))),
    );
    y += 42.0 * s;
    let big = TextStyle::new(34.0 * s)
        .bold()
        .color(theme::TEXT.with_alpha(a(255)));
    if t.measure(c, &song.title, &big) <= w {
        t.draw(c, &song.title, x, y, &big);
    } else {
        let st = TextStyle::new(26.0 * s)
            .bold()
            .color(theme::TEXT.with_alpha(a(255)));
        let (l1, l2) = wrap2(c, t, &song.title, w, &st);
        t.draw(c, &l1, x, y - 6.0 * s, &st);
        if let Some(l2) = l2 {
            y += 30.0 * s;
            t.draw(c, &l2, x, y - 6.0 * s, &st);
        }
    }
    y += 32.0 * s;
    let artist_st = TextStyle::new(16.0 * s).color(theme::MUTED.with_alpha(a(255)));
    let artist = t.fit(c, &song.artist, w, &artist_st).into_owned();
    t.draw(c, &artist, x, y, &artist_st);
    if !song.genre.is_empty() {
        y += 22.0 * s;
        let st = TextStyle::new(13.0 * s).color(theme::MUTED2.with_alpha(a(255)));
        let genre = t.fit(c, &song.genre, w, &st).into_owned();
        t.draw(c, &genre, x, y, &st);
    }

    // Chart stats, aligned to the jacket's bottom
    let bpm = song.bpm_label();
    let notes = thousands(f.notes as u32);
    for (i, (k, v)) in [
        (strings::BPM(), bpm.as_str()),
        (strings::NOTES(), notes.as_str()),
        (strings::MODE(), theme::mode_label(song.play_mode)),
    ]
    .iter()
    .enumerate()
    {
        let sx = x + i as f32 * 112.0 * s;
        t.draw(
            c,
            k,
            sx,
            jacket.bottom() - 74.0 * s,
            &caption(12.0, s).color(theme::MUTED2.with_alpha(a(255))),
        );
        t.draw(
            c,
            v,
            sx,
            jacket.bottom() - 46.0 * s,
            &TextStyle::new(24.0 * s)
                .bold()
                .color(theme::TEXT.with_alpha(a(255))),
        );
    }
    // Options in effect
    let mut cx = x;
    for chip in f.option_chips {
        let st = caption(12.0, s).color(theme::MUTED.with_alpha(a(255)));
        let cw = t.measure(c, chip, &st) + 20.0 * s;
        if cx + cw > x + w {
            break;
        }
        let r = Rect::new(cx, jacket.bottom() - 24.0 * s, cw, 24.0 * s);
        c.nine(&sk.panel_lg, r, theme::SURF2.with_alpha(a(255)));
        t.draw_in(c, chip, r, Align::Center, &st);
        cx += cw + 8.0 * s;
    }

    progress(c, t, &sk, f, s);
}

/// Progress: the bar fills with the stage's share done, or a highlight
/// sweeps along the track when the stage has no count; the status line with
/// animated dots, and the cancel hint.
fn progress(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &LoadingFrame, s: f32) {
    let vp = f.viewport;
    let track = Rect::new(
        vp.x + 160.0 * s,
        vp.y + 540.0 * s,
        vp.width - 320.0 * s,
        4.0 * s,
    );
    match f.fraction {
        Some(p) => {
            c.nine(&sk.panel_sm, track, theme::LINE);
            c.fill_rect_hgradient(
                Rect::new(track.x, track.y, track.w * p, track.h),
                theme::CYAN.with_alpha(120),
                theme::CYAN,
            );
        }
        None => widgets::sweep_bar(c, sk, track, f.elapsed, s),
    }

    // A counted stage moves on its own; only a stage without a count gets the dots.
    let dots = if f.fraction.is_some() {
        0
    } else {
        ((f.elapsed * 3.0) as usize) % 4
    };
    let status = format!("{}{}", f.status, ".".repeat(dots));
    t.draw(
        c,
        &status,
        track.x,
        track.y - 16.0 * s,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );
    let bar = widgets::footer_bar(c, vp, s);
    widgets::footer_hints(c, t, sk, &[("ESC", strings::CANCEL())], bar, s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::PlayMode;

    #[test]
    fn every_frame_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let song = SongMetadata {
            id: beetle_core::ChartId::synthetic(1),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 1,
            file_path: String::new(),
            title: "A very long song title that certainly needs two lines (Extended Mix)".into(),
            subtitle: String::new(),
            artist: "Artist".into(),
            genre: "GENRE".into(),
            bpm: 150.0,
            bpm_min: 150.0,
            bpm_max: 150.0,
            play_level: 12,
            notes_count: 1500,
            play_mode: PlayMode::Keys7,
            difficulty: None,
        };
        let chips = vec![strings::fill(strings::CHIP_GREEN(), &["500"])];
        let mut ui = Ui::new(vp.scale);
        for elapsed in [0.0, 0.1, 0.7, 3.3] {
            ui.begin(1280, 720, vp.scale);
            draw_loading(
                &mut ui,
                &LoadingFrame {
                    viewport: &vp,
                    now: std::time::Instant::now(),
                    song: &song,
                    notes: 1500,
                    jacket: None,
                    ambient: None,
                    elapsed,
                    status: strings::LOADING(),
                    fraction: None,
                    option_chips: &chips,
                    badge: Some(strings::AUTO_PLAY()),
                },
            );
            assert_eq!(ui.canvas.debug_batches().len(), 1, "elapsed={elapsed}");
        }
    }

    #[test]
    fn a_settled_backdrop_adds_one_picture_and_one_scrim_to_the_batch() {
        let vp = Viewport::new(1280, 720);
        let song = SongMetadata {
            id: beetle_core::ChartId::synthetic(3),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 3,
            file_path: String::new(),
            title: "Backdrop".into(),
            subtitle: String::new(),
            artist: "Artist".into(),
            genre: "GENRE".into(),
            bpm: 150.0,
            bpm_min: 150.0,
            bpm_max: 150.0,
            play_level: 12,
            notes_count: 1500,
            play_mode: PlayMode::Keys7,
            difficulty: None,
        };
        let chips = vec![strings::fill(strings::CHIP_GREEN(), &["500"])];
        let mut ui = Ui::new(vp.scale);
        let draw = |ui: &mut Ui| {
            ui.begin(1280, 720, vp.scale);
            draw_loading(
                ui,
                &LoadingFrame {
                    viewport: &vp,
                    now: std::time::Instant::now(),
                    song: &song,
                    notes: 1500,
                    jacket: None,
                    ambient: None,
                    elapsed: 3.3,
                    status: strings::LOADING(),
                    fraction: None,
                    option_chips: &chips,
                    badge: None,
                },
            );
            (
                ui.canvas.debug_vertices().len(),
                ui.canvas.debug_batches().len(),
            )
        };
        let (plain, batches) = draw(&mut ui);
        assert_eq!(batches, 1);

        // A picture shown long enough ago to be fully faded in.
        let settled = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let key = crate::jacket::JacketKey::Placeholder(1);
        let picture =
            vec![90u8; (crate::backdrop::BACKDROP_W * crate::backdrop::BACKDROP_H * 4) as usize];
        assert!(ui
            .backdrop
            .show(ui.canvas.atlas_mut(), key, settled, || picture.clone()));
        let (with_art, batches) = draw(&mut ui);
        assert_eq!(batches, 1, "the picture rides in the main batch");
        // One textured quad for the picture and one fill for the scrim: 4 vertices each.
        assert_eq!(with_art - plain, 8);
    }
}
