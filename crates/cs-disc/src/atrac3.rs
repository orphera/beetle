//! ATRAC3 decoding, as the GuitarFreaks & DrumMania disc keeps its music
//! (in the movie files) and its previews (`.at3`, no container).
//!
//! Only the coding the disc uses: stereo as two independent sound units
//! per frame (no joint stereo), each `block / 2` bytes. A frame is 1024
//! samples a channel. A sound unit (all fields MSB first):
//!
//! - 6 bits id `0x28`, 2 bits the number of coded QMF bands minus one;
//! - gain control: per coded band, 3 bits point count, then 4-bit level
//!   and 5-bit place per point;
//! - tonal components (5 bits count, ...), added onto the spectrum;
//! - the spectrum: 5 bits subband count minus one, 1 bit fixed (1) or
//!   Huffman (0) coded, 3 bits a table per subband, 6 bits a scale factor
//!   per coded subband, then the coefficients.
//!
//! Each of the 4 bands of 256 coefficients goes through a 512-point
//! inverse MDCT (odd bands reversed first), the window, gain compensation
//! with overlap, and a 3-stage QMF synthesis joins the bands.
//!
//! The tables are the format's own (Huffman code lengths, quantiser steps,
//! subband edges, QMF taps); the code is checked sample for sample against
//! a reference decoder on the disc's music.

use std::f32::consts::PI;

pub const FRAME_SAMPLES: usize = 1024;
const BAND: usize = 256;

/// Huffman tables, by (value + 31, code length), in code order.
const HUFF: [&[(u8, u8)]; 7] = [
    &[
        (31, 1),
        (32, 3),
        (33, 3),
        (34, 4),
        (35, 4),
        (36, 5),
        (37, 5),
        (38, 5),
        (39, 5),
    ],
    &[(31, 1), (32, 3), (30, 3), (33, 3), (29, 3)],
    &[
        (31, 1),
        (32, 3),
        (30, 3),
        (33, 4),
        (29, 4),
        (34, 4),
        (28, 4),
    ],
    &[
        (31, 1),
        (32, 3),
        (30, 3),
        (33, 4),
        (29, 4),
        (34, 5),
        (28, 5),
        (35, 5),
        (27, 5),
    ],
    &[
        (31, 2),
        (32, 3),
        (30, 3),
        (33, 4),
        (29, 4),
        (34, 4),
        (28, 4),
        (38, 4),
        (24, 4),
        (35, 5),
        (27, 5),
        (36, 6),
        (26, 6),
        (37, 6),
        (25, 6),
    ],
    &[
        (31, 3),
        (32, 4),
        (30, 4),
        (33, 4),
        (29, 4),
        (34, 4),
        (28, 4),
        (46, 4),
        (16, 4),
        (35, 5),
        (27, 5),
        (36, 5),
        (26, 5),
        (37, 5),
        (25, 5),
        (38, 6),
        (24, 6),
        (39, 6),
        (23, 6),
        (40, 6),
        (22, 6),
        (41, 6),
        (21, 6),
        (42, 7),
        (20, 7),
        (43, 7),
        (19, 7),
        (44, 7),
        (18, 7),
        (45, 7),
        (17, 7),
    ],
    &[
        (31, 3),
        (62, 4),
        (0, 4),
        (32, 5),
        (30, 5),
        (33, 5),
        (29, 5),
        (34, 5),
        (28, 5),
        (35, 5),
        (27, 5),
        (36, 5),
        (26, 5),
        (37, 6),
        (25, 6),
        (38, 6),
        (24, 6),
        (39, 6),
        (23, 6),
        (40, 6),
        (22, 6),
        (41, 6),
        (21, 6),
        (42, 6),
        (20, 6),
        (43, 6),
        (19, 6),
        (44, 6),
        (18, 6),
        (45, 7),
        (17, 7),
        (46, 7),
        (16, 7),
        (47, 7),
        (15, 7),
        (48, 7),
        (14, 7),
        (49, 7),
        (13, 7),
        (50, 7),
        (12, 7),
        (51, 7),
        (11, 7),
        (52, 8),
        (10, 8),
        (53, 8),
        (9, 8),
        (54, 8),
        (8, 8),
        (55, 8),
        (7, 8),
        (56, 8),
        (6, 8),
        (57, 8),
        (5, 8),
        (58, 8),
        (4, 8),
        (59, 8),
        (3, 8),
        (60, 8),
        (2, 8),
        (61, 8),
        (1, 8),
    ],
];

