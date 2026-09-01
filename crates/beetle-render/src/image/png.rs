#[cfg(feature = "bga-enhanced")]
use crate::skin::ColorRgba;
#[cfg(feature = "bga-enhanced")]
use super::ImageBuffer;

#[cfg(feature = "bga-enhanced")]
/// Decodes a PNG image from byte slice (requires bga-enhanced feature).
pub fn decode_png(data: &[u8]) -> Option<ImageBuffer> {
    let mut decoder = ::png::Decoder::new(data);
    decoder.set_transformations(::png::Transformations::EXPAND | ::png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let width = info.width;
    let height = info.height;
    let (color_type, _) = reader.output_color_type();
    let mut pixels = Vec::with_capacity((width * height) as usize);

    let limit = info.buffer_size().min(buf.len());
    match color_type {
        ::png::ColorType::Rgba => {
            for chunk in buf[..limit].chunks_exact(4) {
                pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], chunk[3]));
            }
        }
        ::png::ColorType::Rgb => {
            for chunk in buf[..limit].chunks_exact(3) {
                pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], 255));
            }
        }
        ::png::ColorType::Grayscale => {
            for &b in &buf[..limit] {
                pixels.push(ColorRgba::new(b, b, b, 255));
            }
        }
        ::png::ColorType::GrayscaleAlpha => {
            for chunk in buf[..limit].chunks_exact(2) {
                pixels.push(ColorRgba::new(chunk[0], chunk[0], chunk[0], chunk[1]));
            }
        }
        _ => return None,
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
