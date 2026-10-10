//! The window's screens: the header with the three tabs, each tab's page, the
//! open dialog, the task card and the status bar. Everything is drawn from a
//! `Frame` each time and every clickable thing records a `Hit`, so the click
//! handler never repeats the layout math.

use crate::tables_tab::{TablesState, TablesTab};
use crate::widgets::{
    cap, fit, text_w, text_w_bold, theme, wrap, Btn, GuiRenderer, ListView, ScrollTarget, UiAction,
    PX_BODY, PX_PAGE, PX_SMALL, PX_TITLE,
};
use beetle_render::image::ImageBuffer;
use beetle_render::skin::ColorRgba;
use bms_package_manager::{BgaStatus, PackageRecord};

/// Height of the status bar at the bottom.
const STATUS_H: f32 = 32.0;
const HEADER_H: f32 = 60.0;
const PAD: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Installed,
    Tables,
}

/// The tone of the status bar message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

/// A running background task, for the progress card.
#[derive(Debug, Clone)]
pub struct TaskProgressInfo<'a> {
    pub title: &'a str,
    pub phase: &'a str,
    pub current: usize,
    pub total: usize,
    pub detail: &'a str,
    pub frame: usize,
    pub cancelling: bool,
}

/// One text field of a dialog.
#[derive(Debug, Clone, Default)]
pub struct FieldView {
    pub label: String,
    pub value: String,
    pub placeholder: String,
    pub focused: bool,
    pub browse: bool,
}

/// A one-of-many choice in a dialog.
#[derive(Debug, Clone, Default)]
pub struct ChoiceView {
    pub label: String,
    pub options: Vec<&'static str>,
    pub selected: usize,
    /// What the selected option does, in a sentence.
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct ListRowView {
    pub text: String,
    pub ok: bool,
}

#[derive(Debug, Clone)]
pub struct ButtonView {
    pub label: String,
    pub style: Btn,
    pub action: UiAction,
}

/// Everything an open dialog shows. Built from the dialog state each frame.
#[derive(Debug, Clone, Default)]
pub struct DialogView {
    pub title: String,
    pub body: Vec<String>,
    /// Big choices with a title and a description (`DialogCard(i)`).
    pub cards: Vec<(String, String)>,
    pub fields: Vec<FieldView>,
    pub choices: Vec<ChoiceView>,
    /// Rows with a remove button (`DialogListRemove(i)`).
    pub list: Vec<ListRowView>,
    pub list_empty: String,
    pub buttons: Vec<ButtonView>,
    pub footnote: String,
    pub wide: bool,
}

/// What one frame shows.
pub struct Frame<'a> {
    pub tab: ActiveTab,
    pub packages: &'a [&'a PackageRecord],
    pub installed_total: usize,
    pub installed: &'a mut ListView,
    pub preview: Option<&'a ImageBuffer>,
    pub search: &'a str,
    pub search_active: bool,
    pub status: &'a str,
    pub status_kind: StatusKind,
    pub dialog: Option<&'a DialogView>,
    pub task: Option<TaskProgressInfo<'a>>,
    pub tables: &'a mut TablesTab,
    /// A file is dragged over the window.
    pub drop_hint: bool,
}

/// How a song's background video is stored, in plain words.
fn video_label(status: BgaStatus) -> &'static str {
    match status {
        BgaStatus::Embedded => "배경 영상 포함",
        BgaStatus::Companion => "배경 영상 별도 설치됨",
        BgaStatus::None => "배경 영상 없음",
    }
}

impl GuiRenderer {
    pub fn render(&mut self, mut f: Frame) {
        let w = self.pixmap.width() as f32;
        let h = self.pixmap.height() as f32;
        self.begin_frame(f.dialog.is_none());

        self.draw_header(w, f.tab, f.installed_total, &*f.tables);

        let top = HEADER_H + 16.0;
        let bottom = h - STATUS_H - 16.0;
        match f.tab {
            ActiveTab::Installed => self.page_installed(w, top, bottom, &mut f),
            ActiveTab::Tables => self.page_tables(w, top, bottom, &mut *f.tables),
        }

        self.draw_status(w, h, f.status, f.status_kind, f.task.is_some());

        if let Some(task) = &f.task {
            self.draw_task(w, task);
        }
        if let Some(dialog) = f.dialog {
            self.set_hover_enabled(true);
            self.draw_dialog(w, h, dialog);
        }
        if f.drop_hint {
            self.draw_drop_hint(w, h);
        }
    }

    fn draw_drop_hint(&mut self, w: f32, h: f32) {
        self.fill(0.0, 0.0, w, h, ColorRgba::new(10, 10, 14, 200));
        self.outline(24.0, 24.0, w - 48.0, h - 48.0, 18.0, theme::ACCENT, true);
        self.text_centered(
            "여기에 놓으면 곡을 추가해요",
            w / 2.0,
            h / 2.0 - 20.0,
            PX_PAGE,
            theme::ACCENT,
        );
        self.text_centered(
            "곡 폴더, 압축 파일(.zip/.rar/.7z), .bmsp 패키지를 넣을 수 있어요",
            w / 2.0,
            h / 2.0 + 18.0,
            PX_BODY,
            theme::TEXT_DIM,
        );
    }

    // ----------------------------------------------------------------- header

