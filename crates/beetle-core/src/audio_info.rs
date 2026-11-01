//! What an audio file's header says about its quality, read without decoding
//! (B17, plan P2.1): WAV, FLAC, Ogg Vorbis and MP3.

/// The quality of one audio file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioInfo {
    /// What the bytes are, whatever the file is named: `WAV`, `FLAC`, `OGG` or `MP3`.
    pub format: &'static str,
    /// WAV PCM and FLAC; Ogg and MP3 are lossy.
    pub lossless: bool,
    pub sample_rate: u32,
    /// Bits per sample of a lossless file (0 for lossy).
    pub bits: u16,
    pub channels: u16,
    /// Nominal bitrate of a lossy file in kbit/s (0 when unknown or lossless).
    pub kbps: u32,
}

/// How many bytes of a file `probe` needs at most.
pub const PROBE_BYTES: usize = 64 * 1024;

/// Reads the header at the start of a file.
pub fn probe(bytes: &[u8]) -> Option<AudioInfo> {
    match bytes.get(..4)? {
        b"RIFF" => wav(bytes),
        b"fLaC" => flac(bytes),
        b"OggS" => vorbis(bytes),
        _ => mp3(bytes),
    }
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn wav(b: &[u8]) -> Option<AudioInfo> {
    if b.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut at = 12;
    while at + 8 <= b.len() {
        let size = u32_at(b, at + 4)? as usize;
        if &b[at..at + 4] == b"fmt " {
            let f = at + 8;
            let tag = u16_at(b, f)?;
            let lossless = matches!(tag, 1 | 3 | 0xFFFE);
            return Some(AudioInfo {
                // A WAV file may hold a compressed stream: name the codec.
                format: match tag {
                    0x55 => "MP3",
                    0x674f..=0x6771 => "OGG",
                    2 | 0x11 => "ADPCM",
                    _ => "WAV",
                },
                lossless,
                channels: u16_at(b, f + 2)?,
                sample_rate: u32_at(b, f + 4)?,
                bits: if lossless { u16_at(b, f + 14)? } else { 0 },
                kbps: if lossless {
                    0
                } else {
                    u32_at(b, f + 8)? * 8 / 1000
                },
            });
        }
        at += 8 + size + (size & 1);
    }
    None
}

fn flac(b: &[u8]) -> Option<AudioInfo> {
    // STREAMINFO is the first metadata block: 4-byte block header, then
    // 10 bytes of block and frame sizes, then rate (20 bits), channels-1
    // (3 bits), bits-1 (5 bits).
    let s = b.get(18..22)?;
    let rate = (u32::from(s[0]) << 12) | (u32::from(s[1]) << 4) | (u32::from(s[2]) >> 4);
    Some(AudioInfo {
        format: "FLAC",
        lossless: true,
        sample_rate: rate,
        channels: u16::from((s[2] >> 1) & 0x7) + 1,
        bits: ((u16::from(s[2] & 1) << 4) | u16::from(s[3] >> 4)) + 1,
        kbps: 0,
    })
}

fn vorbis(b: &[u8]) -> Option<AudioInfo> {
    // The first page holds the identification packet alone.
    let segments = *b.get(26)? as usize;
    let p = 27 + segments;
    if b.get(p..p + 7)? != b"\x01vorbis" {
        return None;
    }
    let nominal = u32_at(b, p + 20)? as i32;
    Some(AudioInfo {
        format: "OGG",
        lossless: false,
        channels: u16::from(*b.get(p + 11)?),
        sample_rate: u32_at(b, p + 12)?,
        bits: 0,
        kbps: nominal.max(0) as u32 / 1000,
    })
}

fn mp3(b: &[u8]) -> Option<AudioInfo> {
    let mut at = 0;
    if b.get(..3)? == b"ID3" {
        let s = b.get(6..10)?;
        let size = s
            .iter()
            .fold(0usize, |n, &x| (n << 7) | usize::from(x & 0x7f));
        at = 10 + size;
    }
    let h = b.get(at..at + 4)?;
    if h[0] != 0xff || h[1] & 0xe0 != 0xe0 {
        return None;
    }
    let version = (h[1] >> 3) & 3; // 3: MPEG-1, 2: MPEG-2, 0: MPEG-2.5
    if (h[1] >> 1) & 3 != 1 || version == 1 {
        return None; // not layer III
    }
    const V1: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const V2: [u32; 16] = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    let index = usize::from(h[2] >> 4);
    let base = [44100, 48000, 32000, 0][usize::from((h[2] >> 2) & 3)];
    let (kbps, rate) = match version {
        3 => (V1[index], base),
        2 => (V2[index], base / 2),
        _ => (V2[index], base / 4),
    };
    Some(AudioInfo {
        format: "MP3",
        lossless: false,
        sample_rate: rate,
        bits: 0,
        channels: if h[3] >> 6 == 3 { 1 } else { 2 },
        kbps,
    })
}

impl AudioInfo {
    /// For people: `WAV 44.1kHz/16bit`, `OGG 44.1kHz 192kbps`.
    pub fn describe(&self) -> String {
        let khz = f64::from(self.sample_rate) / 1000.0;
        if self.lossless {
            format!("{} {khz}kHz/{}bit", self.format, self.bits)
        } else if self.kbps > 0 {
            format!("{} {khz}kHz {}kbps", self.format, self.kbps)
        } else {
            format!("{} {khz}kHz", self.format)
        }
    }
}

/// Whether `a` is no worse than `b` in anything and better in something:
/// lossless over lossy, then sample rate, channels, and bit depth (both
/// lossless) or bitrate (both lossy, when both are known).
pub fn better(a: &AudioInfo, b: &AudioInfo) -> bool {
    let mut pairs = vec![
        (u32::from(a.lossless), u32::from(b.lossless)),
        (a.sample_rate, b.sample_rate),
        (u32::from(a.channels), u32::from(b.channels)),
    ];
    if a.lossless && b.lossless {
        pairs.push((u32::from(a.bits), u32::from(b.bits)));
    }
    if !a.lossless && !b.lossless && a.kbps > 0 && b.kbps > 0 {
        pairs.push((a.kbps, b.kbps));
    }
    pairs.iter().all(|(x, y)| x >= y) && pairs.iter().any(|(x, y)| x > y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_header(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
        b.extend(b"LIST\x04\0\0\0abcd");
        b.extend(b"fmt \x10\0\0\0");
        b.extend(tag.to_le_bytes());
        b.extend(channels.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * u32::from(channels) * u32::from(bits) / 8).to_le_bytes());
        b.extend((channels * bits / 8).to_le_bytes());
        b.extend(bits.to_le_bytes());
        b
    }

    fn ogg_header(channels: u8, rate: u32, nominal: u32) -> Vec<u8> {
        let mut b = b"OggS".to_vec();
        b.extend([0u8; 22]);
        b.push(1); // one segment
        b.push(30);
        b.extend(b"\x01vorbis\0\0\0\0");
        b.push(channels);
        b.extend(rate.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(nominal.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b
    }

    #[test]
    fn headers_are_read() {
        let wav = probe(&wav_header(1, 2, 44100, 16)).unwrap();
        assert_eq!(
            (wav.lossless, wav.sample_rate, wav.bits, wav.channels),
            (true, 44100, 16, 2)
        );
        let ogg = probe(&ogg_header(2, 22050, 128_000)).unwrap();
        assert_eq!(
            (ogg.lossless, ogg.sample_rate, ogg.kbps),
            (false, 22050, 128)
        );
        let mut flac = b"fLaC\0\0\0\x22".to_vec();
        flac.extend([0u8; 10]);
        // 44100 Hz, 2 channels, 24 bits.
        flac.extend([0x0a, 0xc4, 0x43, 0x70]);
        let f = probe(&flac).unwrap();
        assert_eq!((f.sample_rate, f.channels, f.bits), (44100, 2, 24));
        let mp3 = probe(&[0xff, 0xfb, 0x90, 0x00]).unwrap();
        assert_eq!((mp3.sample_rate, mp3.kbps, mp3.channels), (44100, 128, 2));
        assert_eq!(probe(b"junk"), None);
    }

    #[test]
    fn lossless_beats_lossy_but_not_at_a_lower_rate() {
        let wav = probe(&wav_header(1, 2, 44100, 16)).unwrap();
        let ogg = probe(&ogg_header(2, 44100, 192_000)).unwrap();
        let ogg_hi = probe(&ogg_header(2, 48000, 320_000)).unwrap();
        let ogg_low = probe(&ogg_header(2, 22050, 64_000)).unwrap();
        assert!(better(&wav, &ogg));
        assert!(!better(&ogg, &wav));
        assert!(!better(&wav, &ogg_hi));
        assert!(better(&ogg, &ogg_low));
        assert!(!better(&wav, &wav));
    }
}
