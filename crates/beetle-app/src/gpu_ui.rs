//! App-side owner of the Canvas UI (ADR-026): the `Ui` plus GPU textures for
//! images that come from songs (BGA bitmaps, video frames, stage art).
//!
//! Textures are uploaded premultiplied. Layer BGA (channel 07) uses black as
//! the transparent color key, so it is uploaded separately with black
//! cleared to transparent.

use crate::config::GpuBackendSetting;
use beetle_core::{BmpId, ChartId};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{BgaVideoPlayer, D3d11Backend, GpuBackend, ImageBuffer, SizedTexture, Ui};
use std::collections::HashMap;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Creates the Direct3D 11 renderer for `window`: hardware first, then WARP
/// (`Auto`), or WARP only.
pub fn create_backend(window: &Window, setting: GpuBackendSetting) -> Result<D3d11Backend, String> {
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return Err("not a Win32 window".into());
    };
    let hwnd = win32.hwnd.get() as *mut std::ffi::c_void;
    let size = window.inner_size();
    match setting {
        GpuBackendSetting::Auto => D3d11Backend::new(hwnd, size.width, size.height),
        GpuBackendSetting::Warp => {
            D3d11Backend::with_driver_types(hwnd, size.width, size.height, &[D3D_DRIVER_TYPE_WARP])
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageKey {
    /// A BGA bitmap; `true` = black color-keyed (layer channel).
    Bga(BmpId, bool),
    /// The selected song's stage / jacket image, by chart id.
    Stage(ChartId),
}

struct VideoTexture {
    tex: SizedTexture,
    serial: u64,
}

pub struct GpuUi {
    pub ui: Ui,
    images: HashMap<ImageKey, SizedTexture>,
    videos: HashMap<(BmpId, bool), VideoTexture>,
}

/// RGBA8 premultiplied copy of `img`, optionally with pure black keyed out.
fn premultiplied(img: &ImageBuffer, color_key_black: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.pixels.len() * 4);
    for p in &img.pixels {
        let a = if color_key_black && p.r == 0 && p.g == 0 && p.b == 0 {
            0
        } else {
            p.a
        };
        let pm = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
        out.extend_from_slice(&[pm(p.r), pm(p.g), pm(p.b), a]);
    }
    out
}

impl GpuUi {
    pub fn new(scale: f32) -> Self {
        Self {
            ui: Ui::new(scale),
            images: HashMap::new(),
            videos: HashMap::new(),
        }
    }

    /// Returns the texture for a static image, uploading it on first use.
    pub fn image(
        &mut self,
        backend: &mut dyn GpuBackend,
        key: ImageKey,
        img: &ImageBuffer,
    ) -> Option<SizedTexture> {
        if let Some(t) = self.images.get(&key) {
            return Some(*t);
        }
        let keyed = matches!(key, ImageKey::Bga(_, true));
        let id = backend.create_texture(img.width, img.height, &premultiplied(img, keyed))?;
        let tex = SizedTexture {
            id,
            width: img.width,
            height: img.height,
        };
        self.images.insert(key, tex);
        Some(tex)
    }

    /// Returns the texture for a video BGA, re-uploading only when the
    /// decoder produced a new frame (`serial` changed).
    pub fn video(
        &mut self,
        backend: &mut dyn GpuBackend,
        id: BmpId,
        keyed: bool,
        frame: &ImageBuffer,
        serial: u64,
    ) -> Option<SizedTexture> {
        if let Some(v) = self.videos.get_mut(&(id, keyed)) {
            if v.serial != serial && v.tex.width == frame.width && v.tex.height == frame.height {
                backend.update_texture(
                    v.tex.id,
                    frame.width,
                    frame.height,
                    &premultiplied(frame, keyed),
                );
                v.serial = serial;
            }
            if v.tex.width == frame.width && v.tex.height == frame.height {
                return Some(v.tex);
            }
            backend.destroy_texture(v.tex.id);
            self.videos.remove(&(id, keyed));
        }
        let tex_id =
            backend.create_texture(frame.width, frame.height, &premultiplied(frame, keyed))?;
        let tex = SizedTexture {
            id: tex_id,
            width: frame.width,
            height: frame.height,
        };
        self.videos
            .insert((id, keyed), VideoTexture { tex, serial });
        Some(tex)
    }

    /// Keeps stage-image textures bounded while browsing: past
    /// `MAX_STAGE_TEXTURES`, frees all of them except `keep` (they are
    /// re-uploaded from the CPU-side cache on demand).
    pub fn trim_stage_textures(&mut self, backend: &mut dyn GpuBackend, keep: ChartId) {
        const MAX_STAGE_TEXTURES: usize = 32;
        let count = self
            .images
            .keys()
            .filter(|k| matches!(k, ImageKey::Stage(_)))
            .count();
        if count <= MAX_STAGE_TEXTURES {
            return;
        }
        self.images.retain(|k, t| {
            let drop = matches!(k, ImageKey::Stage(h) if *h != keep);
            if drop {
                backend.destroy_texture(t.id);
            }
            !drop
        });
    }

    /// Frees every song-scoped texture (BGA bitmaps and videos). Call when a
    /// new song starts; stage images are kept for the song list.
    pub fn release_song_textures(&mut self, backend: &mut dyn GpuBackend) {
        self.images.retain(|k, t| {
            let keep = matches!(k, ImageKey::Stage(_));
            if !keep {
                backend.destroy_texture(t.id);
            }
            keep
        });
        for (_, v) in self.videos.drain() {
            backend.destroy_texture(v.tex.id);
        }
    }
}

/// Texture for BGA `id`: the video's current frame if it is a video,
/// otherwise the decoded bitmap.
pub fn bga_texture(
    gpu: &mut GpuUi,
    backend: &mut dyn GpuBackend,
    bank: &HashMap<BmpId, ImageBuffer>,
    videos: &HashMap<BmpId, BgaVideoPlayer>,
    id: BmpId,
    keyed: bool,
) -> Option<SizedTexture> {
    if let Some(vp) = videos.get(&id) {
        if let Some(frame) = vp.current_frame() {
            return gpu.video(backend, id, keyed, frame, vp.frame_serial());
        }
    }
    let img = bank.get(&id)?;
    gpu.image(backend, ImageKey::Bga(id, keyed), img)
}

/// Base BGA texture: POOR image while it is showing, then the current BGA,
/// then the song's stage image as a static fallback (`state::resolve_bga_id`).
#[allow(clippy::too_many_arguments)]
pub fn gameplay_bga_texture(
    gpu: &mut GpuUi,
    backend: &mut dyn GpuBackend,
    bank: &HashMap<BmpId, ImageBuffer>,
    videos: &HashMap<BmpId, BgaVideoPlayer>,
    poor_until_time: f64,
    poor_bmp: Option<BmpId>,
    current_bmp: Option<BmpId>,
    stage_image: Option<&ImageBuffer>,
    song_id: ChartId,
    audio_time: f64,
) -> Option<SizedTexture> {
    let available =
        |id| bank.contains_key(&id) || videos.get(&id).is_some_and(|v| v.current_frame().is_some());
    match crate::state::resolve_bga_id(
        poor_until_time,
        poor_bmp,
        current_bmp,
        available,
        audio_time,
    ) {
        Some(id) => bga_texture(gpu, backend, bank, videos, id, false),
        None => gpu.image(backend, ImageKey::Stage(song_id), stage_image?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_render::ColorRgba;

    #[test]
    fn premultiply_and_color_key() {
        let mut img = ImageBuffer::new(3, 1, ColorRgba::new(0, 0, 0, 255));
        img.pixels[1] = ColorRgba::new(200, 100, 50, 128);
        img.pixels[2] = ColorRgba::new(10, 0, 0, 255);
        let plain = premultiplied(&img, false);
        assert_eq!(
            &plain[0..4],
            &[0, 0, 0, 255],
            "black stays opaque without key"
        );
        assert_eq!(&plain[4..8], &[100, 50, 25, 128]);
        let keyed = premultiplied(&img, true);
        assert_eq!(&keyed[0..4], &[0, 0, 0, 0], "pure black keyed out");
        assert_eq!(&keyed[8..12], &[10, 0, 0, 255], "near-black kept");
    }
}