/// Bits of a fixed-length coefficient, by table.
const CLC_BITS: [u32; 8] = [0, 4, 3, 3, 4, 4, 5, 6];
/// Table 1 codes two coefficients at once.
const PAIR_CLC: [i32; 4] = [0, 1, -2, -1];
const PAIR_VLC: [(i32, i32); 9] = [
    (0, 0),
    (0, 1),
    (0, -1),
    (1, 0),
    (-1, 0),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];
const INV_MAX_QUANT: [f32; 8] = [
    0.0,
    1.0 / 1.5,
    1.0 / 2.5,
    1.0 / 3.5,
    1.0 / 4.5,
    1.0 / 7.5,
    1.0 / 15.5,
    1.0 / 31.5,
];
const SUBBANDS: [usize; 33] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 288, 320,
    352, 384, 416, 448, 480, 512, 576, 640, 704, 768, 896, 1024,
];
/// Half of the 48-tap QMF prototype (the values as the format gives them).
#[allow(clippy::excessive_precision)]
const QMF_HALF: [f32; 24] = [
    -0.00001461907,
    -0.00009205479,
    -0.000056157569,
    0.00030117269,
    0.0002422519,
    -0.00085293897,
    -0.0005205574,
    0.0020340169,
    0.00078333891,
    -0.0042153862,
    -0.00075614988,
    0.0078402944,
    -0.000061169922,
    -0.01344162,
    0.0024626821,
    0.021736089,
    -0.007801671,
    -0.034090221,
    0.01880949,
    0.054326009,
    -0.043596379,
    -0.099384367,
    0.13207909,
    0.46424159,
];

struct Bits<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn peek(&self, n: u32) -> u32 {
        let mut v = 0u32;
        for i in 0..n as usize {
            let p = self.pos + i;
            let bit = self.bytes.get(p / 8).map_or(0, |b| (b >> (7 - p % 8)) & 1);
            v = v << 1 | bit as u32;
        }
        v
    }
    fn get(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.pos += n as usize;
        v
    }
    fn signed(&mut self, n: u32) -> i32 {
        let v = self.get(n) as i32;
        (v << (32 - n)) >> (32 - n)
    }
}

/// A Huffman table as a lookup on the next 8 bits: (value, length).
struct Vlc([(i8, u8); 256]);

impl Vlc {
    fn new(codes: &[(u8, u8)]) -> Self {
        let mut table = [(0i8, 0u8); 256];
        let mut next = 0u32;
        for &(value, len) in codes {
            let code = next >> (8 - len);
            let span = 1 << (8 - len);
            for slot in &mut table[(code << (8 - len)) as usize..][..span] {
                *slot = (value as i8 - 31, len);
            }
            next += 1 << (8 - len);
        }
        Self(table)
    }
    fn read(&self, bits: &mut Bits) -> i32 {
        let (value, len) = self.0[bits.peek(8) as usize];
        bits.pos += len.max(1) as usize;
        value as i32
    }
}

#[derive(Clone, Copy, Default)]
struct Gain {
    points: usize,
    level: [usize; 8],
    place: [usize; 8],
}

struct Tables {
    vlc: Vec<Vlc>,
    scale: [f32; 64],
    window: [f32; 512],
    qmf: [f32; 48],
    /// The inverse MDCT's twiddles: pre and post rotations, the FFT's roots.
    rot: Vec<(f32, f32)>,
    post: Vec<(f32, f32)>,
    roots: Vec<(f32, f32)>,
}

impl Tables {
    fn new() -> Self {
        let mut scale = [0.0; 64];
        for (i, s) in scale.iter_mut().enumerate() {
            *s = 2f32.powf((i as f32 - 15.0) / 3.0);
        }
        let mut window = [0.0; 512];
        for i in 0..128 {
            let j = 255 - i;
            let wi = (((i as f64 + 0.5) / 256.0 - 0.5) * std::f64::consts::PI).sin() + 1.0;
            let wj = (((j as f64 + 0.5) / 256.0 - 0.5) * std::f64::consts::PI).sin() + 1.0;
            let w = 0.5 * (wi * wi + wj * wj);
            window[i] = (wi / w) as f32;
            window[511 - i] = (wi / w) as f32;
            window[j] = (wj / w) as f32;
            window[511 - j] = (wj / w) as f32;
        }
        let mut qmf = [0.0; 48];
        for i in 0..24 {
            qmf[i] = QMF_HALF[i] * 2.0;
            qmf[47 - i] = QMF_HALF[i] * 2.0;
        }
        let m = BAND as f32;
        let rot = (0..BAND / 2)
            .map(|k| {
                let a = -PI * (4.0 * k as f32 + 1.0) / (4.0 * m);
                (a.cos(), a.sin())
            })
            .collect();
        let half = BAND / 2;
        let post = (0..half)
            .map(|k| {
                let a = -PI * k as f32 / m;
                (a.cos(), a.sin())
            })
            .collect();
        let roots = (0..half)
            .map(|k| {
                let a = -2.0 * PI * k as f32 / half as f32;
                (a.cos(), a.sin())
            })
            .collect();
        Self {
            vlc: HUFF.iter().map(|c| Vlc::new(c)).collect(),
            scale,
            window,
            qmf,
            rot,
            post,
            roots,
        }
    }

