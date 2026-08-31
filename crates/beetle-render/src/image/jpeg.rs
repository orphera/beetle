#[cfg(feature = "bga-enhanced")]
use crate::skin::ColorRgba;
#[cfg(feature = "bga-enhanced")]
use super::ImageBuffer;

#[cfg(feature = "bga-enhanced")]
/// Decodes a JPEG image from byte slice (requires bga-enhanced feature).
pub fn decode_jpeg(data: &[u8]) -> Option<ImageBuffer> {
    let mut decoder = jpeg_decoder::Decoder::new(data);
    let pixels_raw = decoder.decode().ok()?;
    let metadata = decoder.info()?;
    let width = metadata.width as u32;
    let height = metadata.height as u32;
    let mut pixels = Vec::with_capacity((width * height) as usize);

    match metadata.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            for chunk in pixels_raw.chunks_exact(3) {
                pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], 255));
            }
        }
        jpeg_decoder::PixelFormat::L8 => {
            for &b in &pixels_raw {
                pixels.push(ColorRgba::new(b, b, b, 255));
            }
        }
        jpeg_decoder::PixelFormat::L16 => {
            for chunk in pixels_raw.chunks_exact(2) {
                pixels.push(ColorRgba::new(chunk[0], chunk[0], chunk[0], 255));
            }
        }
        jpeg_decoder::PixelFormat::CMYK32 => {
            for chunk in pixels_raw.chunks_exact(4) {
                let c = chunk[0] as f32 / 255.0;
                let m = chunk[1] as f32 / 255.0;
                let y = chunk[2] as f32 / 255.0;
                let k = chunk[3] as f32 / 255.0;
                let r = ((1.0 - c) * (1.0 - k) * 255.0).round() as u8;
                let g = ((1.0 - m) * (1.0 - k) * 255.0).round() as u8;
                let b = ((1.0 - y) * (1.0 - k) * 255.0).round() as u8;
                pixels.push(ColorRgba::new(r, g, b, 255));
            }
        }
    }

    if pixels.len() == (width * height) as usize {
        Some(ImageBuffer {
            width,
            height,
            pixels,
        })
    } else {
        None
    }
}
