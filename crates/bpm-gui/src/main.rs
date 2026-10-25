#![windows_subsystem = "windows"]

mod bitmap_font;
mod clipboard;
mod dialogs;
mod image_draw;
mod josa;
mod tables_tab;
mod tasks;
mod ui;
mod widgets;

#[cfg(test)]
mod snapshot;

use beetle_render::image::ImageBuffer;
use bms_package_manager::{BgaPackMode, PackageManager, PackageRecord};
use dialogs::{Dialog, DialogKind};
use softbuffer::{Context, Surface};
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tasks::{BgTask, TaskKind, TaskMessage};
use ui::{ActiveTab, StatusKind};
use widgets::{GuiRenderer, ListView, ScrollTarget, UiAction};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Where a path picked in the Open dialog goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickTarget {
    /// Into a text field of the open dialog.
    Field(usize),
    /// Straight into "add songs".
    AddPath,
}

struct AppState {
    window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: GuiRenderer,
    /// Window scale factor (1.0 when the display is not scaled). The UI is drawn at
    /// `physical / ui_scale` pixels and enlarged on present.
    ui_scale: f32,
    manager: PackageManager,
    packages: Vec<PackageRecord>,
    filtered_indices: Vec<usize>,
    installed: ListView,
    search_query: String,
    is_search_active: bool,
    /// Whether the window currently accepts IME input (only while searching).
    ime_allowed: bool,
    status: String,
    status_kind: StatusKind,
    dialog: Option<Dialog>,
    preview_image: Option<ImageBuffer>,
    /// Which package state the preview image belongs to.
    preview_key: Option<(String, String)>,
    task: Option<BgTask>,
    /// The IR lookup that is running, and where its links go when it ends.
    ir_pending: Option<IrPending>,
    /// The entry the open IR download dialog is for.
    ir_entry: Option<beetle_core::TableEntry>,
    picker: Option<(PickTarget, Receiver<Option<PathBuf>>)>,
    modifiers: ModifiersState,
    anim_frame: usize,
    last_anim_time: Instant,
    active_tab: ActiveTab,
    tables: tables_tab::TablesTab,
    cursor_pos: (f32, f32),
    hover: Option<UiAction>,
    /// A file is being dragged over the window.
    drag_hover: bool,
    library: Vec<String>,
}

impl AppState {
    fn info(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_kind = StatusKind::Info;
    }

    fn ok(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_kind = StatusKind::Success;
    }

    fn err(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_kind = StatusKind::Error;
    }

    /// True (and says so) when a task is running, for actions that must wait.
    fn busy(&mut self) -> bool {
        if self.task.is_some() {
            self.err("지금 하는 작업이 끝난 뒤에 다시 해 주세요");
            true
        } else {
            false
        }
    }

    fn refresh_packages(&mut self) {
        let root = self.manager.root_dir().to_path_buf();
        if let Ok(new_mgr) = PackageManager::new(&root) {
            self.manager = new_mgr;
        }
        self.packages = self
            .manager
            .registry()
            .list_packages()
            .into_iter()
            .cloned()
            .collect();
        self.packages.sort_by_key(|p| p.name.to_lowercase());
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.trim().to_lowercase();
        self.filtered_indices = self
            .packages
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                q.is_empty()
                    || p.id.to_lowercase().contains(&q)
                    || p.name.to_lowercase().contains(&q)
                    || p.author
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        self.installed.follow = true;
    }

    fn selected_package(&self) -> Option<&PackageRecord> {
        self.filtered_indices
            .get(self.installed.selected)
            .and_then(|&i| self.packages.get(i))
    }

    /// Loads the artwork of the selected package when the selection changed.
    fn update_preview_image(&mut self) {
        let key = self
            .selected_package()
            .map(|p| (p.id.clone(), p.active_state.clone()));
        if key == self.preview_key {
            return;
        }
        self.preview_image = self.selected_package().and_then(|pkg| {
            let state = pkg.state_hashes.get(&pkg.active_state)?;
            load_artwork_from_dir(&self.manager.root_dir().join(&state.path))
        });
        self.preview_key = key;
    }

    fn hwnd(&self) -> isize {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match self.window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        }
    }

    fn open_picker(
        &mut self,
        target: PickTarget,
        kind: bms_package_manager::file_dialog::PickKind,
        title: &str,
    ) {
        if self.picker.is_some() {
            return;
        }
        let rx = bms_package_manager::file_dialog::pick(kind, title, self.hwnd());
        self.picker = Some((target, rx));
    }

    fn open_dialog(&mut self, kind: DialogKind) {
        self.is_search_active = false;
        self.dialog = Some(Dialog::new(kind));
    }

    fn start_task<F>(&mut self, title: String, phase: &str, kind: TaskKind, work: F)
    where
        F: FnOnce(&tasks::Reporter) -> Result<String, String> + Send + 'static,
    {
        if self.busy() {
            return;
        }
        self.info(title.clone());
        self.task = Some(tasks::spawn(title, phase, kind, work));
    }

    /// Adds a folder, an archive, a package or an update file.
    fn start_add(&mut self, path: PathBuf) {
        match tasks::add_title(&path) {
            Ok(title) => {
                let root = self.manager.root_dir().to_path_buf();
                self.start_task(title, "준비하는 중...", TaskKind::Adds, move |r| {
                    tasks::add_path(root, path, r)
                });
            }
            Err(msg) => self.err(msg),
        }
    }
}

fn load_artwork_from_dir(dir: &Path) -> Option<ImageBuffer> {
    if !dir.exists() {
        return None;
    }
    for name in [
        "stagefile.bmp",
        "stage.bmp",
        "banner.bmp",
        "title.bmp",
        "cover.bmp",
        "STAGEFILE.BMP",
        "STAGE.BMP",
        "BANNER.BMP",
        "TITLE.BMP",
    ] {
        if let Some(img) = ImageBuffer::load_from_file(dir.join(name)) {
            return Some(img);
        }
    }
    let img_dir = dir.join("image");
    for entry in fs::read_dir(&img_dir).into_iter().flatten().flatten() {
        let p = entry.path();
        let is_bmp = p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("bmp"));
        if is_bmp {
            if let Some(img) = ImageBuffer::load_from_file(&p) {
                return Some(img);
            }
        }
    }
    None
}