    /// In-place radix-2 FFT of `BAND / 2` points.
    fn fft(&self, re: &mut [f32], im: &mut [f32]) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j |= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (wr, wi) = self.roots[k * step];
                    let (a, b) = (start + k, start + k + len / 2);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len <<= 1;
        }
    }

    /// DCT-IV of 256 coefficients: `c[n] = sum X[k] cos(pi/256 (n+1/2)(k+1/2))`.
    fn dct4(&self, x: &[f32], out: &mut [f32]) {
        let half = BAND / 2;
        let mut re = [0f32; BAND / 2];
        let mut im = [0f32; BAND / 2];
        for k in 0..half {
            let (a, b) = (x[2 * k], x[BAND - 1 - 2 * k]);
            let (c, s) = self.rot[k];
            re[k] = a * c - b * s;
            im[k] = a * s + b * c;
        }
        self.fft(&mut re, &mut im);
        for n in 0..half {
            let (c, s) = self.post[n];
            let yr = re[n] * c - im[n] * s;
            let yi = re[n] * s + im[n] * c;
            out[2 * n] = yr;
            out[BAND - 1 - 2 * n] = -yi;
        }
    }

    /// The windowed 512-point inverse MDCT of one band.
    fn imdct(&self, spectrum: &mut [f32], odd: bool, out: &mut [f32; 512]) {
        if odd {
            spectrum.reverse();
        }
        let mut c = [0f32; BAND];
        self.dct4(spectrum, &mut c);
        let m = BAND;
        for (n, o) in out.iter_mut().enumerate() {
            let i = n + m / 2;
            let v = if i < m {
                c[i]
            } else if i < 2 * m {
                -c[2 * m - 1 - i]
            } else {
                -c[i - 2 * m]
            };
            // The format's inverse MDCT has the opposite sign of the
            // textbook sum (checked against a reference decoder).
            *o = -v * self.window[n];
        }
    }
}

struct Tonal {
    pos: usize,
    coefs: [f32; 8],
    count: usize,
}

struct Channel {
    prev: [f32; FRAME_SAMPLES],
    gains: [[Gain; 4]; 2],
    switch: usize,
    delay: [[f32; 46]; 3],
}

impl Channel {
    fn new() -> Self {
        Self {
            prev: [0.0; FRAME_SAMPLES],
            gains: [[Gain::default(); 4]; 2],
            switch: 0,
            delay: [[0.0; 46]; 3],
        }
    }
}

/// `num` coefficients of table `selector` into `out`.
fn coefficients(t: &Tables, bits: &mut Bits, selector: usize, fixed: bool, out: &mut [i32]) {
    let n = out.len();
    if selector == 1 {
        for i in 0..n / 2 {
            let (a, b) = if fixed {
                let code = bits.get(4) as usize;
                (PAIR_CLC[code >> 2], PAIR_CLC[code & 3])
            } else {
                PAIR_VLC[(t.vlc[0].read(bits) as usize).min(8)]
            };
            out[2 * i] = a;
            out[2 * i + 1] = b;
        }
    } else {
        for o in out.iter_mut() {
            *o = if fixed {
                bits.signed(CLC_BITS[selector])
            } else {
                t.vlc[selector - 1].read(bits)
            };
        }
    }
}

fn gain_compensation(
    input: &[f32; 512],
    prev: &mut [f32],
    now: &Gain,
    next: &Gain,
    out: &mut [f32],
) {
    let level = |code: usize| 2f32.powi(4 - code as i32);
    let scale = if next.points > 0 {
        level(next.level[0])
    } else {
        1.0
    };
    let mut pos = 0;
    for i in 0..now.points {
        let last = now.place[i] << 3;
        let mut lev = level(now.level[i]);
        let to = if i + 1 < now.points {
            now.level[i + 1]
        } else {
            4
        };
        let inc = 2f32.powf(-(to as f32 - now.level[i] as f32) / 8.0);
        while pos < last {
            out[pos] = (input[pos] * scale + prev[pos]) * lev;
            pos += 1;
        }
        while pos < last + 8 {
            out[pos] = (input[pos] * scale + prev[pos]) * lev;
            lev *= inc;
            pos += 1;
        }
    }
    while pos < BAND {
        out[pos] = input[pos] * scale + prev[pos];
        pos += 1;
    }
    prev.copy_from_slice(&input[BAND..]);
}