    fn draw_header(&mut self, w: f32, tab: ActiveTab, installed: usize, tables: &TablesTab) {
        self.fill(0.0, 0.0, w, HEADER_H, theme::SURFACE);
        self.fill(0.0, HEADER_H - 1.0, w, 1.0, theme::BORDER);

        // Logo: a small beetle-yellow mark and the name.
        self.round(PAD, 18.0, 24.0, 24.0, 7.0, theme::ACCENT);
        self.circle(PAD + 12.0, 30.0, 5.0, theme::ACCENT_TEXT);
        self.text_bold("Beetle", PAD + 34.0, 22.0, PX_TITLE, theme::TEXT);
        let name_w = text_w_bold("Beetle", PX_TITLE);
        self.text(
            "곡 관리자",
            PAD + 40.0 + name_w,
            25.0,
            PX_SMALL,
            theme::TEXT_DIM,
        );

        let installed_label = format!("내 곡  {installed}");
        let tables_label = format!("난이도표  {}", tables.tables.len());
        let tabs = [
            (ActiveTab::Installed, installed_label.as_str()),
            (ActiveTab::Tables, tables_label.as_str()),
        ];
        let mut x = 210.0;
        for (t, label) in tabs {
            let tw = text_w(label, PX_BODY) + 36.0;
            let active = t == tab;
            let hover = self.hovered(x, 10.0, tw, HEADER_H - 10.0);
            if active || hover {
                self.round(
                    x,
                    12.0,
                    tw,
                    36.0,
                    8.0,
                    if active {
                        theme::SURFACE_2
                    } else {
                        ColorRgba::new(27, 29, 39, 255)
                    },
                );
            }
            let color = if active || hover {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            };
            let ty = (12.0 + (36.0 - cap(PX_BODY)) / 2.0).round();
            if active {
                self.text_bold(label, x + 18.0, ty, PX_BODY, color);
                self.round(x + 14.0, HEADER_H - 4.0, tw - 28.0, 3.0, 1.5, theme::ACCENT);
            } else {
                self.text(label, x + 18.0, ty, PX_BODY, color);
            }
            self.hit(x, 10.0, tw, HEADER_H - 10.0, UiAction::Tab(t));
            x += tw + 6.0;
        }

        let help = "? 도움말";
        let bw = GuiRenderer::button_w(help);
        self.button(
            w - PAD - bw,
            13.0,
            bw,
            34.0,
            help,
            Btn::Ghost,
            UiAction::Help,
        );
    }

    /// Page title, its one-line explanation, and buttons on the right.
    fn page_head(
        &mut self,
        w: f32,
        y: f32,
        title: &str,
        desc: &str,
        buttons: &[(&str, Btn, UiAction)],
    ) {
        let left = self.buttons_right(w - PAD, y + 2.0, 36.0, buttons);
        self.text_bold(title, PAD, y + 2.0, PX_PAGE, theme::TEXT);
        let desc = fit(desc, left - PAD - 16.0, PX_SMALL, false);
        self.text(&desc, PAD, y + 32.0, PX_SMALL, theme::TEXT_DIM);
    }

    fn search_box(&mut self, x: f32, y: f32, w: f32, query: &str, active: bool, placeholder: &str) {
        let clear = !query.is_empty();
        let field_w = if clear { w - 44.0 } else { w };
        self.text_field(
            x,
            y,
            field_w,
            query,
            placeholder,
            active,
            UiAction::FocusSearch,
            None,
        );
        if clear {
            self.button(
                x + w - 36.0,
                y,
                36.0,
                36.0,
                "x",
                Btn::Ghost,
                UiAction::ClearSearch,
            );
        }
    }

    // ------------------------------------------------------------ "내 곡" page

    fn page_installed(&mut self, w: f32, top: f32, bottom: f32, f: &mut Frame) {
        self.page_head(
            w,
            top,
            "내 곡",
            "이 컴퓨터에 설치된 곡이에요. 여기 있는 곡은 Beetle 게임의 곡 선택 화면에 나와요.",
            &[
                ("고급 도구", Btn::Secondary, UiAction::OpenAdvanced),
                ("기존 폴더 연결", Btn::Secondary, UiAction::OpenLibrary),
                ("+ 곡 추가", Btn::Primary, UiAction::OpenAdd),
            ],
        );

        let row_y = top + 60.0;
        if f.installed_total == 0 {
            self.empty_installed(w, row_y, bottom);
            return;
        }

        let list_w = (w * 0.44).clamp(340.0, 520.0);
        self.search_box(
            PAD,
            row_y,
            list_w,
            f.search,
            f.search_active,
            "곡 제목이나 아티스트로 찾기",
        );
        let count = if f.search.is_empty() {
            format!("{}곡", f.installed_total)
        } else {
            format!("{}곡 중 {}곡", f.installed_total, f.packages.len())
        };
        self.text(
            &count,
            PAD + list_w + 16.0,
            row_y + 11.0,
            PX_SMALL,
            theme::TEXT_FAINT,
        );

        let panel_y = row_y + 48.0;
        let panel_h = bottom - panel_y;
        self.panel(PAD, panel_y, list_w, panel_h);

        // The list.
        let row_h = 56.0;
        let list_top = panel_y + 6.0;
        let list_h = panel_h - 12.0;
        let visible = (list_h / row_h).floor().max(1.0) as usize;
        f.installed.layout(f.packages.len(), visible);
        let view = *f.installed;
        self.scroll_area(PAD, panel_y, list_w, panel_h, ScrollTarget::Installed);
        if f.packages.is_empty() {
            self.text_centered(
                "찾는 곡이 없어요",
                PAD + list_w / 2.0,
                panel_y + 40.0,
                PX_BODY,
                theme::TEXT_DIM,
            );
        }
        for (slot, idx) in (view.scroll..f.packages.len()).take(visible).enumerate() {
            let pkg = f.packages[idx];
            let y = list_top + slot as f32 * row_h;
            let (x, rw) = (PAD + 6.0, list_w - 16.0);
            let selected = idx == view.selected;
            let hover = self.hovered(x, y, rw, row_h - 4.0);
            if selected {
                self.round(x, y, rw, row_h - 4.0, 8.0, theme::SELECT);
                self.round(x, y + 12.0, 3.0, row_h - 28.0, 1.5, theme::ACCENT);
            } else if hover {
                self.round(x, y, rw, row_h - 4.0, 8.0, theme::SURFACE_2);
            }
            let title = fit(&pkg.name, rw - 28.0, PX_BODY, false);
            self.text(&title, x + 14.0, y + 11.0, PX_BODY, theme::TEXT);
            let mut sub = pkg
                .author
                .clone()
                .unwrap_or_else(|| "아티스트 정보 없음".into());
            sub.push_str(" · ");
            sub.push_str(video_label(pkg.bga_status));
            if pkg.state_hashes.len() > 1 {
                sub.push_str(&format!(" · 버전 {}개", pkg.state_hashes.len()));
            }
            let sub = fit(&sub, rw - 28.0, PX_SMALL, false);
            self.text(&sub, x + 14.0, y + 32.0, PX_SMALL, theme::TEXT_FAINT);
            self.hit(x, y, rw, row_h - 4.0, UiAction::SelectInstalled(idx));
        }
        self.scrollbar(
            PAD + list_w - 8.0,
            list_top,
            list_h,
            f.packages.len(),
            visible,
            view.scroll,
        );

        // The details.
        let dx = PAD + list_w + 16.0;
        let dw = w - dx - PAD;
        self.panel(dx, panel_y, dw, panel_h);
        if let Some(pkg) = f.packages.get(view.selected) {
            self.installed_details(dx, panel_y, dw, panel_h, pkg, f.preview);
        }
    }

