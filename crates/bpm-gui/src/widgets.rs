//! Drawing primitives for the window: the color theme, rounded boxes, text at
//! real pixel sizes, and buttons that record where they were drawn so a click
//! can be matched to the action under the mouse (see `Hit`).

use crate::bitmap_font::BitmapFont;
use beetle_render::skin::ColorRgba;
use tiny_skia::{
    Color, FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Shader, Stroke, StrokeDash, Transform,
};

/// The window's colors. Every screen draws with these only.
pub mod theme {
    use beetle_render::skin::ColorRgba;

    pub const BG: ColorRgba = ColorRgba::new(15, 16, 22, 255);
    pub const SURFACE: ColorRgba = ColorRgba::new(22, 24, 32, 255);
    pub const SURFACE_2: ColorRgba = ColorRgba::new(31, 34, 45, 255);
    pub const SURFACE_3: ColorRgba = ColorRgba::new(42, 46, 60, 255);
    pub const BORDER: ColorRgba = ColorRgba::new(48, 52, 68, 255);
    pub const TEXT: ColorRgba = ColorRgba::new(236, 238, 244, 255);
    pub const TEXT_DIM: ColorRgba = ColorRgba::new(160, 165, 182, 255);
    pub const TEXT_FAINT: ColorRgba = ColorRgba::new(108, 113, 130, 255);
    pub const ACCENT: ColorRgba = ColorRgba::new(255, 200, 70, 255);
    pub const ACCENT_TEXT: ColorRgba = ColorRgba::new(28, 22, 6, 255);
    pub const ACCENT_SOFT: ColorRgba = ColorRgba::new(58, 48, 22, 255);
    pub const SELECT: ColorRgba = ColorRgba::new(38, 46, 72, 255);
    pub const BLUE: ColorRgba = ColorRgba::new(110, 180, 255, 255);
    pub const GREEN: ColorRgba = ColorRgba::new(96, 212, 146, 255);
    pub const GREEN_SOFT: ColorRgba = ColorRgba::new(24, 52, 38, 255);
    pub const WARN: ColorRgba = ColorRgba::new(255, 190, 80, 255);
    pub const WARN_SOFT: ColorRgba = ColorRgba::new(60, 46, 20, 255);
    pub const DANGER: ColorRgba = ColorRgba::new(240, 100, 100, 255);
    pub const DANGER_SOFT: ColorRgba = ColorRgba::new(64, 28, 30, 255);
    pub const SCRIM: ColorRgba = ColorRgba::new(0, 0, 0, 170);
}

/// Text sizes in px.
pub const PX_SMALL: u16 = 13;
pub const PX_BODY: u16 = 15;
pub const PX_TITLE: u16 = 18;
pub const PX_PAGE: u16 = 22;

/// What a click on a drawn region does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    /// Swallows the click (the area behind an open dialog).
    None,
    Tab(crate::ui::ActiveTab),
    Help,
    FocusSearch,
    ClearSearch,
    // My songs
    OpenAdd,
    OpenLibrary,
    OpenAdvanced,
    SelectInstalled(usize),
    UseVersion(usize),
    AskUninstallVersion(usize),
    AskUninstall,
    AskRemoveBga,
    // Get songs
    SelectRemote(usize),
    LevelFilter(u8),
    ToggleWithBga,
    InstallRemote,
    SyncSources,
    // Difficulty tables
    PrevTable,
    NextTable,
    SelectTableRow(usize),
    OpenSongPage,
    OpenChartPage,
    AskDownloadChart,
    AskAddFromArchive,
    ScanCollection,
    AskAddTable,
    // The open dialog
    DialogFocus(usize),
    DialogBrowse(usize),
    DialogConfirm,
    DialogCancel,
    DialogChoice(usize, usize),
    DialogListRemove(usize),
    DialogCard(usize),
    CancelTask,
}

/// A list the mouse wheel scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Installed,
    Remote,
    Tables,
}

/// A clickable region drawn this frame.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub action: UiAction,
}

impl Hit {
    pub fn contains(&self, (mx, my): (f32, f32)) -> bool {
        mx >= self.x && mx < self.x + self.w && my >= self.y && my < self.y + self.h
    }
}

/// Selection and scroll position of one list. The keyboard moves the
/// selection and asks the list to follow it; the wheel only scrolls.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListView {
    pub selected: usize,
    pub scroll: usize,
    pub follow: bool,
}

impl ListView {
    /// Clamps to a list of `len` rows of which `visible` fit, scrolling to the
    /// selection when asked to.
    pub fn layout(&mut self, len: usize, visible: usize) {
        self.selected = self.selected.min(len.saturating_sub(1));
        if self.follow && visible > 0 {
            if self.selected < self.scroll {
                self.scroll = self.selected;
            } else if self.selected >= self.scroll + visible {
                self.scroll = self.selected + 1 - visible;
            }
            self.follow = false;
        }
        self.scroll = self.scroll.min(len.saturating_sub(visible));
    }