struct BpmGuiApp {
    state: Option<AppState>,
}

/// The icon group `build.rs` links into the exe (the game's icon).
#[cfg(windows)]
const APP_ICON_ID: u16 = 1;

/// The exe's own icon on the title bar and the taskbar. A missing resource
/// leaves the default icon.
fn with_app_icon(attributes: WindowAttributes) -> WindowAttributes {
    #[cfg(windows)]
    {
        use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
        if let Ok(icon) = winit::window::Icon::from_resource(APP_ICON_ID, None) {
            return attributes
                .with_window_icon(Some(icon.clone()))
                .with_taskbar_icon(Some(icon));
        }
    }
    attributes
}

impl ApplicationHandler for BpmGuiApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("Beetle 고급 도구")
            .with_inner_size(LogicalSize::new(1080.0, 740.0))
            .with_min_inner_size(LogicalSize::new(900.0, 620.0));
        let window_attributes = with_app_icon(window_attributes);

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("Failed to create window: {e}");
                return;
            }
        };
        let context = match Context::new(window.clone()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to create softbuffer context: {e}");
                return;
            }
        };
        let surface = match Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create softbuffer surface: {e}");
                return;
            }
        };

        let packages_dir = bms_package::installed::packages_root();
        let manager =
            PackageManager::new(&packages_dir).expect("Failed to initialize PackageManager");
        let size = window.inner_size();
        let ui_scale = ui_scale_of(window.scale_factor());
        let (lw, lh) = logical_size(size.width, size.height, ui_scale);
        let renderer = GuiRenderer::new(lw, lh).expect("Failed to create GuiRenderer");

        let mut app_state = AppState {
            window,
            _context: context,
            surface,
            renderer,
            ui_scale,
            manager,
            packages: Vec::new(),
            filtered_indices: Vec::new(),
            installed: ListView::default(),
            search_query: String::new(),
            is_search_active: false,
            ime_allowed: false,
            status: "준비됐어요. 처음이라면 오른쪽 위 '도움말'을 눌러 보세요.".to_string(),
            status_kind: StatusKind::Info,
            dialog: None,
            preview_image: None,
            preview_key: None,
            task: None,
            ir_pending: None,
            ir_entry: None,
            picker: None,
            modifiers: ModifiersState::default(),
            anim_frame: 0,
            last_anim_time: Instant::now(),
            active_tab: ActiveTab::Installed,
            tables: tables_tab::TablesTab::load(),
            cursor_pos: (-1.0, -1.0),
            hover: None,
            drag_hover: false,
            library: bms_package_manager::load_library().paths().to_vec(),
        };
        app_state.refresh_packages();
        self.state = Some(app_state);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };

        poll_task(state);
        poll_picker(state);

        if state.task.is_some() {
            let now = Instant::now();
            if now.duration_since(state.last_anim_time) >= Duration::from_millis(33) {
                state.anim_frame = state.anim_frame.wrapping_add(1);
                state.last_anim_time = now;
                state.window.request_redraw();
            }
        }
        if state.task.is_some() || state.picker.is_some() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(33),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                if let Some(task) = &state.task {
                    task.cancel.store(true, Ordering::SeqCst);
                }
                event_loop.exit();
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                state.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(new_size) => {
                if let (Some(w), Some(h)) = (
                    NonZeroU32::new(new_size.width),
                    NonZeroU32::new(new_size.height),
                ) {
                    state.surface.resize(w, h).ok();
                    let (lw, lh) = logical_size(new_size.width, new_size.height, state.ui_scale);
                    state.renderer.resize(lw, lh);
                    state.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                state.ui_scale = ui_scale_of(scale_factor);
                let size = state.window.inner_size();
                let (lw, lh) = logical_size(size.width, size.height, state.ui_scale);
                state.renderer.resize(lw, lh);
                state.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor_pos = (
                    position.x as f32 / state.ui_scale,
                    position.y as f32 / state.ui_scale,
                );
                state.renderer.cursor = state.cursor_pos;
                let hover = state.renderer.hit_at(state.cursor_pos);
                if hover != state.hover {
                    state.hover = hover;
                    state.window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                // Only the highlight goes: a click always comes with a fresh position.
                state.renderer.cursor = (-1.0, -1.0);
                state.hover = None;
                state.window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if state.picker.is_none() {
                    match state.renderer.hit_at(state.cursor_pos) {
                        Some(action) => {
                            if action != UiAction::FocusSearch {
                                state.is_search_active = false;
                            }
                            dispatch(state, action);
                        }
                        None => state.is_search_active = false,
                    }
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                let rows = (-lines * 3.0).round() as isize;
                if rows != 0 && state.dialog.is_none() {
                    match state.renderer.scroll_target_at(state.cursor_pos) {
                        Some(ScrollTarget::Installed) => state.installed.scroll_by(rows),
                        Some(ScrollTarget::Tables) => state.tables.view.scroll_by(rows),
                        None => {}
                    }
                    state.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: ElementState::Pressed,
                        text,
                        ..
                    },
                ..
            } => {
                if state.picker.is_none() {
                    handle_key_input(state, code, text.as_deref());
                    state.window.request_redraw();
                }
            }
            WindowEvent::HoveredFile(_) => {
                state.drag_hover = true;
                state.window.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                state.drag_hover = false;
                state.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                state.drag_hover = false;
                handle_drop(state, path);
                state.window.request_redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) if state.is_search_active => {
                let before = state.search_query.len();
                state
                    .search_query
                    .extend(text.chars().filter(|c| !c.is_control()));
                if state.search_query.len() != before {
                    state.apply_filter();
                }
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => redraw(state),
            _ => (),
        }
        // The search box is the only text field: IME input is on only while it is focused.
        if state.ime_allowed != state.is_search_active {
            state.ime_allowed = state.is_search_active;
            state.window.set_ime_allowed(state.ime_allowed);
        }
    }
}

fn redraw(state: &mut AppState) {
    let size = state.window.inner_size();
    if size.width == 0 || size.height == 0 {
        return;
    }
    state.update_preview_image();

    let filtered_pkgs: Vec<&PackageRecord> = state
        .filtered_indices
        .iter()
        .filter_map(|&idx| state.packages.get(idx))
        .collect();
    let dialog_view = state.dialog.as_ref().map(|d| d.view(&state.library));
    let task_info = state.task.as_ref().map(|task| ui::TaskProgressInfo {
        title: &task.title,
        phase: &task.phase,
        current: task.current,
        total: task.total,
        detail: &task.detail,
        frame: state.anim_frame,
        cancelling: task.cancel.load(Ordering::SeqCst),
    });

    state.renderer.render(ui::Frame {
        tab: state.active_tab,
        packages: &filtered_pkgs,
        installed_total: state.packages.len(),
        installed: &mut state.installed,
        preview: state.preview_image.as_ref(),
        search: &state.search_query,
        search_active: state.is_search_active,
        status: &state.status,
        status_kind: state.status_kind,
        dialog: dialog_view.as_ref(),
        task: task_info,
        tables: &mut state.tables,
        drop_hint: state.drag_hover,
    });

    if let Ok(mut buffer) = state.surface.buffer_mut() {
        let pix = &state.renderer.pixmap;
        if state.ui_scale == 1.0 && buffer.len() == (pix.width() * pix.height()) as usize {
            for (dst, src) in buffer.iter_mut().zip(pix.data().chunks_exact(4)) {
                *dst = pack_argb(src);
            }
        } else {
            enlarge_into(&mut buffer, size.width, size.height, pix, state.ui_scale);
        }
        buffer.present().ok();
    }
}

/// The window scale factor as the UI scale: unscaled displays stay at exactly 1.0.
fn ui_scale_of(factor: f64) -> f32 {
    if factor > 1.05 {
        factor as f32
    } else {
        1.0
    }
}

/// The size the UI is drawn at for a physical window size.
fn logical_size(width: u32, height: u32, scale: f32) -> (u32, u32) {
    (
        ((width as f32 / scale).ceil() as u32).max(1),
        ((height as f32 / scale).ceil() as u32).max(1),
    )
}

fn pack_argb(p: &[u8]) -> u32 {
    ((p[3] as u32) << 24) | ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | (p[2] as u32)
}

/// Fills the `width` x `height` surface buffer from the smaller UI pixmap with
/// bilinear sampling, so a scaled display gets a bigger UI instead of a tiny one.
fn enlarge_into(buf: &mut [u32], width: u32, height: u32, pix: &tiny_skia::Pixmap, scale: f32) {
    let (pw, ph) = (pix.width() as i32, pix.height() as i32);
    let data = pix.data();
    let at = |x: i32, y: i32, c: usize| {
        data[((y.clamp(0, ph - 1) * pw + x.clamp(0, pw - 1)) * 4) as usize + c] as u32
    };
    for y in 0..height {
        let fy = (y as f32 + 0.5) / scale - 0.5;
        let y0 = fy.floor();
        let wy = ((fy - y0) * 256.0) as u32;
        let y0 = y0 as i32;
        for x in 0..width {
            let fx = (x as f32 + 0.5) / scale - 0.5;
            let x0 = fx.floor();
            let wx = ((fx - x0) * 256.0) as u32;
            let x0 = x0 as i32;
            let mut px = [0u8; 4];
            for (c, out) in px.iter_mut().enumerate() {
                let top = at(x0, y0, c) * (256 - wx) + at(x0 + 1, y0, c) * wx;
                let bot = at(x0, y0 + 1, c) * (256 - wx) + at(x0 + 1, y0 + 1, c) * wx;
                *out = ((top * (256 - wy) + bot * wy) >> 16) as u8;
            }
            if let Some(dst) = buf.get_mut((y * width + x) as usize) {
                *dst = pack_argb(&px);
            }
        }
    }
}

fn poll_task(state: &mut AppState) {
    let Some(task) = &mut state.task else {
        return;
    };
    let mut done = None;
    while let Ok(msg) = task.receiver.try_recv() {
        match msg {
            TaskMessage::Progress {
                phase,
                current,
                total,
                detail,
            } => {
                task.phase = phase;
                task.current = current;
                task.total = total;
                task.detail = detail;
            }
            TaskMessage::Done(result) => {
                done = Some(result);
                break;
            }
        }
    }
    let Some(result) = done else {
        return;
    };
    let kind = state.task.take().map(|t| t.kind).unwrap_or(TaskKind::Other);
    let success = result.is_ok();
    match result {
        Ok(msg) if kind == TaskKind::Adds => state.ok(format!("{msg} · {RETURN_TO_BEETLE}")),
        Ok(msg) => state.ok(msg),
        Err(msg) => state.err(msg),
    }
    state.refresh_packages();
    if kind == TaskKind::IrLookup {
        if let (true, Some(pending)) = (success, state.ir_pending.take()) {
            table_ir_ready(state, pending);
        }
    }
    if success {
        match kind {
            TaskKind::Adds => state.tables.stale = state.tables.index.is_some(),
            TaskKind::TableScan => {
                state.tables.reload_index();
                state.tables.stale = false;
            }
            TaskKind::TableAdded => {
                let before: Vec<String> =
                    state.tables.tables.iter().map(|t| t.name.clone()).collect();
                state.tables.reload_tables();
                let added = state
                    .tables
                    .tables
                    .iter()
                    .find(|t| !before.contains(&t.name))
                    .map(|t| t.name.clone());
                if let Some(name) = added {
                    state.tables.show_table(&name);
                }
            }
            TaskKind::Other | TaskKind::IrLookup => {}
        }
    }
    state.window.request_redraw();
}

/// Tells where the added songs show up (Beetle rescans when its window regains focus).
const RETURN_TO_BEETLE: &str = "Beetle 창으로 돌아가면 목록이 갱신돼요";

fn poll_picker(state: &mut AppState) {
    let Some((target, rx)) = &state.picker else {
        return;
    };
    let Ok(picked) = rx.try_recv() else {
        return;
    };
    let target = *target;
    state.picker = None;
    state.window.request_redraw();
    let Some(path) = picked else {
        return;
    };
    match target {
        PickTarget::AddPath => {
            state.dialog = None;
            state.start_add(path);
        }
        PickTarget::Field(i) => {
            let text = path.display().to_string();
            let library = matches!(
                state.dialog.as_ref().map(|d| &d.kind),
                Some(DialogKind::Library)
            );
            if library {
                library_add(state, &text);
            } else if let Some(field) = state.dialog.as_mut().and_then(|d| d.fields.get_mut(i)) {
                *field = text;
            }
        }
    }
}

/// A file or folder dropped on the window: into the open dialog's field, or added.
fn handle_drop(state: &mut AppState, path: PathBuf) {
    if let Some(dialog) = &mut state.dialog {
        if dialog.kind != DialogKind::Add && dialog.has_fields() {
            let focus = dialog.focus.min(dialog.fields.len() - 1);
            dialog.fields[focus] = path.display().to_string();
            if dialog.kind == DialogKind::Library {
                let text = path.display().to_string();
                library_add(state, &text);
            }
            return;
        }
        state.dialog = None;
    }
    state.start_add(path);
}

fn library_add(state: &mut AppState, text: &str) {
    let mut list = bms_package_manager::load_library();
    // A folder, a disc image (.iso) or a drive.
    match bms_package_manager::library::library_entry(text) {
        Ok(abs) if list.add(&abs) => match bms_package_manager::save_library(&list) {
            Ok(()) => state.ok(format!(
                "{} 연결했어요. 게임을 다시 시작하면 반영돼요.",
                josa::quoted_eul(&abs)
            )),
            Err(e) => state.err(format!("저장하지 못했어요: {e}")),
        },
        Ok(abs) => state.info(format!("{} 이미 연결돼 있어요", josa::quoted_eun(&abs))),
        Err(e) => state.err(format!("폴더나 디스크 이미지를 찾을 수 없어요: {e}")),
    }
    state.library = list.paths().to_vec();
    if let Some(dialog) = &mut state.dialog {
        for field in &mut dialog.fields {
            field.clear();
        }
    }
}

fn library_remove(state: &mut AppState, index: usize) {
    let mut list = bms_package_manager::load_library();
    let Some(path) = list.paths().get(index).cloned() else {
        return;
    };
    if list.remove(&path) {
        match bms_package_manager::save_library(&list) {
            Ok(()) => state.ok(format!(
                "'{path}' 연결을 뺐어요. 폴더 안의 파일은 그대로예요."
            )),
            Err(e) => state.err(format!("저장하지 못했어요: {e}")),
        }
    }
    state.library = list.paths().to_vec();
}

/// Runs what a click (or a key mapped to it) asks for.
fn dispatch(state: &mut AppState, action: UiAction) {
    match action {
        UiAction::None => {}
        UiAction::Tab(tab) => {
            state.active_tab = tab;
            state.is_search_active = false;
        }
        UiAction::Help => state.open_dialog(DialogKind::Help),
        UiAction::FocusSearch => state.is_search_active = true,
        UiAction::ClearSearch => {
            state.search_query.clear();
            state.apply_filter();
        }
        UiAction::OpenAdd => state.open_dialog(DialogKind::Add),
        UiAction::OpenLibrary => {
            state.library = bms_package_manager::load_library().paths().to_vec();
            state.open_dialog(DialogKind::Library);
        }
        UiAction::OpenAdvanced => state.open_dialog(DialogKind::Advanced),
        UiAction::SelectInstalled(i) => state.installed.select(i),
        UiAction::UseVersion(i) => use_version(state, i),
        UiAction::AskUninstallVersion(i) => {
            if let Some(pkg) = state.selected_package() {
                let st = pkg.state_hashes.keys().nth(i).cloned();
                let (id, name) = (pkg.id.clone(), pkg.name.clone());
                state.open_dialog(DialogKind::ConfirmUninstall {
                    id,
                    name,
                    state: st,
                });
            }
        }
        UiAction::AskUninstall => {
            if let Some(pkg) = state.selected_package() {
                let (id, name) = (pkg.id.clone(), pkg.name.clone());
                state.open_dialog(DialogKind::ConfirmUninstall {
                    id,
                    name,
                    state: None,
                });
            }
        }
        UiAction::AskRemoveBga => {
            if let Some(pkg) = state.selected_package() {
                let (id, name) = (pkg.id.clone(), pkg.name.clone());
                state.open_dialog(DialogKind::ConfirmRemoveBga { id, name });
            }
        }
        UiAction::PrevTable => state.tables.switch_table(false),
        UiAction::NextTable => state.tables.switch_table(true),
        UiAction::SelectTableRow(i) => state.tables.view.select(i),
        UiAction::OpenSongPage => table_open_link(state, false),
        UiAction::OpenChartPage => table_open_link(state, true),
        UiAction::AskDownloadChart => table_ask_diff(state),
        UiAction::AskIrChart => table_ask_ir(state),
        UiAction::AskAddFromArchive => {
            if let Some(row) = state.tables.selected_row() {
                let title = row.title.clone();
                state.open_dialog(DialogKind::TableGetBody { title });
            }
        }
        UiAction::ScanCollection => table_scan(state),
        UiAction::AskAddTable => state.open_dialog(DialogKind::AddTable),
        UiAction::DialogFocus(i) => {
            if let Some(dialog) = &mut state.dialog {
                dialog.focus = i;
            }
        }
        UiAction::DialogBrowse(i) => {
            if let Some((kind, title)) = state.dialog.as_ref().and_then(|d| d.browse(i)) {
                state.open_picker(PickTarget::Field(i), kind, title);
            }
        }
        UiAction::DialogBrowseArchive => {
            if let Some((kind, title)) = state.dialog.as_ref().and_then(|d| d.archive_pick()) {
                state.open_picker(PickTarget::Field(0), kind, title);
            }
        }
        UiAction::DialogConfirm => confirm_dialog(state),
        UiAction::DialogCancel => state.dialog = None,
        UiAction::DialogChoice(group, option) => {
            if let Some(Dialog {
                kind: DialogKind::Pack { turbo, bga },
                ..
            }) = &mut state.dialog
            {
                match group {
                    0 => *turbo = option == 1,
                    _ => {
                        *bga = match option {
                            0 => BgaPackMode::Embed,
                            1 => BgaPackMode::Split,
                            _ => BgaPackMode::NoVideo,
                        }
                    }
                }
            }
        }
        UiAction::DialogListRemove(i) => library_remove(state, i),
        UiAction::DialogCard(i) => {
            let Some(dialog) = &state.dialog else {
                return;
            };
            if let Some((kind, title)) = dialog.card_pick(i) {
                state.open_picker(PickTarget::AddPath, kind, title);
            } else if dialog.kind == DialogKind::Advanced {
                state.open_dialog(match i {
                    0 => DialogKind::Pack {
                        turbo: false,
                        bga: BgaPackMode::Embed,
                    },
                    1 => DialogKind::ApplyDelta,
                    _ => DialogKind::CreateDelta,
                });
            }
        }
        UiAction::CancelTask => {
            if let Some(task) = &state.task {
                task.cancel.store(true, Ordering::SeqCst);
                state.info("취소하는 중...");
            }
        }
    }
}

fn confirm_dialog(state: &mut AppState) {
    let Some(dialog) = &state.dialog else {
        return;
    };
    let kind = dialog.kind.clone();
    let first = dialog.field(0);
    let second = dialog.field(1);
    match kind {
        DialogKind::Help | DialogKind::Advanced => state.dialog = None,
        DialogKind::Add | DialogKind::ApplyDelta => {
            if first.is_empty() {
                state.err("경로를 넣거나 골라 주세요");
                return;
            }
            state.dialog = None;
            state.start_add(PathBuf::from(first));
        }
        DialogKind::Library => {
            for text in [first, second] {
                if !text.is_empty() {
                    library_add(state, &text);
                }
            }
        }
        DialogKind::Pack { turbo, bga } => {
            if first.is_empty() {
                state.err("패키지로 만들 곡 폴더나 압축 파일을 골라 주세요");
                return;
            }
            state.dialog = None;
            let folder = PathBuf::from(first);
            let name = folder
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            state.start_task(
                format!("'{name}' 패키지 만드는 중"),
                "곡 폴더 읽는 중...",
                TaskKind::Other,
                move |r| tasks::pack(folder, turbo, bga, r),
            );
        }
        DialogKind::CreateDelta => {
            if first.is_empty() || second.is_empty() {
                state.err("원래 폴더와 바뀐 폴더를 모두 골라 주세요");
                return;
            }
            state.dialog = None;
            let (base, target) = (PathBuf::from(first), PathBuf::from(second));
            state.start_task(
                "업데이트 파일 만드는 중".to_string(),
                "두 폴더 비교하는 중...",
                TaskKind::Other,
                move |_| tasks::create_delta(base, target),
            );
        }
        DialogKind::ConfirmUninstall {
            id,
            name,
            state: one,
        } => {
            state.dialog = None;
            uninstall(state, &id, &name, one);
        }
        DialogKind::ConfirmRemoveBga { id, name } => {
            state.dialog = None;
            if state.busy() {
                return;
            }
            match state.manager.remove_bga_companion(&id) {
                Ok(reclaimed) => {
                    let mb = reclaimed as f64 / (1024.0 * 1024.0);
                    state.ok(format!("'{name}'의 배경 영상을 지웠어요 ({mb:.1} MB 확보)"));
                }
                Err(e) => state.err(format!("배경 영상을 지우지 못했어요: {e}")),
            }
            state.refresh_packages();
        }
        DialogKind::TableFetchDiff { .. } => {
            state.dialog = None;
            table_start_diff(state);
        }
        DialogKind::TableFetchIr {
            title,
            folder,
            links,
            ..
        } => {
            state.dialog = None;
            table_start_ir(state, title, folder, links);
        }
        DialogKind::TableGetBody { .. } => {
            if first.is_empty() {
                state.err("받은 곡 파일을 골라 주세요");
                return;
            }
            state.dialog = None;
            table_start_get(state, PathBuf::from(first));
        }
        DialogKind::AddTable => {
            let address = first;
            if address.is_empty() {
                state.err("난이도표 주소를 넣어 주세요");
                return;
            }
            state.dialog = None;
            state.start_task(
                "난이도표 받는 중".to_string(),
                "표를 내려받는 중...",
                TaskKind::TableAdded,
                move |_| {
                    let client = bms_package_manager::HttpClient::with_timeouts(
                        Duration::from_secs(10),
                        Duration::from_secs(30),
                    );
                    let get = |url: &str, max: u64| client.get_bytes(url, max);
                    let table = bms_package_manager::fetch_table(&get, &address)
                        .map_err(|e| format!("난이도표를 받지 못했어요: {e}"))?;
                    tables_tab::store()
                        .install(&table)
                        .map_err(|e| format!("난이도표를 저장하지 못했어요: {e}"))?;
                    Ok(format!(
                        "'{}' 난이도표를 추가했어요 ({}곡)",
                        table.name,
                        table.entries.len()
                    ))
                },
            );
        }
    }
}

fn use_version(state: &mut AppState, index: usize) {
    if state.busy() {
        return;
    }
    let Some(pkg) = state.selected_package() else {
        return;
    };
    let Some(hash) = pkg.state_hashes.keys().nth(index).cloned() else {
        return;
    };
    let id = pkg.id.clone();
    match state.manager.set_active(&id, &hash) {
        Ok(()) => state.ok(format!("이제 게임에서 쓰는 버전: {}번", index + 1)),
        Err(e) => state.err(format!("버전을 바꾸지 못했어요: {e}")),
    }
    state.refresh_packages();
}

fn uninstall(state: &mut AppState, id: &str, name: &str, one: Option<String>) {
    if state.busy() {
        return;
    }
    let hashes: Vec<String> = match one {
        Some(hash) => vec![hash],
        None => state
            .packages
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.state_hashes.keys().cloned().collect())
            .unwrap_or_default(),
    };
    let mut result = Ok(());
    for hash in &hashes {
        result = state.manager.uninstall(id, hash);
        if result.is_err() {
            break;
        }
    }
    match result {
        Ok(()) => state.ok(format!("{} 삭제했어요", josa::quoted_eul(&name))),
        Err(e) => state.err(format!("삭제하지 못했어요: {e}")),
    }
    state.refresh_packages();
}