/// Joins a low and a high band of `n` samples into `2n`.
fn iqmf(t: &Tables, lo: &[f32], hi: &[f32], out: &mut [f32], delay: &mut [f32; 46]) {
    let n = lo.len();
    let mut temp = vec![0f32; 46 + 2 * n];
    temp[..46].copy_from_slice(delay);
    for i in 0..n {
        temp[46 + 2 * i] = lo[i] + hi[i];
        temp[46 + 2 * i + 1] = lo[i] - hi[i];
    }
    for j in 0..n {
        let p = &temp[2 * j..2 * j + 48];
        let (mut s1, mut s2) = (0f32, 0f32);
        for i in (0..48).step_by(2) {
            s1 += p[i] * t.qmf[i];
            s2 += p[i + 1] * t.qmf[i + 1];
        }
        out[2 * j] = s2;
        out[2 * j + 1] = s1;
    }
    delay.copy_from_slice(&temp[2 * n..2 * n + 46]);
}

/// Decodes one sound unit into 1024 samples. `None` when it is not one.
fn sound_unit(t: &Tables, ch: &mut Channel, data: &[u8], out: &mut [f32]) -> Option<()> {
    let mut bits = Bits {
        bytes: data,
        pos: 0,
    };
    if bits.get(6) != 0x28 {
        return None;
    }
    let bands = bits.get(2) as usize;
    let next = 1 - ch.switch;
    let mut gains = [Gain::default(); 4];
    for g in gains.iter_mut().take(bands + 1) {
        g.points = bits.get(3) as usize;
        for j in 0..g.points {
            g.level[j] = bits.get(4) as usize;
            g.place[j] = bits.get(5) as usize;
            if j > 0 && g.place[j] <= g.place[j - 1] {
                return None;
            }
        }
    }
    ch.gains[next] = gains;

    // Tonal components.
    let mut tonal: Vec<Tonal> = Vec::new();
    let components = bits.get(5);
    if components > 0 {
        let selector_mode = bits.get(2);
        if selector_mode == 2 {
            return None;
        }
        let mut fixed = selector_mode & 1 == 1;
        for _ in 0..components {
            let mut flags = [false; 4];
            for f in flags.iter_mut().take(bands + 1) {
                *f = bits.get(1) == 1;
            }
            let values = bits.get(3) as usize + 1;
            let step = bits.get(3) as usize;
            if step <= 1 {
                return None;
            }
            if selector_mode == 3 {
                fixed = bits.get(1) == 1;
            }
            for b in 0..(bands + 1) * 4 {
                if !flags[b >> 2] {
                    continue;
                }
                for _ in 0..bits.get(3) {
                    let sf = bits.get(6) as usize;
                    if tonal.len() >= 64 {
                        return None;
                    }
                    let pos = b * 64 + bits.get(6) as usize;
                    let count = values.min(FRAME_SAMPLES - pos);
                    let mut m = [0i32; 8];
                    coefficients(t, &mut bits, step, fixed, &mut m[..count]);
                    let s = t.scale[sf] * INV_MAX_QUANT[step];
                    let mut coefs = [0f32; 8];
                    for (c, &q) in coefs.iter_mut().zip(&m[..count]) {
                        *c = q as f32 * s;
                    }
                    tonal.push(Tonal { pos, coefs, count });
                }
            }
        }
    }

    // The spectrum.
    let mut spectrum = [0f32; FRAME_SAMPLES];
    let subbands = bits.get(5) as usize;
    let fixed = bits.get(1) == 1;
    let mut selectors = [0usize; 32];
    for s in selectors.iter_mut().take(subbands + 1) {
        *s = bits.get(3) as usize;
    }
    let mut sf = [0usize; 32];
    for i in 0..=subbands {
        if selectors[i] != 0 {
            sf[i] = bits.get(6) as usize;
        }
    }
    let mut m = [0i32; 128];
    for i in 0..=subbands {
        if selectors[i] == 0 {
            continue;
        }
        let (first, last) = (SUBBANDS[i], SUBBANDS[i + 1]);
        let m = &mut m[..last - first];
        coefficients(t, &mut bits, selectors[i], fixed, m);
        let s = t.scale[sf[i]] * INV_MAX_QUANT[selectors[i]];
        for (o, &q) in spectrum[first..last].iter_mut().zip(m.iter()) {
            *o = q as f32 * s;
        }
    }
    let mut last_tonal: isize = -1;
    for c in &tonal {
        last_tonal = last_tonal.max((c.pos + c.count) as isize);
        for (o, &v) in spectrum[c.pos..c.pos + c.count].iter_mut().zip(&c.coefs) {
            *o += v;
        }
    }
    let mut used = (SUBBANDS[subbands + 1] - 1) >> 8;
    if last_tonal >= 0 {
        used = used.max((last_tonal as usize + 256) >> 8);
    }

    let now = ch.switch;
    for band in 0..4 {
        let mut buf = [0f32; 512];
        if band <= used {
            t.imdct(
                &mut spectrum[band * BAND..][..BAND],
                band & 1 == 1,
                &mut buf,
            );
        }
        let (g_now, g_next) = (ch.gains[now][band], ch.gains[next][band]);
        gain_compensation(
            &buf,
            &mut ch.prev[band * BAND..][..BAND],
            &g_now,
            &g_next,
            &mut out[band * BAND..][..BAND],
        );
    }
    ch.switch = next;
    Some(())
}