    pub fn move_by(&mut self, delta: isize, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, len as isize - 1) as usize;
        self.follow = true;
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index;
        self.follow = true;
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = (self.scroll as isize + delta).max(0) as usize;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

/// Cap height of text at `px`, for centering text in a box.
pub fn cap(px: u16) -> f32 {
    (px as f32 * 0.73).round()
}

pub fn text_w(text: &str, px: u16) -> f32 {
    BitmapFont::text_width_px(text, px, false)
}

pub fn text_w_bold(text: &str, px: u16) -> f32 {
    BitmapFont::text_width_px(text, px, true)
}

/// Cuts `text` to `max_w` px, ending with "…" when cut.
pub fn fit(text: &str, max_w: f32, px: u16, bold: bool) -> String {
    if max_w <= 0.0 {
        return String::new();
    }
    if BitmapFont::text_width_px(text, px, bold) <= max_w {
        return text.to_string();
    }
    let ell_w = BitmapFont::text_width_px("...", px, bold);
    let mut out = String::new();
    let mut width = 0.0;
    for c in text.chars() {
        let cw = BitmapFont::text_width_px(c.encode_utf8(&mut [0; 4]), px, bold);
        if width + cw + ell_w > max_w {
            break;
        }
        width += cw;
        out.push(c);
    }
    out.push_str("...");
    out
}

/// Splits `text` into lines no wider than `max_w` px, breaking at spaces
/// where it can and anywhere inside a word that is too long.
pub fn wrap(text: &str, max_w: f32, px: u16) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if text_w(&candidate, px) <= max_w {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for c in word.chars() {
            let mut next = line.clone();
            next.push(c);
            if text_w(&next, px) > max_w && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            line.push(c);
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

fn skia(color: ColorRgba) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(Color::from_rgba8(color.r, color.g, color.b, color.a)),
        anti_alias: true,
        ..Default::default()
    }
}

fn round_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// The pixmap the window shows, plus what was drawn where this frame.
pub struct GuiRenderer {
    pub pixmap: Pixmap,
    /// Clickable regions in drawing order: the last one under the mouse wins.
    pub hits: Vec<Hit>,
    pub scroll_areas: Vec<(Hit, ScrollTarget)>,
    pub cursor: (f32, f32),
    /// Off while a dialog covers the screen, so nothing behind it lights up.
    hover_enabled: bool,
}

impl GuiRenderer {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let pixmap = Pixmap::new(width.max(1), height.max(1))?;
        Some(Self {
            pixmap,
            hits: Vec::new(),
            scroll_areas: Vec::new(),
            cursor: (-1.0, -1.0),
            hover_enabled: true,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0
            && height > 0
            && (self.pixmap.width() != width || self.pixmap.height() != height)
        {
            if let Some(new_pixmap) = Pixmap::new(width, height) {
                self.pixmap = new_pixmap;
            }
        }
    }

    pub fn begin_frame(&mut self, hover_enabled: bool) {
        self.hits.clear();
        self.scroll_areas.clear();
        self.hover_enabled = hover_enabled;
        self.pixmap.fill(Color::from_rgba8(
            theme::BG.r,
            theme::BG.g,
            theme::BG.b,
            255,
        ));
    }

    pub fn set_hover_enabled(&mut self, on: bool) {
        self.hover_enabled = on;
    }

    /// The action of the topmost region under `pos`.
    pub fn hit_at(&self, pos: (f32, f32)) -> Option<UiAction> {
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.contains(pos))
            .map(|hit| hit.action)
    }

    pub fn scroll_target_at(&self, pos: (f32, f32)) -> Option<ScrollTarget> {
        if self.hit_at(pos) == Some(UiAction::None) {
            return None;
        }
        self.scroll_areas
            .iter()
            .rev()
            .find(|(area, _)| area.contains(pos))
            .map(|(_, target)| *target)
    }

    pub fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, action: UiAction) {
        self.hits.push(Hit { x, y, w, h, action });
    }

    pub fn scroll_area(&mut self, x: f32, y: f32, w: f32, h: f32, target: ScrollTarget) {
        self.scroll_areas.push((
            Hit {
                x,
                y,
                w,
                h,
                action: UiAction::None,
            },
            target,
        ));
    }