fn handle_key_input(state: &mut AppState, code: KeyCode, text: Option<&str>) {
    let ctrl = state.modifiers.control_key();
    let paste = (ctrl && code == KeyCode::KeyV) || text == Some("\u{16}");

    // 1. The open dialog takes every key.
    if let Some(dialog) = &mut state.dialog {
        match code {
            KeyCode::Escape => state.dialog = None,
            KeyCode::Enter | KeyCode::NumpadEnter => confirm_dialog(state),
            KeyCode::Tab if dialog.fields.len() > 1 => {
                dialog.focus = (dialog.focus + 1) % dialog.fields.len();
            }
            KeyCode::Backspace => {
                if let Some(field) = dialog.fields.get_mut(dialog.focus) {
                    field.pop();
                }
            }
            _ => {
                if let Some(field) = dialog.fields.get_mut(dialog.focus) {
                    if paste {
                        if let Some(clip) = clipboard::get_clipboard_text() {
                            field.push_str(clip.trim_end_matches(['\r', '\n']));
                        }
                    } else if let Some(t) = text {
                        field.extend(t.chars().filter(|c| !c.is_control()));
                    }
                }
            }
        }
        return;
    }

    // 2. Typing in the search box.
    if state.is_search_active {
        match code {
            KeyCode::Escape | KeyCode::Enter | KeyCode::NumpadEnter => {
                state.is_search_active = false
            }
            KeyCode::ArrowDown | KeyCode::ArrowUp => {
                state.is_search_active = false;
                handle_key_input(state, code, text);
            }
            KeyCode::Backspace => {
                state.search_query.pop();
                state.apply_filter();
            }
            _ => {
                if paste {
                    if let Some(clip) = clipboard::get_clipboard_text() {
                        state.search_query.push_str(clip.trim());
                        state.apply_filter();
                    }
                } else if let Some(t) = text {
                    let before = state.search_query.len();
                    state
                        .search_query
                        .extend(t.chars().filter(|c| !c.is_control()));
                    if state.search_query.len() != before {
                        state.apply_filter();
                    }
                }
            }
        }
        return;
    }

    // 3. Keys that work on every tab.
    match code {
        KeyCode::Escape => {
            if state.task.is_some() {
                dispatch(state, UiAction::CancelTask);
            } else if !state.search_query.is_empty() {
                dispatch(state, UiAction::ClearSearch);
            }
            return;
        }
        KeyCode::Tab => {
            let order = [ActiveTab::Installed, ActiveTab::Tables];
            let i = order
                .iter()
                .position(|&t| t == state.active_tab)
                .unwrap_or(0);
            let len = order.len();
            let next = if state.modifiers.shift_key() {
                (i + len - 1) % len
            } else {
                (i + 1) % len
            };
            dispatch(state, UiAction::Tab(order[next]));
            return;
        }
        KeyCode::F1 => {
            dispatch(state, UiAction::Help);
            return;
        }
        KeyCode::Slash | KeyCode::KeyF
            if state.active_tab != ActiveTab::Tables && (code == KeyCode::Slash || ctrl) =>
        {
            dispatch(state, UiAction::FocusSearch);
            return;
        }
        _ => {}
    }

    // 4. Keys of the shown tab.
    let page = |len: usize, view: &mut ListView, code: KeyCode| match code {
        KeyCode::ArrowUp | KeyCode::KeyK => view.move_by(-1, len),
        KeyCode::ArrowDown | KeyCode::KeyJ => view.move_by(1, len),
        KeyCode::PageUp => view.move_by(-10, len),
        KeyCode::PageDown => view.move_by(10, len),
        KeyCode::Home => view.move_by(-(len as isize), len),
        KeyCode::End => view.move_by(len as isize, len),
        _ => {}
    };
    match state.active_tab {
        ActiveTab::Installed => match code {
            KeyCode::Delete => dispatch(state, UiAction::AskUninstall),
            KeyCode::KeyI | KeyCode::Insert => dispatch(state, UiAction::OpenAdd),
            KeyCode::KeyL => dispatch(state, UiAction::OpenLibrary),
            KeyCode::F5 => {
                state.refresh_packages();
                state.info("목록을 새로 읽었어요");
            }
            _ => page(state.filtered_indices.len(), &mut state.installed, code),
        },
        ActiveTab::Tables => match code {
            KeyCode::BracketLeft | KeyCode::ArrowLeft => dispatch(state, UiAction::PrevTable),
            KeyCode::BracketRight | KeyCode::ArrowRight => dispatch(state, UiAction::NextTable),
            KeyCode::KeyO => dispatch(state, UiAction::OpenSongPage),
            KeyCode::KeyD => dispatch(state, UiAction::AskDownloadChart),
            KeyCode::KeyG => dispatch(state, UiAction::AskAddFromArchive),
            KeyCode::F5 | KeyCode::KeyS => dispatch(state, UiAction::ScanCollection),
            _ => {
                let len = state.tables.rows.len();
                page(len, &mut state.tables.view, code)
            }
        },
    }
}

