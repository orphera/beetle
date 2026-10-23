//! MPEG-1 / MPEG-2 video (ISO/IEC 11172-2, 13818-2 main profile, 4:2:0
//! frame pictures) to frames in display order.
//!
//! Not handled: field pictures (`picture_structure` other than frame), dual
//! prime prediction, 4:2:2 and 4:4:4, scalable extensions. A stream that
//! needs them ends with `Error::Unsupported`.

use std::fmt;

use crate::bits::Bits;
use crate::idct::idct;
use crate::vlc::{self, Coef, Vlc, BACKWARD, FORWARD, INTRA, PATTERN, QUANT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The stream breaks the syntax (a bad code, a block past 64 coefficients).
    Corrupt(&'static str),
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Corrupt(what) => write!(f, "corrupt MPEG video: {what}"),
            Error::Unsupported(what) => write!(f, "unsupported MPEG video: {what}"),
        }
    }
}

const SEQUENCE_HEADER: u8 = 0xb3;
const EXTENSION: u8 = 0xb5;
const SEQUENCE_END: u8 = 0xb7;
const GROUP: u8 = 0xb8;
const PICTURE: u8 = 0x00;

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
const ALTERNATE: [usize; 64] = [
    0, 8, 16, 24, 1, 9, 2, 10, 17, 25, 32, 40, 48, 56, 57, 49, 41, 33, 26, 18, 3, 11, 4, 12, 19,
    27, 34, 42, 50, 58, 35, 43, 51, 59, 20, 28, 5, 13, 6, 14, 21, 29, 36, 44, 52, 60, 37, 45, 53,
    61, 22, 30, 7, 15, 23, 31, 38, 46, 54, 62, 39, 47, 55, 63,
];
/// The default intra quantiser matrix, raster order.
const DEFAULT_INTRA: [u8; 64] = [
    8, 16, 19, 22, 26, 27, 29, 34, 16, 16, 22, 24, 27, 29, 34, 37, 19, 22, 26, 27, 29, 34, 34, 38,
    22, 22, 26, 27, 29, 34, 37, 40, 22, 26, 27, 29, 32, 35, 40, 48, 26, 27, 29, 32, 35, 40, 48, 58,
    26, 27, 29, 34, 38, 46, 56, 69, 27, 29, 35, 38, 46, 56, 69, 83,
];
/// MPEG-2 `q_scale_type` 1.
const NON_LINEAR_SCALE: [i32; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 18, 20, 22, 24, 28, 32, 36, 40, 44, 48, 52, 56, 64,
    72, 80, 88, 96, 104, 112,
];
/// Frames per second by `frame_rate_code`.
const FRAME_RATES: [f64; 9] = [
    0.0,
    24000.0 / 1001.0,
    24.0,
    25.0,
    30000.0 / 1001.0,
    30.0,
    50.0,
    60000.0 / 1001.0,
    60.0,
];

/// A decoded picture: 4:2:0 planes, padded to whole macroblocks.
#[derive(Clone)]
pub struct Frame {
    /// The visible size.
    pub width: u32,
    pub height: u32,
    /// Plane widths (luma; chroma is half).
    stride: usize,
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
}

impl Frame {
    fn new(width: u32, height: u32) -> Self {
        let stride = (width as usize).div_ceil(16) * 16;
        let rows = (height as usize).div_ceil(16) * 16;
        Self {
            width,
            height,
            stride,
            y: vec![0; stride * rows],
            cb: vec![128; stride / 2 * rows / 2],
            cr: vec![128; stride / 2 * rows / 2],
        }
    }

    /// Luma at (x, y).
    pub fn luma(&self, x: usize, y: usize) -> u8 {
        self.y[y * self.stride + x]
    }

    /// The visible picture as RGBA (BT.601, limited range).
    pub fn to_rgba(&self) -> Vec<[u8; 4]> {
        let (w, h) = (self.width as usize, self.height as usize);
        let cs = self.stride / 2;
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let l = 298 * (self.y[y * self.stride + x] as i32 - 16);
                let u = self.cb[(y / 2) * cs + x / 2] as i32 - 128;
                let v = self.cr[(y / 2) * cs + x / 2] as i32 - 128;
                let c = |n: i32| ((n + 128) >> 8).clamp(0, 255) as u8;
                out.push([
                    c(l + 409 * v),
                    c(l - 100 * u - 208 * v),
                    c(l + 516 * u),
                    255,
                ]);
            }
        }
        out
    }

    /// The planes cut to the visible size, Y then Cb then Cr (`yuv420p`).
    pub fn to_yuv420(&self) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let cs = self.stride / 2;
        let mut out = Vec::with_capacity(w * h * 3 / 2);
        for y in 0..h {
            out.extend_from_slice(&self.y[y * self.stride..y * self.stride + w]);
        }
        for plane in [&self.cb, &self.cr] {
            for y in 0..h.div_ceil(2) {
                out.extend_from_slice(&plane[y * cs..y * cs + w.div_ceil(2)]);
            }
        }
        out
    }
}

