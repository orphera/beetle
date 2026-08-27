#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

use beetle_render::image::ImageBuffer;
use bms_package_manager::{PackageRecord, PackageManager};
use softbuffer::{Context, Surface};
use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ui::GuiRenderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalMode {
    ImportFolder,
    InstallBmsp,
    PackFolder,
}

struct AppState {
    window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: GuiRenderer,
    manager: PackageManager,
    packages: Vec<PackageRecord>,
    filtered_indices: Vec<usize>,
    selected_idx: usize,
    selected_ver_idx: usize,
    search_query: String,
    is_search_active: bool,
    status_msg: String,
    modal: Option<(ModalMode, String)>,
    preview_image: Option<ImageBuffer>,
}

impl AppState {
    fn refresh_packages(&mut self) {
        // Reload registry
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

        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let q = self.search_query.trim().to_ascii_lowercase();
        if q.is_empty() {
            self.filtered_indices = (0..self.packages.len()).collect();
        } else {
            self.filtered_indices = self
                .packages
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.id.to_ascii_lowercase().contains(&q)
                        || p.name.to_ascii_lowercase().contains(&q)
                        || p.author.as_deref().unwrap_or("").to_ascii_lowercase().contains(&q)
                })
                .map(|(i, _)| i)
                .collect();
        }

        if self.selected_idx >= self.filtered_indices.len() {
            self.selected_idx = self.filtered_indices.len().saturating_sub(1);
        }
        self.selected_ver_idx = 0;
        self.update_preview_image();
    }

    fn update_preview_image(&mut self) {
        self.preview_image = None;
        if let Some(&pkg_idx) = self.filtered_indices.get(self.selected_idx) {
            if let Some(pkg) = self.packages.get(pkg_idx) {
                if let Some(ver_rec) = pkg.versions.get(&pkg.active_version) {
                    let dir = self.manager.root_dir().join(&ver_rec.path);
                    self.preview_image = load_artwork_from_dir(&dir);
                }
            }
        }
    }
}

fn load_artwork_from_dir(dir: &Path) -> Option<ImageBuffer> {
    if !dir.exists() {
        return None;
    }

    for name in &[
        "stagefile.bmp", "stage.bmp", "banner.bmp", "title.bmp", "cover.bmp",
        "STAGEFILE.BMP", "STAGE.BMP", "BANNER.BMP", "TITLE.BMP",
    ] {
        let p = dir.join(name);
        if let Some(img) = ImageBuffer::load_from_file(&p) {
            return Some(img);
        }
    }

    // Check image/ folder if present
    let img_dir = dir.join("image");
    if img_dir.exists() {
        if let Ok(entries) = fs::read_dir(&img_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("bmp") {
                        if let Some(img) = ImageBuffer::load_from_file(&p) {
                            return Some(img);
                        }
                    }
                }
            }
        }
    }

    None
}

struct BpmGuiApp {
    state: Option<AppState>,
}