/// The folder a downloaded chart goes to: `songs/<table>/<#>` under the folder
/// the window was started from, the same `songs` folder the game reads.
fn table_chart_folder(table_name: &str, number: usize) -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    base.join("songs")
        .join(bms_package_manager::TableStore::slug(table_name))
        .join(number.to_string())
}

/// Opens the selected entry's song page, or its chart page, in the browser.
fn table_open_link(state: &mut AppState, chart: bool) {
    let Some(row) = state.tables.selected_row() else {
        return;
    };
    let link = if chart { &row.url_diff } else { &row.url };
    if link.is_empty() {
        state.err("이 곡은 난이도표에 받는 곳 주소가 없어요");
        return;
    }
    match bms_package_manager::table_fetch::open_in_browser(link) {
        Ok(()) => {
            let what = if chart { "채보" } else { "곡 파일" };
            state.ok(format!(
                "브라우저에서 {what} 페이지를 열었어요. 받은 뒤 '받은 곡 파일 추가'를 눌러 주세요."
            ));
        }
        Err(e) => state.err(e),
    }
}

/// Asks before downloading the selected entry's chart.
fn table_ask_diff(state: &mut AppState) {
    let Some(row) = state.tables.selected_row() else {
        return;
    };
    if row.url_diff.is_empty() {
        state.err("이 곡은 난이도표에 채보 주소가 없어요");
        return;
    }
    if !bms_package_manager::table_fetch::is_direct_pack(&row.url_diff) {
        table_open_link(state, true);
        return;
    }
    let (number, title) = (row.number, row.title.clone());
    let table_name = state
        .tables
        .table()
        .map(|t| t.name.clone())
        .unwrap_or_default();
    let folder = table_chart_folder(&table_name, number);
    state.open_dialog(DialogKind::TableFetchDiff { title, folder });
}