    fn installed_details(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        pkg: &PackageRecord,
        preview: Option<&ImageBuffer>,
    ) {
        let inner = w - 40.0;
        let mut dy = y + 20.0;

        // Artwork, when the song has a picture.
        let art_w = inner.min(320.0);
        let art_h = (art_w * 9.0 / 16.0).round();
        if let Some(img) = preview {
            self.round(x + 20.0, dy, art_w, art_h, 8.0, theme::BG);
            crate::image_draw::draw_scaled(
                img,
                &mut self.pixmap,
                (x + 20.0) as i32,
                dy as i32,
                art_w as u32,
                art_h as u32,
            );
            dy += art_h + 18.0;
        }

        for line in wrap(&pkg.name, inner, PX_TITLE).iter().take(2) {
            self.text_bold(line, x + 20.0, dy, PX_TITLE, theme::TEXT);
            dy += 26.0;
        }
        let artist = pkg.author.as_deref().unwrap_or("아티스트 정보 없음");
        self.text(
            &fit(artist, inner, PX_BODY, false),
            x + 20.0,
            dy,
            PX_BODY,
            theme::TEXT_DIM,
        );
        dy += 28.0;

        let mut cx = x + 20.0;
        let (vfg, vbg) = match pkg.bga_status {
            BgaStatus::None => (theme::TEXT_DIM, theme::SURFACE_2),
            _ => (theme::GREEN, theme::GREEN_SOFT),
        };
        cx += self.chip(video_label(pkg.bga_status), cx, dy, vfg, vbg) + 6.0;
        self.chip(
            "게임에서 플레이 가능",
            cx,
            dy,
            theme::BLUE,
            ColorRgba::new(26, 40, 64, 255),
        );
        dy += 36.0;

        // Versions: only worth a section when there is more than one.
        let states: Vec<&String> = pkg.state_hashes.keys().collect();
        if states.len() > 1 {
            self.fill(x + 20.0, dy, inner, 1.0, theme::BORDER);
            dy += 14.0;
            self.text_bold("버전", x + 20.0, dy, PX_BODY, theme::TEXT);
            self.text(
                "같은 곡의 다른 버전이 있어요. 게임에서 쓸 버전 하나를 고르세요.",
                x + 64.0,
                dy + 1.0,
                PX_SMALL,
                theme::TEXT_FAINT,
            );
            dy += 26.0;
            let room = ((y + h - 70.0 - dy) / 40.0).max(1.0) as usize;
            for (i, st) in states.iter().enumerate().take(room) {
                let active = **st == pkg.active_state;
                self.round(x + 20.0, dy, inner, 36.0, 8.0, theme::SURFACE_2);
                let short = &st[..st.len().min(8)];
                self.text(
                    &format!("버전 {}", i + 1),
                    x + 34.0,
                    dy + 11.0,
                    PX_BODY,
                    theme::TEXT,
                );
                self.text(
                    &format!("#{short}"),
                    x + 100.0,
                    dy + 12.0,
                    PX_SMALL,
                    theme::TEXT_FAINT,
                );
                if active {
                    let cw = text_w("사용 중", PX_SMALL) + 14.0;
                    self.chip(
                        "사용 중",
                        x + 20.0 + inner - cw - 10.0,
                        dy + 8.0,
                        theme::GREEN,
                        theme::GREEN_SOFT,
                    );
                } else {
                    let right = x + 20.0 + inner - 6.0;
                    self.buttons_right(
                        right,
                        dy + 4.0,
                        28.0,
                        &[
                            ("삭제", Btn::Ghost, UiAction::AskUninstallVersion(i)),
                            ("이 버전 사용", Btn::Secondary, UiAction::UseVersion(i)),
                        ],
                    );
                }
                dy += 40.0;
            }
        }

        // Actions at the bottom of the panel.
        let by = y + h - 56.0;
        self.fill(x + 20.0, by - 12.0, inner, 1.0, theme::BORDER);
        let mut buttons: Vec<(&str, Btn, UiAction)> = Vec::new();
        if pkg.bga_status == BgaStatus::Companion {
            buttons.push(("배경 영상 지우기", Btn::Secondary, UiAction::AskRemoveBga));
        }
        buttons.push(("곡 삭제", Btn::Danger, UiAction::AskUninstall));
        self.buttons_right(x + w - 20.0, by, 36.0, &buttons);
    }