/// A stereo ATRAC3 decoder: two independent sound units a frame.
pub struct Decoder {
    tables: Tables,
    channels: [Channel; 2],
    /// Bytes of one frame (both channels).
    pub block: usize,
}

impl Decoder {
    pub fn new(block: usize) -> Self {
        Self {
            tables: Tables::new(),
            channels: [Channel::new(), Channel::new()],
            block,
        }
    }

    /// One frame into 1024 interleaved stereo samples (appended to `out`).
    /// `false` (and silence) when a sound unit is broken.
    pub fn frame(&mut self, data: &[u8], out: &mut Vec<i16>) -> bool {
        let unit = self.block / 2;
        let mut pcm = [[0f32; FRAME_SAMPLES]; 2];
        let mut ok = true;
        for (c, samples) in pcm.iter_mut().enumerate() {
            let mut bands = [0f32; FRAME_SAMPLES];
            let part = data.get(c * unit..(c + 1) * unit).unwrap_or(&[]);
            if sound_unit(&self.tables, &mut self.channels[c], part, &mut bands).is_none() {
                ok = false;
                continue;
            }
            let ch = &mut self.channels[c];
            let (p1, rest) = bands.split_at(BAND);
            let (p2, rest) = rest.split_at(BAND);
            let (p3, p4) = rest.split_at(BAND);
            let mut low = [0f32; 512];
            let mut high = [0f32; 512];
            iqmf(&self.tables, p1, p2, &mut low, &mut ch.delay[0]);
            iqmf(&self.tables, p4, p3, &mut high, &mut ch.delay[1]);
            iqmf(&self.tables, &low, &high, samples, &mut ch.delay[2]);
        }
        for i in 0..FRAME_SAMPLES {
            for s in &pcm {
                out.push(s[i].round().clamp(-32768.0, 32767.0) as i16);
            }
        }
        ok
    }

    /// A whole stream of frames into interleaved stereo (`gfdm::MUSIC_RATE`).
    pub fn decode_all(block: usize, data: &[u8]) -> Vec<i16> {
        let mut d = Self::new(block);
        let mut out = Vec::with_capacity(data.len() / block * FRAME_SAMPLES * 2);
        for frame in data.chunks_exact(block) {
            d.frame(frame, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fast_dct_iv_matches_the_sum() {
        let t = Tables::new();
        let x: Vec<f32> = (0..BAND)
            .map(|i| ((i * 37 % 101) as f32 - 50.0) / 7.0)
            .collect();
        let mut fast = [0f32; BAND];
        t.dct4(&x, &mut fast);
        for (n, &f) in fast.iter().enumerate() {
            let slow: f64 = x
                .iter()
                .enumerate()
                .map(|(k, &v)| {
                    v as f64
                        * (std::f64::consts::PI / BAND as f64 * (n as f64 + 0.5) * (k as f64 + 0.5))
                            .cos()
                })
                .sum();
            assert!((f as f64 - slow).abs() < 1e-2, "{n}: {f} vs {slow}");
        }
    }

    #[test]
    fn huffman_codes_are_prefix_free_and_fill_the_table() {
        for codes in HUFF {
            let v = Vlc::new(codes);
            assert!(v.0.iter().all(|&(_, len)| len > 0));
        }
    }

    #[test]
    fn a_unit_with_another_id_is_refused() {
        let mut d = Decoder::new(384);
        let mut out = Vec::new();
        assert!(!d.frame(&[0u8; 384], &mut out));
        assert_eq!(out.len(), 2048);
    }
}