/// Downloads the chart of the selected entry into `songs/<table>/<#>`.
fn table_start_diff(state: &mut AppState) {
    let (Some(row), Some(entry), Some(table)) = (
        state.tables.selected_row().cloned(),
        state.tables.selected_entry().cloned(),
        state.tables.table().map(|t| t.name.clone()),
    ) else {
        return;
    };
    let number = row.number;
    let folder = table_chart_folder(&table, number);
    let scratch =
        std::env::temp_dir().join(format!("bpm-gui-diff-{}-{number}", std::process::id()));
    state.start_task(
        format!("'{}' 채보 받는 중", row.title),
        "내려받아 확인하는 중...",
        TaskKind::Adds,
        move |_| {
            let _ = fs::remove_dir_all(&scratch);
            let client = bms_package_manager::HttpClient::new();
            let result =
                bms_package_manager::table_ops::fetch_diff(&client, &entry, &scratch, &folder);
            let _ = fs::remove_dir_all(&scratch);
            let kept = result.map_err(|e| format!("채보를 받지 못했어요: {e}"))?;
            if kept.matching == 0 {
                return Err(
                    "받은 파일에 이 곡의 채보가 없어요 (버전이 다를 수 있어요). 아무것도 저장하지 않았어요."
                        .to_string(),
                );
            }
            Ok(format!(
                "채보 {}개를 {}에 저장했어요. '내 곡 다시 확인'을 누르면 목록이 맞춰져요.",
                kept.matching,
                folder.display()
            ))
        },
    );
}