    fn empty_installed(&mut self, w: f32, y: f32, bottom: f32) {
        let (x, zw, zh) = (PAD, w - PAD * 2.0, bottom - y);
        self.round(x, y, zw, zh, 14.0, theme::SURFACE);
        self.outline(x, y, zw, zh, 14.0, theme::SURFACE_3, true);
        let cx = x + zw / 2.0;
        let mut cy = y + (zh / 2.0 - 120.0).max(24.0);

        // A folder drawn from two boxes.
        self.round(cx - 34.0, cy, 30.0, 14.0, 4.0, theme::ACCENT_SOFT);
        self.round(cx - 34.0, cy + 8.0, 68.0, 46.0, 7.0, theme::ACCENT_SOFT);
        self.round(cx - 2.0, cy + 20.0, 4.0, 22.0, 2.0, theme::ACCENT);
        self.round(cx - 11.0, cy + 29.0, 22.0, 4.0, 2.0, theme::ACCENT);
        cy += 74.0;

        self.text_centered("아직 설치된 곡이 없어요", cx, cy, PX_TITLE, theme::TEXT);
        cy += 32.0;
        for line in [
            "BMS 곡 폴더나 .bmsp 파일을 이 창에 끌어다 놓으면 바로 추가돼요.",
            "인터넷에서 곡을 받고 싶다면 '난이도표' 탭에서 찾아 받을 수 있어요.",
        ] {
            self.text_centered(line, cx, cy, PX_BODY, theme::TEXT_DIM);
            cy += 24.0;
        }
        cy += 14.0;
        let a = "+ 곡 추가";
        let b = "난이도표로 가기";
        let (aw, bw) = (GuiRenderer::button_w(a) + 16.0, GuiRenderer::button_w(b));
        let bx = cx - (aw + bw + 10.0) / 2.0;
        self.button(bx, cy, aw, 40.0, a, Btn::Primary, UiAction::OpenAdd);
        self.button(
            bx + aw + 10.0,
            cy,
            bw,
            40.0,
            b,
            Btn::Secondary,
            UiAction::Tab(ActiveTab::Tables),
        );
        cy += 72.0;

        let note = "BMS는 리듬게임용 곡 형식이에요. 곡 하나는 보통 음악, 효과음(키음), 채보 파일(.bms/.bme/.bml)이 든 폴더 하나예요.";
        for line in wrap(note, (zw - 80.0).min(620.0), PX_SMALL) {
            self.text_centered(&line, cx, cy, PX_SMALL, theme::TEXT_FAINT);
            cy += 20.0;
        }
    }

    // --------------------------------------------------------- "바로 설치" page

    // -------------------------------------------------------- "난이도표" page