impl ApplicationHandler for BpmGuiApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("BMS Package Manager (BPM GUI)")
            .with_inner_size(LogicalSize::new(960.0, 680.0));

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

        let packages_dir = env::var("BEETLE_PACKAGES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("packages"));

        let manager = PackageManager::new(&packages_dir).expect("Failed to initialize PackageManager");
        let size = window.inner_size();
        let renderer = GuiRenderer::new(size.width, size.height).expect("Failed to create GuiRenderer");

        let mut app_state = AppState {
            window,
            _context: context,
            surface,
            renderer,
            manager,
            packages: Vec::new(),
            filtered_indices: Vec::new(),
            selected_idx: 0,
            selected_ver_idx: 0,
            search_query: String::new(),
            is_search_active: false,
            status_msg: "Ready".to_string(),
            modal: None,
            preview_image: None,
        };

        app_state.refresh_packages();
        self.state = Some(app_state);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let (Some(w), Some(h)) = (NonZeroU32::new(new_size.width), NonZeroU32::new(new_size.height)) {
                    state.surface.resize(w, h).ok();
                    state.renderer.resize(new_size.width, new_size.height);
                    state.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        text,
                        ..
                    },
                ..
            } => {
                if key_state == ElementState::Pressed {
                    handle_key_input(state, code, text.as_deref(), event_loop);
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let size = state.window.inner_size();
                if NonZeroU32::new(size.width).is_some() && NonZeroU32::new(size.height).is_some() {
                    let filtered_pkgs: Vec<&PackageRecord> = state
                        .filtered_indices
                        .iter()
                        .filter_map(|&idx| state.packages.get(idx))
                        .collect();

                    let modal_info = state.modal.as_ref().map(|(mode, input)| {
                        let prompt = match mode {
                            ModalMode::ImportFolder => "Import BMS Folder (enter directory path):",
                            ModalMode::InstallBmsp => "Install .bmsp Package (enter file path):",
                            ModalMode::PackFolder => "Pack BMS Folder (enter directory path):",
                        };
                        (prompt, input.as_str())
                    });

                    state.renderer.render_frame(
                        &filtered_pkgs,
                        state.selected_idx,
                        state.selected_ver_idx,
                        &state.search_query,
                        state.is_search_active,
                        &state.status_msg,
                        state.preview_image.as_ref(),
                        modal_info,
                    );

                    if let Ok(mut buffer) = state.surface.buffer_mut() {
                        let data = state.renderer.pixmap.data();
                        for (dst, src) in buffer.iter_mut().zip(data.chunks_exact(4)) {
                            *dst = ((src[3] as u32) << 24)
                                | ((src[0] as u32) << 16)
                                | ((src[1] as u32) << 8)
                                | (src[2] as u32);
                        }
                        buffer.present().ok();
                    }
                }
            }
            _ => (),
        }
    }
}