/// A chart page lookup that is running: the chart it is for, the folder its
/// download goes to, and the slot the lookup fills with the page's links.
struct IrPending {
    title: String,
    folder: PathBuf,
    /// The entry that was looked up. Its hash decides what is kept, so it is
    /// not read again from the selection, which may have moved.
    entry: beetle_core::TableEntry,
    found: Arc<Mutex<Option<bms_package_manager::ir::IrLinks>>>,
}

/// Reads the selected entry's IR chart page for its download links, in the background.
fn table_ask_ir(state: &mut AppState) {
    let (Some(row), Some(entry), Some(table)) = (
        state.tables.selected_row().cloned(),
        state.tables.selected_entry().cloned(),
        state.tables.table().map(|t| t.name.clone()),
    ) else {
        return;
    };
    let Some(md5) = entry.md5 else {
        state.err("이 곡은 난이도표에 MD5가 없어서 IR에서 찾을 수 없어요");
        return;
    };
    if state.busy() {
        return;
    }
    let found = Arc::new(Mutex::new(None));
    let slot = found.clone();
    let title = row.title.clone();
    state.ir_pending = Some(IrPending {
        title: row.title.clone(),
        folder: table_chart_folder(&table, row.number),
        entry,
        found,
    });
    state.start_task(
        format!("'{title}' IR 페이지 읽는 중"),
        "IR 페이지를 읽는 중...",
        TaskKind::IrLookup,
        move |_| {
            let client = bms_package_manager::HttpClient::new();
            let links = bms_package_manager::ir::fetch_chart_links(&client, &md5)?;
            *slot
                .lock()
                .map_err(|_| "IR 결과를 전하지 못했어요".to_string())? = Some(links);
            Ok("IR 페이지를 읽었어요".to_string())
        },
    );
}