    pub fn hovered(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        let (mx, my) = self.cursor;
        self.hover_enabled && mx >= x && mx < x + w && my >= y && my < y + h
    }

    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: ColorRgba) {
        if let Some(rect) = Rect::from_xywh(x, y, w, h) {
            self.pixmap
                .fill_rect(rect, &skia(color), Transform::identity(), None);
        }
    }

    pub fn round(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: ColorRgba) {
        if let Some(path) = round_path(x, y, w, h, r) {
            self.pixmap.fill_path(
                &path,
                &skia(color),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    /// A 1px rounded outline, optionally dashed.
    #[allow(clippy::too_many_arguments)]
    pub fn outline(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        color: ColorRgba,
        dashed: bool,
    ) {
        if let Some(path) = round_path(x + 0.5, y + 0.5, w - 1.0, h - 1.0, r) {
            let stroke = Stroke {
                width: if dashed { 1.5 } else { 1.0 },
                dash: if dashed {
                    StrokeDash::new(vec![7.0, 5.0], 0.0)
                } else {
                    None
                },
                ..Default::default()
            };
            self.pixmap
                .stroke_path(&path, &skia(color), &stroke, Transform::identity(), None);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: ColorRgba) {
        if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
            self.pixmap.fill_path(
                &path,
                &skia(color),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    /// Text with its cap-height line at `y`.
    pub fn text(&mut self, text: &str, x: f32, y: f32, px: u16, color: ColorRgba) {
        BitmapFont::draw_text_px(&mut self.pixmap.as_mut(), text, x, y, px, false, color);
    }

    pub fn text_bold(&mut self, text: &str, x: f32, y: f32, px: u16, color: ColorRgba) {
        BitmapFont::draw_text_px(&mut self.pixmap.as_mut(), text, x, y, px, true, color);
    }

    /// Text centered vertically in the band `y..y+h`, cut to `max_w`.
    #[allow(clippy::too_many_arguments)]
    pub fn text_in(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        h: f32,
        max_w: f32,
        px: u16,
        color: ColorRgba,
    ) {
        let shown = fit(text, max_w, px, false);
        self.text(&shown, x, y + ((h - cap(px)) / 2.0).round(), px, color);
    }

    /// A small rounded label such as "설치됨". Returns its width.
    pub fn chip(&mut self, label: &str, x: f32, y: f32, fg: ColorRgba, bg: ColorRgba) -> f32 {
        let w = text_w(label, PX_SMALL) + 14.0;
        let h = 20.0;
        self.round(x, y, w, h, 10.0, bg);
        self.text(
            label,
            x + 7.0,
            y + ((h - cap(PX_SMALL)) / 2.0).round(),
            PX_SMALL,
            fg,
        );
        w
    }

    /// Width a button with `label` needs.
    pub fn button_w(label: &str) -> f32 {
        text_w(label, PX_BODY) + 32.0
    }

    /// Draws a button and records its click region.
    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        style: Btn,
        action: UiAction,
    ) {
        let hover = self.hovered(x, y, w, h);
        let (bg, fg, border) = match style {
            Btn::Primary => (
                if hover {
                    ColorRgba::new(255, 214, 110, 255)
                } else {
                    theme::ACCENT
                },
                theme::ACCENT_TEXT,
                None,
            ),
            Btn::Secondary => (
                if hover {
                    theme::SURFACE_3
                } else {
                    theme::SURFACE_2
                },
                theme::TEXT,
                Some(theme::BORDER),
            ),
            Btn::Danger => (
                if hover {
                    ColorRgba::new(84, 34, 36, 255)
                } else {
                    theme::DANGER_SOFT
                },
                ColorRgba::new(255, 150, 150, 255),
                Some(ColorRgba::new(110, 50, 54, 255)),
            ),
            Btn::Ghost => (
                if hover {
                    theme::SURFACE_2
                } else {
                    ColorRgba::new(0, 0, 0, 0)
                },
                if hover { theme::TEXT } else { theme::TEXT_DIM },
                None,
            ),
        };
        self.round(x, y, w, h, 7.0, bg);
        if let Some(border) = border {
            self.outline(x, y, w, h, 7.0, border, false);
        }
        let shown = fit(label, w - 16.0, PX_BODY, style == Btn::Primary);
        let tw = BitmapFont::text_width_px(&shown, PX_BODY, style == Btn::Primary);
        let tx = (x + (w - tw) / 2.0).round();
        let ty = (y + (h - cap(PX_BODY)) / 2.0).round();
        if style == Btn::Primary {
            self.text_bold(&shown, tx, ty, PX_BODY, fg);
        } else {
            self.text(&shown, tx, ty, PX_BODY, fg);
        }
        self.hit(x, y, w, h, action);
    }

    /// A row of buttons ending at `right`; returns the x where the row starts.
    pub fn buttons_right(
        &mut self,
        right: f32,
        y: f32,
        h: f32,
        items: &[(&str, Btn, UiAction)],
    ) -> f32 {
        let mut x = right;
        for (label, style, action) in items.iter().rev() {
            let w = Self::button_w(label);
            x -= w;
            self.button(x, y, w, h, label, *style, *action);
            x -= 8.0;
        }
        x + 8.0
    }

    /// A checkbox with a label; the whole line is clickable.
    pub fn checkbox(&mut self, x: f32, y: f32, label: &str, checked: bool, action: UiAction) {
        let w = 26.0 + text_w(label, PX_BODY);
        let hover = self.hovered(x, y, w, 22.0);
        if checked {
            self.round(x, y + 2.0, 18.0, 18.0, 4.0, theme::ACCENT);
            // A check mark from two short strokes.
            let mut pb = PathBuilder::new();
            pb.move_to(x + 4.5, y + 11.0);
            pb.line_to(x + 8.0, y + 14.5);
            pb.line_to(x + 14.0, y + 7.0);
            if let Some(path) = pb.finish() {
                let stroke = Stroke {
                    width: 2.2,
                    ..Default::default()
                };
                self.pixmap.stroke_path(
                    &path,
                    &skia(theme::ACCENT_TEXT),
                    &stroke,
                    Transform::identity(),
                    None,
                );
            }
        } else {
            self.round(x, y + 2.0, 18.0, 18.0, 4.0, theme::SURFACE_2);
            self.outline(
                x,
                y + 2.0,
                18.0,
                18.0,
                4.0,
                if hover {
                    theme::TEXT_DIM
                } else {
                    theme::BORDER
                },
                false,
            );
        }
        let color = if hover { theme::TEXT } else { theme::TEXT_DIM };
        self.text(
            label,
            x + 26.0,
            y + ((22.0 - cap(PX_BODY)) / 2.0).round(),
            PX_BODY,
            color,
        );
        self.hit(x, y, w, 22.0, action);
    }

    /// A text field. `browse` adds a "찾아보기" button on the right.
    #[allow(clippy::too_many_arguments)]
    pub fn text_field(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        value: &str,
        placeholder: &str,
        focused: bool,
        focus_action: UiAction,
        browse: Option<UiAction>,
    ) {
        let h = 36.0;
        let browse_w = if browse.is_some() {
            Self::button_w("찾아보기...")
        } else {
            0.0
        };
        let field_w = if browse.is_some() {
            w - browse_w - 8.0
        } else {
            w
        };
        let hover = self.hovered(x, y, field_w, h);
        self.round(x, y, field_w, h, 7.0, theme::BG);
        let border = if focused {
            theme::ACCENT
        } else if hover {
            theme::TEXT_FAINT
        } else {
            theme::BORDER
        };
        self.outline(x, y, field_w, h, 7.0, border, false);
        let inner_w = field_w - 24.0;
        let ty = (y + (h - cap(PX_BODY)) / 2.0).round();
        if value.is_empty() {
            let px_x = if focused { x + 17.0 } else { x + 12.0 };
            let shown = fit(placeholder, inner_w - 5.0, PX_BODY, false);
            self.text(&shown, px_x, ty, PX_BODY, theme::TEXT_FAINT);
            if focused {
                self.fill(x + 12.0, y + 9.0, 1.5, h - 18.0, theme::ACCENT);
            }
        } else {
            // Show the end of a long value: that is where typing happens.
            let mut shown = value.to_string();
            if text_w(&shown, PX_BODY) > inner_w {
                let chars: Vec<char> = value.chars().collect();
                let mut start = 0;
                while start < chars.len()
                    && text_w(&chars[start..].iter().collect::<String>(), PX_BODY) + 14.0 > inner_w
                {
                    start += 1;
                }
                shown = format!("...{}", chars[start..].iter().collect::<String>());
            }
            self.text(&shown, x + 12.0, ty, PX_BODY, theme::TEXT);
            if focused {
                let cx = x + 13.0 + text_w(&shown, PX_BODY);
                self.fill(cx, y + 9.0, 1.5, h - 18.0, theme::ACCENT);
            }
        }
        self.hit(x, y, field_w, h, focus_action);
        if let Some(action) = browse {
            self.button(
                x + field_w + 8.0,
                y,
                browse_w,
                h,
                "찾아보기...",
                Btn::Secondary,
                action,
            );
        }
    }

    /// A rounded panel with an optional border.
    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.round(x, y, w, h, 10.0, theme::SURFACE);
        self.outline(x, y, w, h, 10.0, ColorRgba::new(34, 37, 50, 255), false);
    }

    /// A thin scroll bar for a list showing `visible` of `len` rows from `first`.
    #[allow(clippy::too_many_arguments)]
    pub fn scrollbar(&mut self, x: f32, y: f32, h: f32, len: usize, visible: usize, first: usize) {
        if len <= visible || len == 0 {
            return;
        }
        let thumb_h = (h * visible as f32 / len as f32).max(24.0);
        let max_first = (len - visible) as f32;
        let thumb_y = y + (h - thumb_h) * (first as f32 / max_first).min(1.0);
        self.round(x, thumb_y, 4.0, thumb_h, 2.0, theme::SURFACE_3);
    }
}