#[derive(Clone)]
struct Sequence {
    width: u32,
    height: u32,
    frame_rate: f64,
    intra_matrix: [i32; 64],
    non_intra_matrix: [i32; 64],
    mpeg2: bool,
    progressive: bool,
}

/// The coding parameters of one picture.
#[derive(Clone, Copy)]
struct PictureParams {
    kind: u8,
    /// `f_code[direction][x or y]`; MPEG-1 sets both of a direction alike.
    f_code: [[u32; 2]; 2],
    full_pel: [bool; 2],
    intra_dc_precision: u32,
    structure: u32,
    frame_pred_frame_dct: bool,
    concealment_vectors: bool,
    q_scale_type: bool,
    intra_vlc_format: bool,
    alternate_scan: bool,
}

struct Tables {
    address: Vlc<u8>,
    mb_type: [Vlc<u8>; 3],
    cbp: Vlc<u8>,
    motion: Vlc<i8>,
    dc_luma: Vlc<u8>,
    dc_chroma: Vlc<u8>,
    coef: [Vlc<Coef>; 2],
}

impl Tables {
    fn new() -> Self {
        Self {
            address: vlc::address_increment(),
            mb_type: [
                vlc::macroblock_type(1),
                vlc::macroblock_type(2),
                vlc::macroblock_type(3),
            ],
            cbp: vlc::coded_block_pattern(),
            motion: vlc::motion_code(),
            dc_luma: vlc::dc_size(false),
            dc_chroma: vlc::dc_size(true),
            coef: [vlc::dct_coefficients(false), vlc::dct_coefficients(true)],
        }
    }
}

/// Decodes a video elementary stream (see `ps::video_stream` for `.mpg`).
pub struct Decoder {
    es: Vec<u8>,
    pos: usize,
    tables: Tables,
    sequence: Option<Sequence>,
    /// The older and newer reference pictures.
    past: Option<Frame>,
    future: Option<Frame>,
    /// Whether `future` is still to be shown.
    future_pending: bool,
    done: bool,
}

/// The next start code at or after `from`: (position of its 00 00 01, code).
fn next_start_code(data: &[u8], from: usize) -> Option<(usize, u8)> {
    let mut i = from;
    while i + 3 < data.len() {
        if data[i + 2] > 1 {
            i += 3;
        } else if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            return Some((i, data[i + 3]));
        } else {
            i += 1;
        }
    }
    None
}

fn read_matrix(bits: &mut Bits) -> [i32; 64] {
    let mut m = [0i32; 64];
    for &at in &ZIGZAG {
        m[at] = bits.read(8) as i32;
    }
    m
}

impl Decoder {
    pub fn new(es: Vec<u8>) -> Self {
        Self {
            es,
            pos: 0,
            tables: Tables::new(),
            sequence: None,
            past: None,
            future: None,
            future_pending: false,
            done: false,
        }
    }

    /// The picture size, once the first sequence header is read.
    pub fn size(&mut self) -> Option<(u32, u32)> {
        self.ensure_sequence();
        self.sequence.as_ref().map(|s| (s.width, s.height))
    }

    pub fn frame_rate(&mut self) -> Option<f64> {
        self.ensure_sequence();
        self.sequence.as_ref().map(|s| s.frame_rate)
    }

    fn ensure_sequence(&mut self) {
        if self.sequence.is_some() {
            return;
        }
        let mut at = 0;
        while let Some((pos, code)) = next_start_code(&self.es, at) {
            if code == SEQUENCE_HEADER {
                self.read_sequence_header(pos + 4);
                // Its extension, if any, says MPEG-2.
                if let Some((p, EXTENSION)) = next_start_code(&self.es, pos + 4) {
                    let _ = self.read_extension(p + 4, None);
                }
                return;
            }
            at = pos + 4;
        }
    }

