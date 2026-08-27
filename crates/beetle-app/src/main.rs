use std::num::NonZeroU32;
use std::sync::Arc;

use beetle_render::{SkinConfig, SoftwareRenderer};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct AppState {
    window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: SoftwareRenderer,
}

#[derive(Default)]
struct BeetleApp {
    state: Option<AppState>,
}

impl ApplicationHandler for BeetleApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("Beetle BMS Player")
            .with_inner_size(LogicalSize::new(800.0, 720.0));

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(err) => {
                eprintln!("Failed to create window: {err}");
                return;
            }
        };

        let context = match Context::new(window.clone()) {
            Ok(c) => c,
            Err(err) => {
                eprintln!("Failed to create softbuffer context: {err}");
                return;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("Failed to create softbuffer surface: {err}");
                return;
            }
        };

        let size = window.inner_size();
        let renderer = SoftwareRenderer::new(size.width, size.height, SkinConfig::default())
            .expect("Failed to initialize software renderer");

        self.state = Some(AppState {
            window,
            _context: context,
            surface,
            renderer,
        });
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
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                    let _ = state.surface.resize(w, h);
                    state.renderer.resize(size.width, size.height);
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let width = state.renderer.width();
                let height = state.renderer.height();

                state.renderer.clear();

                if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
                    let _ = state.surface.resize(w, h);
                    if let Ok(mut buffer) = state.surface.buffer_mut() {
                        let src = state.renderer.data();
                        for (dst, chunk) in buffer.iter_mut().zip(src.chunks_exact(4)) {
                            // tiny-skia RGBA to softbuffer 0x00RRGGBB / 0xAARRGGBB
                            let r = chunk[0] as u32;
                            let g = chunk[1] as u32;
                            let b = chunk[2] as u32;
                            *dst = (r << 16) | (g << 8) | b;
                        }
                        let _ = buffer.present();
                    }
                }
            }
            _ => (),
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = BeetleApp::default();
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Application error: {e}");
    }
}