    fn page_tables(&mut self, w: f32, top: f32, bottom: f32, t: &mut TablesTab) {
        self.page_head(
            w,
            top,
            "난이도표",
            "커뮤니티가 난이도별로 모은 곡 목록이에요. 이 표에 있지만 아직 내 곡에 없는 곡을 보여 줘요.",
            &[
                ("+ 난이도표 추가", Btn::Secondary, UiAction::AskAddTable),
                ("내 곡 다시 확인", Btn::Secondary, UiAction::ScanCollection),
            ],
        );
        let row_y = top + 60.0;
        match t.state() {
            TablesState::NoTables => {
                self.empty_state(
                    w,
                    row_y,
                    bottom,
                    "추가한 난이도표가 없어요",
                    &[
                        "난이도표 웹페이지 주소를 넣으면 표를 받아 와요.",
                        "그러면 그 표의 곡 중 아직 없는 곡과 받는 곳을 여기서 볼 수 있어요.",
                    ],
                    ("+ 난이도표 추가", UiAction::AskAddTable),
                );
                return;
            }
            TablesState::NoIndex => {
                self.empty_state(
                    w,
                    row_y,
                    bottom,
                    "먼저 내 곡을 확인해야 해요",
                    &[
                        "어떤 곡이 없는지 알려면 갖고 있는 곡을 한 번 훑어봐야 해요.",
                        "곡이 많으면 조금 걸릴 수 있어요.",
                    ],
                    ("지금 확인하기", UiAction::ScanCollection),
                );
                return;
            }
            TablesState::Ready => {}
        }

        // Table picker: < name >
        let name = t
            .table()
            .map(|table| table.name.clone())
            .unwrap_or_default();
        self.button(
            PAD,
            row_y,
            36.0,
            36.0,
            "<",
            Btn::Secondary,
            UiAction::PrevTable,
        );
        let name_w = (text_w_bold(&name, PX_BODY) + 32.0).clamp(160.0, 340.0);
        self.round(PAD + 42.0, row_y, name_w, 36.0, 7.0, theme::SURFACE_2);
        let shown = fit(&name, name_w - 24.0, PX_BODY, true);
        let sw = text_w_bold(&shown, PX_BODY);
        self.text_bold(
            &shown,
            PAD + 42.0 + (name_w - sw) / 2.0,
            row_y + 11.0,
            PX_BODY,
            theme::TEXT,
        );
        self.button(
            PAD + 48.0 + name_w,
            row_y,
            36.0,
            36.0,
            ">",
            Btn::Secondary,
            UiAction::NextTable,
        );
        let mut info_x = PAD + 96.0 + name_w;
        if t.tables.len() > 1 {
            let pos = format!("{} / {}", t.table_idx + 1, t.tables.len());
            self.text(&pos, info_x, row_y + 12.0, PX_SMALL, theme::TEXT_FAINT);
            info_x += text_w(&pos, PX_SMALL) + 16.0;
        }
        let total = t.table().map_or(0, |table| table.entries.len());
        let summary = format!("{total}곡 중 {}곡이 아직 없어요", t.rows.len());
        self.text(&summary, info_x, row_y + 12.0, PX_SMALL, theme::TEXT_DIM);
        if t.stale {
            let note = "곡을 추가했어요. '내 곡 다시 확인'을 누르면 목록이 맞춰져요";
            let nw = text_w(note, PX_SMALL) + 14.0;
            if w - PAD - nw > info_x + text_w(&summary, PX_SMALL) + 16.0 {
                self.chip(
                    note,
                    w - PAD - nw,
                    row_y + 8.0,
                    theme::WARN,
                    theme::WARN_SOFT,
                );
            }
        }

        let detail_h = 132.0;
        let panel_y = row_y + 48.0;
        let panel_h = bottom - panel_y - detail_h - 12.0;
        let list_w = w - PAD * 2.0;
        self.panel(PAD, panel_y, list_w, panel_h);
        if t.rows.is_empty() {
            self.text_centered(
                "이 난이도표의 곡을 모두 갖고 있어요!",
                PAD + list_w / 2.0,
                panel_y + panel_h / 2.0 - 8.0,
                PX_BODY,
                theme::GREEN,
            );
        }
        let row_h = 32.0;
        let list_top = panel_y + 6.0;
        let list_h = panel_h - 12.0;
        let visible = (list_h / row_h).floor().max(1.0) as usize;
        t.view.layout(t.rows.len(), visible);
        self.scroll_area(PAD, panel_y, list_w, panel_h, ScrollTarget::Tables);
        let symbol = t
            .table()
            .map(|table| table.symbol.clone())
            .unwrap_or_default();
        for (slot, idx) in (t.view.scroll..t.rows.len()).take(visible).enumerate() {
            let row = &t.rows[idx];
            let y = list_top + slot as f32 * row_h;
            let (x, rw) = (PAD + 6.0, list_w - 16.0);
            let selected = idx == t.view.selected;
            let hover = self.hovered(x, y, rw, row_h - 2.0);
            if selected {
                self.round(x, y, rw, row_h - 2.0, 6.0, theme::SELECT);
            } else if hover {
                self.round(x, y, rw, row_h - 2.0, 6.0, theme::SURFACE_2);
            }
            let level = format!("{symbol}{}", row.level);
            let lw = (text_w(&level, PX_SMALL) + 14.0).max(56.0);
            self.round(x + 8.0, y + 5.0, lw, 20.0, 10.0, theme::ACCENT_SOFT);
            let lt = text_w(&level, PX_SMALL);
            self.text(
                &level,
                x + 8.0 + (lw - lt) / 2.0,
                y + 5.0 + ((20.0 - cap(PX_SMALL)) / 2.0).round(),
                PX_SMALL,
                theme::ACCENT,
            );
            let tx = x + lw + 20.0;
            let mut right = x + rw - 10.0;
            if row.body_needed {
                let tag = "곡 파일 필요";
                let cw = text_w(tag, PX_SMALL) + 14.0;
                right -= cw;
                self.chip(tag, right, y + 5.0, theme::WARN, theme::WARN_SOFT);
                right -= 10.0;
            }
            let artist_w = ((right - tx) * 0.35).min(260.0);
            let artist = fit(&row.artist, artist_w, PX_SMALL, false);
            self.text(
                &artist,
                right - text_w(&artist, PX_SMALL),
                y + ((row_h - 2.0 - cap(PX_SMALL)) / 2.0).round(),
                PX_SMALL,
                theme::TEXT_FAINT,
            );
            let title = fit(&row.title, right - artist_w - 16.0 - tx, PX_BODY, false);
            self.text(
                &title,
                tx,
                y + ((row_h - 2.0 - cap(PX_BODY)) / 2.0).round(),
                PX_BODY,
                theme::TEXT,
            );
            self.hit(x, y, rw, row_h - 2.0, UiAction::SelectTableRow(idx));
        }
        self.scrollbar(
            PAD + list_w - 8.0,
            list_top,
            list_h,
            t.rows.len(),
            visible,
            t.view.scroll,
        );

        // The selected entry and what can be done with it.
        let dy = bottom - detail_h;
        self.panel(PAD, dy, list_w, detail_h);
        let Some(row) = t.selected_row() else {
            return;
        };
        let inner = list_w - 40.0;
        let title = fit(&row.title, inner * 0.6, PX_TITLE, true);
        self.text_bold(&title, PAD + 20.0, dy + 18.0, PX_TITLE, theme::TEXT);
        let artist = fit(&row.artist, inner * 0.6, PX_SMALL, false);
        self.text(&artist, PAD + 20.0, dy + 46.0, PX_SMALL, theme::TEXT_DIM);

        let direct = !row.url_diff.is_empty()
            && bms_package_manager::table_fetch::is_direct_pack(&row.url_diff);
        let mut steps: Vec<(&str, Btn, UiAction)> = Vec::new();
        if !row.url.is_empty() {
            steps.push((
                "곡 파일 받으러 가기",
                Btn::Secondary,
                UiAction::OpenSongPage,
            ));
        }
        steps.push((
            "받은 곡 파일 추가...",
            Btn::Secondary,
            UiAction::AskAddFromArchive,
        ));
        // The IR lookup needs only the MD5, so it is offered whenever the table has one.
        let has_md5 = t.selected_entry().is_some_and(|e| e.md5.is_some());
        if direct {
            steps.push(("채보 받기", Btn::Primary, UiAction::AskDownloadChart));
        } else {
            if has_md5 {
                steps.push(("IR에서 채보 찾기", Btn::Primary, UiAction::AskIrChart));
            }
            if !row.url_diff.is_empty() {
                steps.push(("채보 페이지 열기", Btn::Secondary, UiAction::OpenChartPage));
            }
        }
        // Number the buttons when they are steps to follow in order.
        let labels: Vec<String> = if steps.len() > 1 {
            steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {}", i + 1, s.0))
                .collect()
        } else {
            steps.iter().map(|s| s.0.to_string()).collect()
        };
        let buttons: Vec<(&str, Btn, UiAction)> = steps
            .iter()
            .zip(&labels)
            .map(|(s, label)| (label.as_str(), s.1, s.2))
            .collect();
        self.buttons_right(PAD + list_w - 20.0, dy + 72.0, 36.0, &buttons);
        let tip = if row.body_needed {
            "이 곡은 채보는 있지만 소리 파일이 모자라요. 곡 파일(본체)을 받아 추가해 주세요."
        } else if steps.len() == 1 {
            "난이도표에 이 곡을 받을 주소가 없어요. 곡 파일을 따로 구했다면 '받은 곡 파일 추가'로 넣을 수 있어요."
        } else {
            "BMS 곡은 '곡 파일(본체)'과 '채보(차분)'가 따로 배포되기도 해요. 왼쪽 버튼부터 순서대로 하면 돼요."
        };
        let tip_max = list_w
            - 40.0
            - (buttons
                .iter()
                .map(|b| GuiRenderer::button_w(b.0) + 8.0)
                .sum::<f32>())
            - 16.0;
        for (i, line) in wrap(tip, tip_max.max(120.0), PX_SMALL)
            .iter()
            .take(2)
            .enumerate()
        {
            self.text(
                line,
                PAD + 20.0,
                dy + 76.0 + i as f32 * 18.0,
                PX_SMALL,
                theme::TEXT_FAINT,
            );
        }
    }