    fn read_sequence_header(&mut self, at: usize) {
        let mut b = Bits::at(&self.es, at);
        let width = b.read(12);
        let height = b.read(12);
        b.skip(4); // aspect ratio
        let rate = b.read(4) as usize;
        b.skip(18 + 1 + 10 + 1); // bit rate, marker, VBV buffer, constrained
        let intra = if b.bit() {
            read_matrix(&mut b)
        } else {
            DEFAULT_INTRA.map(|v| v as i32)
        };
        let non_intra = if b.bit() {
            read_matrix(&mut b)
        } else {
            [16; 64]
        };
        let mpeg2 = self.sequence.as_ref().is_some_and(|s| s.mpeg2);
        let progressive = self.sequence.as_ref().is_none_or(|s| s.progressive);
        self.sequence = Some(Sequence {
            width,
            height,
            frame_rate: FRAME_RATES
                .get(rate)
                .copied()
                .filter(|&r| r > 0.0)
                .unwrap_or(30.0),
            intra_matrix: intra,
            non_intra_matrix: non_intra,
            mpeg2,
            progressive,
        });
    }

    /// Reads an extension; picture coding ones go into `picture`.
    fn read_extension(
        &mut self,
        at: usize,
        picture: Option<&mut PictureParams>,
    ) -> Result<(), Error> {
        let mut b = Bits::at(&self.es, at);
        match b.read(4) {
            1 => {
                b.skip(8); // profile and level
                let progressive = b.bit();
                if b.read(2) != 1 {
                    return Err(Error::Unsupported("chroma format other than 4:2:0"));
                }
                if let Some(s) = &mut self.sequence {
                    s.mpeg2 = true;
                    s.progressive = progressive;
                }
            }
            3 => {
                if let Some(s) = &mut self.sequence {
                    if b.bit() {
                        s.intra_matrix = read_matrix(&mut b);
                    }
                    if b.bit() {
                        s.non_intra_matrix = read_matrix(&mut b);
                    }
                }
            }
            8 => {
                if let Some(p) = picture {
                    p.f_code = [[b.read(4), b.read(4)], [b.read(4), b.read(4)]];
                    p.intra_dc_precision = b.read(2);
                    p.structure = b.read(2);
                    b.skip(1); // top field first
                    p.frame_pred_frame_dct = b.bit();
                    p.concealment_vectors = b.bit();
                    p.q_scale_type = b.bit();
                    p.intra_vlc_format = b.bit();
                    p.alternate_scan = b.bit();
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The next frame in display order; `None` at the end of the stream.
    pub fn next_frame(&mut self) -> Option<Result<Frame, Error>> {
        loop {
            if self.done {
                // Show the last reference picture.
                if self.future_pending {
                    self.future_pending = false;
                    return self.future.clone().map(Ok);
                }
                return None;
            }
            let Some((pos, code)) = next_start_code(&self.es, self.pos) else {
                self.done = true;
                continue;
            };
            self.pos = pos + 4;
            match code {
                SEQUENCE_HEADER => {
                    self.read_sequence_header(pos + 4);
                }
                EXTENSION => {
                    if let Err(e) = self.read_extension(pos + 4, None) {
                        self.done = true;
                        return Some(Err(e));
                    }
                }
                SEQUENCE_END => self.done = true,
                GROUP => {}
                PICTURE => match self.decode_picture(pos + 4) {
                    Ok(Some(frame)) => return Some(Ok(frame)),
                    Ok(None) => {}
                    Err(e) => {
                        self.done = true;
                        return Some(Err(e));
                    }
                },
                _ => {}
            }
        }
    }

    /// Decodes the picture whose header starts at `at`; returns the frame
    /// now due for display, if any.
    fn decode_picture(&mut self, at: usize) -> Result<Option<Frame>, Error> {
        let Some(seq) = self.sequence.clone() else {
            return Err(Error::Corrupt("picture before a sequence header"));
        };
        let mut b = Bits::at(&self.es, at);
        b.skip(10); // temporal reference
        let kind = b.read(3) as u8;
        b.skip(16); // VBV delay
        let mut params = PictureParams {
            kind,
            f_code: [[15, 15], [15, 15]],
            full_pel: [false, false],
            intra_dc_precision: 0,
            structure: 3,
            frame_pred_frame_dct: true,
            concealment_vectors: false,
            q_scale_type: false,
            intra_vlc_format: false,
            alternate_scan: false,
        };
        if kind == 2 || kind == 3 {
            params.full_pel[0] = b.bit();
            let f = b.read(3);
            params.f_code[0] = [f, f];
        }
        if kind == 3 {
            params.full_pel[1] = b.bit();
            let f = b.read(3);
            params.f_code[1] = [f, f];
        }
        if !(1..=3).contains(&kind) {
            // D pictures and reserved types: skip.
            return Ok(None);
        }
        // Extensions and user data until the first slice.
        let mut cursor = at;
        let first_slice = loop {
            let Some((pos, code)) = next_start_code(&self.es, cursor) else {
                return Ok(None);
            };
            match code {
                EXTENSION => {
                    self.read_extension(pos + 4, Some(&mut params))?;
                }
                0x01..=0xaf => break pos,
                0xb2 => {} // user data
                _ => return Ok(None),
            }
            cursor = pos + 4;
        };
        if params.structure != 3 {
            return Err(Error::Unsupported("field pictures"));
        }
        if kind == 3 && (self.past.is_none() || self.future.is_none()) {
            // A B picture of an open GOP at the start: its earlier reference
            // is not in the stream. Show the later one in its place, so the
            // frame count, and the timing with it, stays right.
            self.pos = self.skip_slices(first_slice);
            return Ok(self.future.clone());
        }
        if kind == 2 && self.future.is_none() {
            self.pos = self.skip_slices(first_slice);
            return Ok(None);
        }

        let mut frame = Frame::new(seq.width, seq.height);
        let (past, future) = match kind {
            2 => (self.future.as_ref(), None),
            3 => (self.past.as_ref(), self.future.as_ref()),
            _ => (None, None),
        };
        let mut slices = SliceDecoder {
            seq: &seq,
            params: &params,
            tables: &self.tables,
            forward: past,
            backward: future,
            frame: &mut frame,
        };
        let mut pos = first_slice;
        while let Some((p, code)) = next_start_code(&self.es, pos) {
            if !(0x01..=0xaf).contains(&code) {
                pos = p;
                break;
            }
            let end = next_start_code(&self.es, p + 4).map_or(self.es.len(), |(e, _)| e);
            slices.decode_slice(&self.es[..end], p + 4, code as usize - 1)?;
            pos = end;
        }
        self.pos = pos;

        if kind == 3 {
            return Ok(Some(frame));
        }
        // An I or P picture: the newer reference is due now.
        let shown = if self.future_pending {
            self.future.take()
        } else {
            None
        };
        self.past = shown.clone().or_else(|| self.future.take());
        self.future = Some(frame);
        self.future_pending = true;
        Ok(shown)
    }

    fn skip_slices(&self, mut pos: usize) -> usize {
        while let Some((p, code)) = next_start_code(&self.es, pos) {
            if !(0x01..=0xaf).contains(&code) {
                return p;
            }
            pos = p + 4;
        }
        self.es.len()
    }
}

/// Motion state across the macroblocks of a slice.
#[derive(Clone, Copy, Default)]
struct Motion {
    /// `PMV[r][s][t]`: r first or second vector, s forward or backward, t x or y.
    pmv: [[[i32; 2]; 2]; 2],
    /// The previous macroblock's prediction, for skipped B macroblocks.
    last_type: u8,
    last_field: bool,
    /// Vectors in half pels, and field selects, of the previous macroblock.
    vectors: [[[i32; 2]; 2]; 2],
    selects: [[bool; 2]; 2],
}

struct SliceDecoder<'a> {
    seq: &'a Sequence,
    params: &'a PictureParams,
    tables: &'a Tables,
    forward: Option<&'a Frame>,
    backward: Option<&'a Frame>,
    frame: &'a mut Frame,
}

impl SliceDecoder<'_> {
    fn qscale(&self, code: u32) -> i32 {
        if self.seq.mpeg2 && self.params.q_scale_type {
            NON_LINEAR_SCALE[code as usize]
        } else {
            code as i32 * 2
        }
    }

    fn decode_slice(&mut self, es: &[u8], at: usize, row: usize) -> Result<(), Error> {
        let mb_width = self.frame.stride / 16;
        let mb_height = self.frame.y.len() / self.frame.stride / 16;
        if row >= mb_height {
            return Ok(());
        }
        let mut b = Bits::at(es, at);
        let mut qscale = self.qscale(b.read(5));
        while b.bit() {
            b.skip(8); // extra slice information
        }
        let dc_reset = 1 << (7 + self.params.intra_dc_precision);
        let mut dc = [dc_reset; 3];
        let mut motion = Motion::default();
        let mut address = row * mb_width;
        let mut first = true;
        loop {
            // Macroblock address increment, with escapes and stuffing.
            let mut increment = 0usize;
            loop {
                match self.tables.address.read(&mut b) {
                    Some(34) => increment += 33,
                    Some(0) => {}
                    Some(n) if n <= 33 => {
                        increment += n as usize;
                        break;
                    }
                    _ => return Err(Error::Corrupt("macroblock address increment")),
                }
            }
            if first {
                address += increment - 1;
                first = false;
            } else {
                // Skipped macroblocks.
                for _ in 1..increment {
                    address += 1;
                    self.skipped(address, mb_width, &mut motion)?;
                    dc = [dc_reset; 3];
                }
                address += 1;
            }
            if address >= mb_width * mb_height {
                return Ok(());
            }
            let kind = (self.params.kind - 1) as usize;
            let mb_type = self.tables.mb_type[kind]
                .read(&mut b)
                .ok_or(Error::Corrupt("macroblock type"))?;
            let moves = mb_type & (FORWARD | BACKWARD) != 0;
            let mut field_motion = false;
            if moves && !self.params.frame_pred_frame_dct {
                match b.read(2) {
                    1 => field_motion = true,
                    2 => {}
                    _ => return Err(Error::Unsupported("dual prime prediction")),
                }
            }
            let mut field_dct = false;
            if mb_type & (INTRA | PATTERN) != 0 && !self.params.frame_pred_frame_dct {
                field_dct = b.bit();
            }
            if mb_type & QUANT != 0 {
                qscale = self.qscale(b.read(5));
            }
            let (mbx, mby) = (address % mb_width, address / mb_width);

            if mb_type & INTRA != 0 {
                if self.params.concealment_vectors {
                    self.vector(&mut b, &mut motion, 0, 0, false)?;
                    b.skip(1);
                } else {
                    motion.pmv = Default::default();
                }
                let mut blocks = [[0i32; 64]; 6];
                for (k, block) in blocks.iter_mut().enumerate() {
                    self.intra_block(&mut b, k, block, &mut dc, qscale)?;
                }
                self.put_blocks(&mut blocks, mbx, mby, field_dct, None);
                motion.last_type = mb_type;
                continue_end(&b)?;
                if self.end_of_slice(&b) {
                    return Ok(());
                }
                continue;
            }
            dc = [dc_reset; 3];

            // Motion vectors.
            if self.params.kind == 2 && mb_type & FORWARD == 0 {
                // P without motion: zero vector.
                motion.pmv = Default::default();
                motion.vectors = Default::default();
                motion.selects = Default::default();
                field_motion = false;
            }
            for s in 0..2 {
                let flag = if s == 0 { FORWARD } else { BACKWARD };
                if mb_type & flag == 0 {
                    continue;
                }
                if field_motion {
                    for r in 0..2 {
                        motion.selects[r][s] = b.bit();
                        self.vector(&mut b, &mut motion, r, s, true)?;
                    }
                } else {
                    self.vector(&mut b, &mut motion, 0, s, false)?;
                    motion.pmv[1][s] = motion.pmv[0][s];
                }
            }
            let directions = if self.params.kind == 2 {
                mb_type | FORWARD
            } else {
                mb_type
            };
            motion.last_type = directions;
            motion.last_field = field_motion;
            self.predict(mbx, mby, directions, field_motion, &motion);

            if mb_type & PATTERN != 0 {
                let cbp = self
                    .tables
                    .cbp
                    .read(&mut b)
                    .ok_or(Error::Corrupt("coded block pattern"))?;
                let mut blocks = [[0i32; 64]; 6];
                let mut coded = [false; 6];
                for k in 0..6 {
                    if cbp & (32 >> k) != 0 {
                        self.non_intra_block(&mut b, &mut blocks[k], k, qscale)?;
                        coded[k] = true;
                    }
                }
                self.put_blocks(&mut blocks, mbx, mby, field_dct, Some(coded));
            }
            continue_end(&b)?;
            if self.end_of_slice(&b) {
                return Ok(());
            }
        }
    }

    /// No more macroblocks: only zero bits up to the next start code remain.
    fn end_of_slice(&self, b: &Bits) -> bool {
        b.bits_left() <= 0 || b.peek(23) == 0
    }

    /// One motion vector (both components) of vector `r`, direction `s`.
    fn vector(
        &self,
        b: &mut Bits,
        motion: &mut Motion,
        r: usize,
        s: usize,
        field: bool,
    ) -> Result<(), Error> {
        for t in 0..2 {
            let f_code = self.params.f_code[s][t];
            let r_size = f_code.saturating_sub(1);
            let f = 1i32 << r_size;
            let code = self
                .tables
                .motion
                .read(b)
                .filter(|&c| c != i8::MIN)
                .ok_or(Error::Corrupt("motion code"))? as i32;
            let delta = if f == 1 || code == 0 {
                code
            } else {
                let residual = b.read(r_size) as i32;
                let magnitude = (code.abs() - 1) * f + residual + 1;
                if code < 0 {
                    -magnitude
                } else {
                    magnitude
                }
            };
            let vertical_field = field && t == 1;
            let mut pred = motion.pmv[r][s][t];
            if vertical_field {
                pred >>= 1;
            }
            let (low, high, range) = (-16 * f, 16 * f - 1, 32 * f);
            let mut v = pred + delta;
            if v < low {
                v += range;
            } else if v > high {
                v -= range;
            }
            motion.pmv[r][s][t] = if vertical_field { v * 2 } else { v };
            // In half pels (MPEG-1 full-pel vectors count whole pels).
            motion.vectors[r][s][t] = if self.params.full_pel[s] { v * 2 } else { v };
        }
        Ok(())
    }

    /// A skipped macroblock: P copies the reference, B repeats the last prediction.
    fn skipped(
        &mut self,
        address: usize,
        mb_width: usize,
        motion: &mut Motion,
    ) -> Result<(), Error> {
        let (mbx, mby) = (address % mb_width, address / mb_width);
        match self.params.kind {
            2 => {
                motion.pmv = Default::default();
                motion.vectors = Default::default();
                motion.last_type = FORWARD;
                motion.last_field = false;
                self.predict(mbx, mby, FORWARD, false, motion);
            }
            3 => {
                if motion.last_type & INTRA != 0 || motion.last_type & (FORWARD | BACKWARD) == 0 {
                    return Err(Error::Corrupt("skipped macroblock after an intra one"));
                }
                // A skipped macroblock of a frame picture is predicted frame
                // based, by the motion vector predictors (13818-2 7.6.6.4),
                // even after a macroblock with field motion.
                motion.last_field = false;
                for s in 0..2 {
                    motion.vectors[0][s] = motion.pmv[0][s];
                }
                self.predict(mbx, mby, motion.last_type, motion.last_field, motion);
            }
            _ => return Err(Error::Corrupt("skipped macroblock in an I picture")),
        }
        Ok(())
    }

    /// Writes the motion-compensated prediction of a macroblock.
    fn predict(&mut self, mbx: usize, mby: usize, directions: u8, field: bool, motion: &Motion) {
        let refs = [self.forward, self.backward];
        let both = directions & FORWARD != 0 && directions & BACKWARD != 0;
        let mut first = true;
        for (s, reference) in refs.into_iter().enumerate() {
            let flag = if s == 0 { FORWARD } else { BACKWARD };
            if directions & flag == 0 {
                continue;
            }
            let Some(reference) = reference else { continue };
            let average = both && !first;
            if field {
                for r in 0..2 {
                    let v = motion.vectors[r][s];
                    let src_parity = motion.selects[r][s] as usize;
                    mc(
                        self.frame,
                        reference,
                        Plane::Y,
                        mbx * 16,
                        mby * 8,
                        v,
                        16,
                        8,
                        (r, src_parity),
                        average,
                    );
                    let cv = [v[0] / 2, v[1] / 2];
                    for plane in [Plane::Cb, Plane::Cr] {
                        mc(
                            self.frame,
                            reference,
                            plane,
                            mbx * 8,
                            mby * 4,
                            cv,
                            8,
                            4,
                            (r, src_parity),
                            average,
                        );
                    }
                }
            } else {
                let v = motion.vectors[0][s];
                mc(
                    self.frame,
                    reference,
                    Plane::Y,
                    mbx * 16,
                    mby * 16,
                    v,
                    16,
                    16,
                    FRAME,
                    average,
                );
                let cv = [v[0] / 2, v[1] / 2];
                for plane in [Plane::Cb, Plane::Cr] {
                    mc(
                        self.frame,
                        reference,
                        plane,
                        mbx * 8,
                        mby * 8,
                        cv,
                        8,
                        8,
                        FRAME,
                        average,
                    );
                }
            }
            first = false;
        }
    }

    fn scan(&self) -> &'static [usize; 64] {
        if self.params.alternate_scan {
            &ALTERNATE
        } else {
            &ZIGZAG
        }
    }

    fn intra_block(
        &self,
        b: &mut Bits,
        k: usize,
        block: &mut [i32; 64],
        dc: &mut [i32; 3],
        qscale: i32,
    ) -> Result<(), Error> {
        let (table, component) = if k < 4 {
            (&self.tables.dc_luma, 0)
        } else {
            (&self.tables.dc_chroma, k - 3)
        };
        let size = table.read(b).ok_or(Error::Corrupt("DC size"))? as u32;
        let diff = if size == 0 {
            0
        } else {
            let v = b.read(size) as i32;
            if v < 1 << (size - 1) {
                v - (1 << size) + 1
            } else {
                v
            }
        };
        dc[component] += diff;
        block[0] = dc[component] << (3 - self.params.intra_dc_precision);
        let table = &self.tables.coef[(self.params.intra_vlc_format && self.seq.mpeg2) as usize];
        let scan = self.scan();
        let mut i = 1;
        while let Some((run, level)) = self.coefficient(b, table, false)? {
            i += run;
            if i > 63 {
                return Err(Error::Corrupt("intra block runs past 64"));
            }
            let at = scan[i];
            let mut v = (2 * level * self.seq.intra_matrix[at] * qscale) / 32;
            if !self.seq.mpeg2 && v & 1 == 0 && v != 0 {
                v -= v.signum();
            }
            block[at] = v.clamp(-2048, 2047);
            i += 1;
        }
        self.mismatch(block);
        Ok(())
    }

    fn non_intra_block(
        &self,
        b: &mut Bits,
        block: &mut [i32; 64],
        _k: usize,
        qscale: i32,
    ) -> Result<(), Error> {
        let table = &self.tables.coef[0];
        let scan = self.scan();
        let mut i = 0;
        while let Some((run, level)) = self.coefficient(b, table, i == 0)? {
            i += run;
            if i > 63 {
                return Err(Error::Corrupt("block runs past 64"));
            }
            let at = scan[i];
            let mut v =
                ((2 * level + level.signum()) * self.seq.non_intra_matrix[at] * qscale) / 32;
            if !self.seq.mpeg2 && v & 1 == 0 && v != 0 {
                v -= v.signum();
            }
            block[at] = v.clamp(-2048, 2047);
            i += 1;
        }
        self.mismatch(block);
        Ok(())
    }

    /// MPEG-2 mismatch control: an even sum toggles the last coefficient's low bit.
    fn mismatch(&self, block: &mut [i32; 64]) {
        if self.seq.mpeg2 && block.iter().sum::<i32>() & 1 == 0 {
            block[63] ^= 1;
        }
    }

    /// The next (run, signed level), or `None` at the end of the block.
    fn coefficient(
        &self,
        b: &mut Bits,
        table: &Vlc<Coef>,
        first: bool,
    ) -> Result<Option<(usize, i32)>, Error> {
        if first && b.peek(1) == 1 {
            // The first coefficient of a non-intra block: "1s" is run 0, level 1.
            b.skip(1);
            return Ok(Some((0, if b.bit() { -1 } else { 1 })));
        }
        match table.read(b) {
            Some(Coef::EndOfBlock) => Ok(None),
            Some(Coef::RunLevel(run, level)) => {
                let level = level as i32;
                Ok(Some((run as usize, if b.bit() { -level } else { level })))
            }
            Some(Coef::Escape) => {
                let run = b.read(6) as usize;
                let level = if self.seq.mpeg2 {
                    b.read_signed(12)
                } else {
                    match b.read_signed(8) {
                        0 => b.read(8) as i32,
                        -128 => b.read(8) as i32 - 256,
                        l => l,
                    }
                };
                if level == 0 {
                    return Err(Error::Corrupt("escape level 0"));
                }
                Ok(Some((run, level)))
            }
            None => Err(Error::Corrupt("DCT coefficient")),
        }
    }

    /// Transforms the blocks and writes them: intra ones as they are,
    /// coded non-intra ones (`coded`) added to the prediction already there.
    fn put_blocks(
        &mut self,
        blocks: &mut [[i32; 64]; 6],
        mbx: usize,
        mby: usize,
        field_dct: bool,
        coded: Option<[bool; 6]>,
    ) {
        let stride = self.frame.stride;
        let cstride = stride / 2;
        for (k, block) in blocks.iter_mut().enumerate() {
            if let Some(coded) = coded {
                if !coded[k] {
                    continue;
                }
            }
            idct(block);
            let add = coded.is_some();
            let (plane, x0, rows): (&mut Vec<u8>, usize, [usize; 8]) = if k < 4 {
                let x0 = mbx * 16 + (k & 1) * 8;
                let rows = std::array::from_fn(|j| {
                    if field_dct {
                        mby * 16 + (k >> 1) + 2 * j
                    } else {
                        mby * 16 + (k >> 1) * 8 + j
                    }
                });
                (&mut self.frame.y, x0, rows.map(|r| r * stride))
            } else {
                let rows: [usize; 8] = std::array::from_fn(|j| (mby * 8 + j) * cstride);
                let plane = if k == 4 {
                    &mut self.frame.cb
                } else {
                    &mut self.frame.cr
                };
                (plane, mbx * 8, rows)
            };
            for (j, &row) in rows.iter().enumerate() {
                for i in 0..8 {
                    let at = row + x0 + i;
                    let base = if add { plane[at] as i32 } else { 0 };
                    plane[at] = (base + block[j * 8 + i]).clamp(0, 255) as u8;
                }
            }
        }
    }
}

/// After a macroblock: the stream must not have run out.
fn continue_end(b: &Bits) -> Result<(), Error> {
    if b.bits_left() < 0 {
        return Err(Error::Corrupt("slice runs past its end"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Plane {
    Y,
    Cb,
    Cr,
}

/// Frame prediction: no field split.
const FRAME: (usize, usize) = (usize::MAX, 0);

/// Motion compensation of one block into `dst` from `src`, at plane
/// position (x, y) of size w × h, by vector `v` in half pels. With `field`
/// (destination parity, source parity), x/y/h are in field lines and only
/// that field is read and written. `average` blends with what is there.
#[allow(clippy::too_many_arguments)]
fn mc(
    dst: &mut Frame,
    src: &Frame,
    plane: Plane,
    x: usize,
    y: usize,
    v: [i32; 2],
    w: usize,
    h: usize,
    field: (usize, usize),
    average: bool,
) {
    let chroma = !matches!(plane, Plane::Y);
    let stride = if chroma { dst.stride / 2 } else { dst.stride };
    let rows_total = if chroma {
        dst.cb.len() / stride
    } else {
        dst.y.len() / stride
    };
    let (d, s) = match plane {
        Plane::Y => (&mut dst.y, &src.y),
        Plane::Cb => (&mut dst.cb, &src.cb),
        Plane::Cr => (&mut dst.cr, &src.cr),
    };
    let is_field = field.0 != usize::MAX;
    // A field is every other line; its rows are counted in field lines.
    let (line, dst_off, src_off, rows) = if is_field {
        (
            2 * stride,
            field.0 * stride,
            field.1 * stride,
            rows_total / 2,
        )
    } else {
        (stride, 0, 0, rows_total)
    };
    let (hx, hy) = (v[0] & 1, v[1] & 1);
    let sx = x as i32 + (v[0] >> 1);
    let sy = y as i32 + (v[1] >> 1);
    let inside = sx >= 0
        && sy >= 0
        && sx as usize + w + hx as usize <= stride
        && sy as usize + h + hy as usize <= rows;
    if inside {
        // The usual case: no edge to clamp at, so whole rows at a time.
        let (sx, sy) = (sx as usize, sy as usize);
        for j in 0..h {
            let a = src_off + (sy + j) * line + sx;
            let b = a + line;
            let n = w + hx as usize;
            let row0 = &s[a..a + n];
            let row1 = if hy == 1 { &s[b..b + n] } else { row0 };
            let di = dst_off + (y + j) * line + x;
            let out = &mut d[di..di + w];
            for i in 0..w {
                let p = match (hx, hy) {
                    (0, 0) => row0[i] as i32,
                    (1, 0) => (row0[i] as i32 + row0[i + 1] as i32 + 1) >> 1,
                    (0, _) => (row0[i] as i32 + row1[i] as i32 + 1) >> 1,
                    _ => {
                        (row0[i] as i32
                            + row0[i + 1] as i32
                            + row1[i] as i32
                            + row1[i + 1] as i32
                            + 2)
                            >> 2
                    }
                };
                out[i] = if average {
                    ((out[i] as i32 + p + 1) >> 1) as u8
                } else {
                    p as u8
                };
            }
        }
        return;
    }
    let at = |px: i32, py: i32| -> i32 {
        let cx = px.clamp(0, stride as i32 - 1) as usize;
        let cy = py.clamp(0, rows as i32 - 1) as usize;
        s[src_off + cy * line + cx] as i32
    };
    for j in 0..h {
        for i in 0..w {
            let (px, py) = (sx + i as i32, sy + j as i32);
            let p = match (hx, hy) {
                (0, 0) => at(px, py),
                (1, 0) => (at(px, py) + at(px + 1, py) + 1) >> 1,
                (0, _) => (at(px, py) + at(px, py + 1) + 1) >> 1,
                _ => (at(px, py) + at(px + 1, py) + at(px, py + 1) + at(px + 1, py + 1) + 2) >> 2,
            };
            let di = dst_off + (y + j) * line + x + i;
            d[di] = if average {
                ((d[di] as i32 + p + 1) >> 1) as u8
            } else {
                p as u8
            };
        }
    }
}