fn handle_key_input(state: &mut AppState, code: KeyCode, text: Option<&str>, event_loop: &ActiveEventLoop) {
    // 1. Modal Dialog Input Mode
    if let Some((mode, input)) = &mut state.modal {
        match code {
            KeyCode::Escape => {
                state.modal = None;
            }
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Enter => {
                let target_path = input.trim().to_string();
                let m = *mode;
                state.modal = None;

                if target_path.is_empty() {
                    state.status_msg = "Path cannot be empty".to_string();
                    return;
                }

                match m {
                    ModalMode::ImportFolder => {
                        match state.manager.import_folder(&target_path, None) {
                            Ok(installed) => {
                                state.status_msg = format!("Imported '{}' v{}", installed.name, installed.version);
                                state.refresh_packages();
                            }
                            Err(e) => {
                                state.status_msg = format!("Import error: {e}");
                            }
                        }
                    }
                    ModalMode::InstallBmsp => {
                        match state.manager.install(&target_path) {
                            Ok(installed) => {
                                state.status_msg = format!("Installed '{}' v{}", installed.name, installed.version);
                                state.refresh_packages();
                            }
                            Err(e) => {
                                state.status_msg = format!("Install error: {e}");
                            }
                        }
                    }
                    ModalMode::PackFolder => {
                        let folder_p = PathBuf::from(&target_path);
                        let out_name = format!("{}.bmsp", folder_p.file_name().and_then(|n| n.to_str()).unwrap_or("package"));
                        match state.manager.pack_folder(&target_path, None) {
                            Ok(bytes) => {
                                if let Err(e) = fs::write(&out_name, bytes) {
                                    state.status_msg = format!("Write error: {e}");
                                } else {
                                    state.status_msg = format!("Packed into '{}'", out_name);
                                }
                            }
                            Err(e) => {
                                state.status_msg = format!("Pack error: {e}");
                            }
                        }
                    }
                }
            }
            _ => {
                if let Some(t) = text {
                    for c in t.chars() {
                        if !c.is_control() {
                            input.push(c);
                        }
                    }
                }
            }
        }
        return;
    }

    // 2. Search Filter Input Mode
    if state.is_search_active {
        match code {
            KeyCode::Escape => {
                state.is_search_active = false;
            }
            KeyCode::Enter => {
                state.is_search_active = false;
            }
            KeyCode::Backspace => {
                state.search_query.pop();
                state.apply_filter();
            }
            _ => {
                if let Some(t) = text {
                    for c in t.chars() {
                        if !c.is_control() {
                            state.search_query.push(c);
                        }
                    }
                    state.apply_filter();
                }
            }
        }
        return;
    }

    // 3. Normal Navigation & Shortcuts
    match code {
        KeyCode::Escape => {
            event_loop.exit();
        }
        KeyCode::Slash => {
            state.is_search_active = true;
        }
        KeyCode::F5 | KeyCode::KeyR => {
            state.refresh_packages();
            state.status_msg = "Packages refreshed".to_string();
        }
        KeyCode::ArrowUp | KeyCode::KeyK => {
            if state.selected_idx > 0 {
                state.selected_idx -= 1;
                state.selected_ver_idx = 0;
                state.update_preview_image();
            }
        }
        KeyCode::ArrowDown | KeyCode::KeyJ => {
            if !state.filtered_indices.is_empty() && state.selected_idx + 1 < state.filtered_indices.len() {
                state.selected_idx += 1;
                state.selected_ver_idx = 0;
                state.update_preview_image();
            }
        }
        KeyCode::ArrowLeft => {
            if state.selected_ver_idx > 0 {
                state.selected_ver_idx -= 1;
            }
        }
        KeyCode::ArrowRight => {
            if let Some(&pkg_idx) = state.filtered_indices.get(state.selected_idx) {
                if let Some(pkg) = state.packages.get(pkg_idx) {
                    if state.selected_ver_idx + 1 < pkg.versions.len() {
                        state.selected_ver_idx += 1;
                    }
                }
            }
        }
        KeyCode::KeyA => {
            // Activate selected version
            if let Some(&pkg_idx) = state.filtered_indices.get(state.selected_idx) {
                if let Some(pkg) = state.packages.get(pkg_idx) {
                    let versions: Vec<&String> = pkg.versions.keys().collect();
                    if let Some(&ver) = versions.get(state.selected_ver_idx) {
                        let id = pkg.id.clone();
                        let version = ver.clone();
                        match state.manager.set_active(&id, &version) {
                            Ok(()) => {
                                state.status_msg = format!("Set active version of '{}' to v{}", id, version);
                                state.refresh_packages();
                            }
                            Err(e) => {
                                state.status_msg = format!("Activation error: {e}");
                            }
                        }
                    }
                }
            }
        }
        KeyCode::KeyU | KeyCode::Delete => {
            // Uninstall selected version
            if let Some(&pkg_idx) = state.filtered_indices.get(state.selected_idx) {
                if let Some(pkg) = state.packages.get(pkg_idx) {
                    let versions: Vec<&String> = pkg.versions.keys().collect();
                    if let Some(&ver) = versions.get(state.selected_ver_idx) {
                        let id = pkg.id.clone();
                        let version = ver.clone();
                        match state.manager.uninstall(&id, &version) {
                            Ok(()) => {
                                state.status_msg = format!("Uninstalled '{}' v{}", id, version);
                                state.refresh_packages();
                            }
                            Err(e) => {
                                state.status_msg = format!("Uninstall error: {e}");
                            }
                        }
                    }
                }
            }
        }
        KeyCode::KeyI | KeyCode::F1 => {
            state.modal = Some((ModalMode::ImportFolder, String::new()));
        }
        KeyCode::F2 => {
            state.modal = Some((ModalMode::InstallBmsp, String::new()));
        }
        KeyCode::KeyP => {
            state.modal = Some((ModalMode::PackFolder, String::new()));
        }
        _ => (),
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to build event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = BpmGuiApp { state: None };
    event_loop.run_app(&mut app).expect("Error running event loop");
}