    /// A centered message with one action, in place of an empty list.
    fn empty_state(
        &mut self,
        w: f32,
        y: f32,
        bottom: f32,
        title: &str,
        lines: &[&str],
        action: (&str, UiAction),
    ) {
        let (x, zw, zh) = (PAD, w - PAD * 2.0, bottom - y);
        self.panel(x, y, zw, zh);
        let cx = x + zw / 2.0;
        let mut cy = y + (zh / 2.0 - 70.0).max(24.0);
        self.text_centered(title, cx, cy, PX_TITLE, theme::TEXT);
        cy += 34.0;
        for line in lines {
            self.text_centered(line, cx, cy, PX_BODY, theme::TEXT_DIM);
            cy += 24.0;
        }
        cy += 16.0;
        let bw = GuiRenderer::button_w(action.0) + 16.0;
        self.button(
            cx - bw / 2.0,
            cy,
            bw,
            40.0,
            action.0,
            Btn::Primary,
            action.1,
        );
    }

    fn text_centered(&mut self, text: &str, cx: f32, y: f32, px: u16, color: ColorRgba) {
        let tw = text_w(text, px);
        self.text(text, (cx - tw / 2.0).round(), y, px, color);
    }

    // ---------------------------------------------------- status, task, dialog

    fn draw_status(&mut self, w: f32, h: f32, msg: &str, kind: StatusKind, busy: bool) {
        let y = h - STATUS_H;
        self.fill(0.0, y, w, STATUS_H, theme::SURFACE);
        self.fill(0.0, y, w, 1.0, theme::BORDER);
        let color = match kind {
            StatusKind::Info => theme::TEXT_FAINT,
            StatusKind::Success => theme::GREEN,
            StatusKind::Error => theme::DANGER,
        };
        let hint = if busy {
            ""
        } else {
            "곡 폴더나 파일을 창에 끌어다 놓아도 추가돼요"
        };
        let hint_w = if hint.is_empty() {
            0.0
        } else {
            text_w(hint, PX_SMALL) + 24.0
        };
        self.circle(PAD + 4.0, y + STATUS_H / 2.0, 4.0, color);
        let text_color = if kind == StatusKind::Info {
            theme::TEXT_DIM
        } else {
            color
        };
        self.text_in(
            msg,
            PAD + 16.0,
            y,
            STATUS_H,
            w - PAD * 2.0 - 16.0 - hint_w,
            PX_SMALL,
            text_color,
        );
        if !hint.is_empty() && w > 720.0 {
            self.text_in(
                hint,
                w - PAD - hint_w + 24.0,
                y,
                STATUS_H,
                hint_w,
                PX_SMALL,
                theme::TEXT_FAINT,
            );
        }
    }