/// Opens the download dialog for the zip links of a finished IR lookup.
fn table_ir_ready(state: &mut AppState, pending: IrPending) {
    // Opening the dialog now would replace whatever the user has open.
    if state.dialog.is_some() {
        state.err("다른 창을 닫은 뒤 'IR에서 채보 찾기'를 다시 눌러 주세요");
        return;
    }
    let links = pending.found.lock().ok().and_then(|mut slot| slot.take());
    let Some(links) = links else {
        state.err("IR 페이지의 링크를 받지 못했어요");
        return;
    };
    let (zips, others) = bms_package_manager::ir::split_zip_links(links);
    if zips.is_empty() {
        state.err(format!(
            "'{}'의 IR 페이지에 받을 수 있는 zip 파일이 없어요",
            pending.title
        ));
        return;
    }
    state.ir_entry = Some(pending.entry);
    state.open_dialog(DialogKind::TableFetchIr {
        title: pending.title,
        folder: pending.folder,
        links: zips,
        others: others.len(),
    });
}

/// Downloads the chosen zip links of an IR chart page into the chart's folder.
/// Each pack is kept only when one of its charts has the entry's hash.
fn table_start_ir(
    state: &mut AppState,
    title: String,
    folder: PathBuf,
    links: Vec<(&'static str, String)>,
) {
    let Some(entry) = state.ir_entry.take() else {
        state.err("채보를 받을 곡을 다시 찾아 주세요");
        return;
    };
    let total = links.len();
    state.start_task(
        format!("'{title}' IR에서 받는 중"),
        "내려받아 확인하는 중...",
        TaskKind::Adds,
        move |r| {
            let client = bms_package_manager::HttpClient::new();
            let mut kept_total = 0;
            let mut copied_total = 0;
            let mut skipped_total = 0;
            let mut failures = Vec::new();
            for (i, (label, url)) in links.iter().enumerate() {
                if r.cancelled() {
                    return Err("취소했어요".to_string());
                }
                r.progress("내려받는 중", i + 1, total, url);
                let scratch =
                    std::env::temp_dir().join(format!("bpm-gui-ir-{}-{i}", std::process::id()));
                let _ = fs::remove_dir_all(&scratch);
                let result = bms_package_manager::table_ops::fetch_pack(
                    &client, url, &entry, &scratch, &folder,
                );
                let _ = fs::remove_dir_all(&scratch);
                match result {
                    Ok(kept) => {
                        kept_total += kept.matching;
                        copied_total += kept.copied;
                        skipped_total += kept.skipped;
                    }
                    Err(e) => failures.push(format!("{label}: {e}")),
                }
            }
            let skipped = if failures.is_empty() {
                String::new()
            } else {
                format!(" 받지 못한 링크 {}개: {}", failures.len(), failures.join("; "))
            };
            if kept_total == 0 {
                return Err(format!(
                    "받은 파일에 이 곡의 채보가 없어요 (버전이 다를 수 있어요). 아무것도 저장하지 않았어요.{skipped}"
                ));
            }
            Ok(format!(
                "채보 {kept_total}개가 {}에 있어요 (새로 저장 {copied_total}개, 이미 있어 그대로 둔 파일 {skipped_total}개). '내 곡 다시 확인'을 누르면 목록이 맞춰져요.{skipped}",
                folder.display()
            ))
        },
    );
}

/// Adds the selected entry from the song archive at `path`.
fn table_start_get(state: &mut AppState, path: PathBuf) {
    let (Some(entry), Some(row)) = (
        state.tables.selected_entry().cloned(),
        state.tables.selected_row().cloned(),
    ) else {
        return;
    };
    let packages_root = state.manager.root_dir().to_path_buf();
    let scratch =
        std::env::temp_dir().join(format!("bpm-gui-get-{}-{}", std::process::id(), row.number));
    state.start_task(
        format!("'{}' 추가하는 중", row.title),
        "곡 파일 압축 푸는 중...",
        TaskKind::Adds,
        move |_| {
            let _ = fs::remove_dir_all(&scratch);
            let client = bms_package_manager::HttpClient::new();
            let result = bms_package_manager::table_ops::get_from_body(
                &client,
                &entry,
                &path,
                &scratch,
                &packages_root,
            );
            let _ = fs::remove_dir_all(&scratch);
            result
                .map(|summary| format!("{} 추가했어요 ({summary})", josa::quoted_eul(&row.title)))
                .map_err(|e| format!("추가하지 못했어요: {e}"))
        },
    );
}

/// Scans the library folders, the songs folder, and the packages into the index.
fn table_scan(state: &mut AppState) {
    let packages_root = state.manager.root_dir().to_path_buf();
    state.start_task(
        "내 곡 확인하는 중".to_string(),
        "폴더와 설치된 곡을 읽는 중...",
        TaskKind::TableScan,
        move |_| {
            let report = bms_package_manager::table_ops::scan_collection(&packages_root)?;
            Ok(format!(
                "확인을 마쳤어요: 채보 {}개를 찾았어요",
                report.charts
            ))
        },
    );
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to build event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = BpmGuiApp { state: None };
    event_loop
        .run_app(&mut app)
        .expect("Error running event loop");
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    #[test]
    fn unscaled_displays_stay_at_one() {
        assert_eq!(ui_scale_of(1.0), 1.0);
        assert_eq!(ui_scale_of(1.5), 1.5);
        assert_eq!(logical_size(1620, 1110, 1.5), (1080, 740));
    }

    #[test]
    fn enlarging_keeps_a_solid_color_and_covers_the_buffer() {
        let mut pix = tiny_skia::Pixmap::new(4, 4).unwrap();
        pix.fill(tiny_skia::Color::from_rgba8(10, 20, 30, 255));
        let mut buf = vec![0u32; 6 * 6];
        enlarge_into(&mut buf, 6, 6, &pix, 1.5);
        assert!(buf.iter().all(|&p| p == 0xFF0A141E), "{buf:x?}");
    }
}