    fn draw_task(&mut self, w: f32, task: &TaskProgressInfo) {
        let cw = (w - 40.0).min(560.0);
        let ch = 108.0;
        let x = ((w - cw) / 2.0).round();
        let y = HEADER_H + 10.0;
        self.round(
            x - 1.0,
            y - 1.0,
            cw + 2.0,
            ch + 2.0,
            12.0,
            ColorRgba::new(0, 0, 0, 120),
        );
        self.round(x, y, cw, ch, 12.0, theme::SURFACE_2);
        self.outline(x, y, cw, ch, 12.0, theme::SURFACE_3, false);

        let cancel = if task.cancelling {
            "취소하는 중..."
        } else {
            "취소"
        };
        let bw = GuiRenderer::button_w(cancel);
        self.button(
            x + cw - bw - 16.0,
            y + 14.0,
            bw,
            32.0,
            cancel,
            Btn::Secondary,
            UiAction::CancelTask,
        );
        let title = fit(task.title, cw - bw - 52.0, PX_BODY, true);
        self.text_bold(&title, x + 20.0, y + 18.0, PX_BODY, theme::TEXT);
        let phase = if task.phase.is_empty() {
            "작업 중..."
        } else {
            task.phase
        };
        self.text(
            &fit(phase, cw - bw - 52.0, PX_SMALL, false),
            x + 20.0,
            y + 40.0,
            PX_SMALL,
            theme::TEXT_DIM,
        );

        // Progress bar: filled when the total is known, a moving band otherwise.
        let (bx, by, bw2, bh) = (x + 20.0, y + 64.0, cw - 40.0, 8.0);
        self.round(bx, by, bw2, bh, 4.0, theme::BG);
        if task.total > 0 {
            let ratio = (task.current as f32 / task.total as f32).clamp(0.0, 1.0);
            if ratio > 0.0 {
                self.round(bx, by, (bw2 * ratio).max(8.0), bh, 4.0, theme::ACCENT);
            }
        } else {
            let band = bw2 * 0.28;
            let span = bw2 - band;
            let t = (task.frame % 40) as f32 / 40.0;
            let pos = if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 };
            self.round(bx + span * pos, by, band, bh, 4.0, theme::ACCENT);
        }
        let detail = if task.total > 0 && task.detail.is_empty() {
            format!("{} / {}", task.current, task.total)
        } else {
            task.detail.to_string()
        };
        self.text(
            &fit(&detail, cw - 40.0, PX_SMALL, false),
            x + 20.0,
            y + 84.0,
            PX_SMALL,
            theme::TEXT_FAINT,
        );
    }

    fn draw_dialog(&mut self, w: f32, h: f32, d: &DialogView) {
        self.fill(0.0, 0.0, w, h, theme::SCRIM);
        self.hit(0.0, 0.0, w, h, UiAction::None);

        let dw = (if d.wide { 640.0_f32 } else { 540.0 }).min(w - 40.0);
        let inner = dw - 56.0;

        // Measure first, so the box fits its content.
        // Each body entry is a paragraph: wrapped lines, then a small gap.
        let body: Vec<Vec<String>> = d
            .body
            .iter()
            .map(|line| wrap(line, inner, PX_BODY))
            .collect();
        let body_lines: usize = body.iter().map(Vec::len).sum();
        let mut dh = 28.0 + 34.0 + body_lines as f32 * 23.0 + body.len() as f32 * 6.0;
        if !body.is_empty() {
            dh += 4.0;
        }
        dh += d.cards.len() as f32 * 76.0;
        for field in &d.fields {
            dh += if field.label.is_empty() { 0.0 } else { 24.0 } + 48.0;
        }
        for choice in &d.choices {
            dh += 24.0 + 42.0 + if choice.note.is_empty() { 0.0 } else { 22.0 } + 8.0;
        }
        let list_rows = d.list.len().min(6);
        if !d.list.is_empty() || !d.list_empty.is_empty() {
            dh += list_rows.max(1) as f32 * 40.0 + 10.0;
        }
        let foot: Vec<String> = if d.footnote.is_empty() {
            Vec::new()
        } else {
            wrap(&d.footnote, inner, PX_SMALL)
        };
        dh += foot.len() as f32 * 19.0 + if foot.is_empty() { 0.0 } else { 6.0 };
        dh += 8.0 + 40.0 + 24.0;
        let dh = dh.min(h - 40.0);

        let x = ((w - dw) / 2.0).round();
        let y = ((h - dh) / 2.0).round().max(20.0);
        self.round(
            x - 2.0,
            y - 2.0,
            dw + 4.0,
            dh + 4.0,
            16.0,
            ColorRgba::new(0, 0, 0, 140),
        );
        self.round(x, y, dw, dh, 14.0, theme::SURFACE_2);
        self.outline(x, y, dw, dh, 14.0, theme::SURFACE_3, false);
        self.hit(x, y, dw, dh, UiAction::None);

        let lx = x + 28.0;
        let mut cy = y + 28.0;
        self.text_bold(
            &fit(&d.title, inner, PX_TITLE, true),
            lx,
            cy,
            PX_TITLE,
            theme::TEXT,
        );
        cy += 34.0;
        for paragraph in &body {
            for line in paragraph {
                self.text(line, lx, cy, PX_BODY, theme::TEXT_DIM);
                cy += 23.0;
            }
            cy += 6.0;
        }
        if !body.is_empty() {
            cy += 4.0;
        }

        for (i, (title, desc)) in d.cards.iter().enumerate() {
            let hover = self.hovered(lx, cy, inner, 66.0);
            self.round(
                lx,
                cy,
                inner,
                66.0,
                10.0,
                if hover {
                    theme::SURFACE_3
                } else {
                    theme::SURFACE
                },
            );
            self.outline(
                lx,
                cy,
                inner,
                66.0,
                10.0,
                if hover { theme::ACCENT } else { theme::BORDER },
                false,
            );
            self.text_bold(
                &fit(title, inner - 60.0, PX_BODY, true),
                lx + 18.0,
                cy + 16.0,
                PX_BODY,
                theme::TEXT,
            );
            self.text(
                &fit(desc, inner - 60.0, PX_SMALL, false),
                lx + 18.0,
                cy + 40.0,
                PX_SMALL,
                theme::TEXT_DIM,
            );
            self.text_bold(
                ">",
                lx + inner - 28.0,
                cy + 26.0,
                PX_BODY,
                if hover {
                    theme::ACCENT
                } else {
                    theme::TEXT_FAINT
                },
            );
            self.hit(lx, cy, inner, 66.0, UiAction::DialogCard(i));
            cy += 76.0;
        }

        for (i, field) in d.fields.iter().enumerate() {
            if !field.label.is_empty() {
                self.text(&field.label, lx, cy, PX_SMALL, theme::TEXT_DIM);
                cy += 24.0;
            }
            self.text_field(
                lx,
                cy,
                inner,
                &field.value,
                &field.placeholder,
                field.focused,
                UiAction::DialogFocus(i),
                field.browse.then_some(UiAction::DialogBrowse(i)),
            );
            cy += 48.0;
        }

        for (gi, choice) in d.choices.iter().enumerate() {
            self.text(&choice.label, lx, cy, PX_SMALL, theme::TEXT_DIM);
            cy += 24.0;
            let seg_w = inner / choice.options.len() as f32;
            self.round(lx, cy, inner, 36.0, 8.0, theme::SURFACE);
            for (oi, option) in choice.options.iter().enumerate() {
                let ox = lx + oi as f32 * seg_w;
                let active = oi == choice.selected;
                let hover = self.hovered(ox + 3.0, cy + 3.0, seg_w - 6.0, 30.0);
                if active {
                    self.round(ox + 3.0, cy + 3.0, seg_w - 6.0, 30.0, 6.0, theme::ACCENT);
                } else if hover {
                    self.round(ox + 3.0, cy + 3.0, seg_w - 6.0, 30.0, 6.0, theme::SURFACE_3);
                }
                let label = fit(option, seg_w - 14.0, PX_SMALL, active);
                let tw = if active {
                    text_w_bold(&label, PX_SMALL)
                } else {
                    text_w(&label, PX_SMALL)
                };
                let tx = (ox + (seg_w - tw) / 2.0).round();
                let ty = cy + ((36.0 - cap(PX_SMALL)) / 2.0).round();
                if active {
                    self.text_bold(&label, tx, ty, PX_SMALL, theme::ACCENT_TEXT);
                } else {
                    self.text(&label, tx, ty, PX_SMALL, theme::TEXT_DIM);
                }
                self.hit(ox, cy, seg_w, 36.0, UiAction::DialogChoice(gi, oi));
            }
            cy += 42.0;
            if !choice.note.is_empty() {
                self.text(
                    &fit(&choice.note, inner, PX_SMALL, false),
                    lx,
                    cy,
                    PX_SMALL,
                    theme::TEXT_FAINT,
                );
                cy += 22.0;
            }
            cy += 8.0;
        }

        if !d.list.is_empty() || !d.list_empty.is_empty() {
            if d.list.is_empty() {
                self.round(lx, cy, inner, 36.0, 8.0, theme::SURFACE);
                self.text_in(
                    &d.list_empty,
                    lx + 14.0,
                    cy,
                    36.0,
                    inner - 28.0,
                    PX_SMALL,
                    theme::TEXT_FAINT,
                );
                cy += 40.0;
            }
            for (i, row) in d.list.iter().enumerate().take(6) {
                self.round(lx, cy, inner, 36.0, 8.0, theme::SURFACE);
                let (tag, fg, bg) = if row.ok {
                    ("연결됨", theme::GREEN, theme::GREEN_SOFT)
                } else {
                    ("폴더 없음", theme::DANGER, theme::DANGER_SOFT)
                };
                let tag_w = self.chip(tag, lx + 10.0, cy + 8.0, fg, bg);
                let bw = GuiRenderer::button_w("빼기");
                self.text_in(
                    &row.text,
                    lx + 20.0 + tag_w,
                    cy,
                    36.0,
                    inner - tag_w - bw - 40.0,
                    PX_SMALL,
                    theme::TEXT,
                );
                self.button(
                    lx + inner - bw - 4.0,
                    cy + 4.0,
                    bw,
                    28.0,
                    "빼기",
                    Btn::Ghost,
                    UiAction::DialogListRemove(i),
                );
                cy += 40.0;
            }
            cy += 10.0;
        }

        for line in &foot {
            self.text(line, lx, cy, PX_SMALL, theme::TEXT_FAINT);
            cy += 19.0;
        }

        let items: Vec<(&str, Btn, UiAction)> = d
            .buttons
            .iter()
            .map(|b| (b.label.as_str(), b.style, b.action))
            .collect();
        self.buttons_right(x + dw - 28.0, y + dh - 64.0, 40.0, &items);
    }
}
