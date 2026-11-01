//! The part of MessagePack (<https://msgpack.org>) the save files use:
//! nil, booleans, integers, 64-bit floats, strings, binary, arrays and maps.
//! Extension types and 32-bit floats are never written; a reader meeting
//! one rejects the file.
//!
//! Every save file is one map with a `format` string and a `version`
//! number next to its own fields, so any MessagePack tool can open it.
//! Records are maps keyed by short names, and readers skip keys they do
//! not know, so a file from a newer build still loads.

/// Builds a MessagePack document. Each value is written with its shortest encoding.
#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn nil(&mut self) {
        self.buf.push(0xc0);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(if v { 0xc3 } else { 0xc2 });
    }

    pub fn uint(&mut self, v: u64) {
        if v < 0x80 {
            self.buf.push(v as u8);
        } else if v <= 0xff {
            self.buf.extend_from_slice(&[0xcc, v as u8]);
        } else if v <= 0xffff {
            self.buf.push(0xcd);
            self.buf.extend_from_slice(&(v as u16).to_be_bytes());
        } else if v <= 0xffff_ffff {
            self.buf.push(0xce);
            self.buf.extend_from_slice(&(v as u32).to_be_bytes());
        } else {
            self.buf.push(0xcf);
            self.buf.extend_from_slice(&v.to_be_bytes());
        }
    }

    pub fn int(&mut self, v: i64) {
        if v >= 0 {
            self.uint(v as u64);
        } else if v >= -32 {
            self.buf.push(v as u8);
        } else if v >= i64::from(i8::MIN) {
            self.buf.extend_from_slice(&[0xd0, v as u8]);
        } else if v >= i64::from(i16::MIN) {
            self.buf.push(0xd1);
            self.buf.extend_from_slice(&(v as i16).to_be_bytes());
        } else if v >= i64::from(i32::MIN) {
            self.buf.push(0xd2);
            self.buf.extend_from_slice(&(v as i32).to_be_bytes());
        } else {
            self.buf.push(0xd3);
            self.buf.extend_from_slice(&v.to_be_bytes());
        }
    }

    pub fn f64(&mut self, v: f64) {
        self.buf.push(0xcb);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn str(&mut self, v: &str) {
        let n = v.len();
        if n < 32 {
            self.buf.push(0xa0 | n as u8);
        } else {
            self.len_header(n, [0xd9, 0xda, 0xdb]);
        }
        self.buf.extend_from_slice(v.as_bytes());
    }

    pub fn bin(&mut self, v: &[u8]) {
        self.len_header(v.len(), [0xc4, 0xc5, 0xc6]);
        self.buf.extend_from_slice(v);
    }

    /// A string, or nil for `None`.
    pub fn opt_str(&mut self, v: Option<&str>) {
        match v {
            Some(s) => self.str(s),
            None => self.nil(),
        }
    }

    /// A number, or nil for `None`.
    pub fn opt_uint(&mut self, v: Option<u64>) {
        match v {
            Some(n) => self.uint(n),
            None => self.nil(),
        }
    }

    /// Starts an array of `n` values; the values follow.
    pub fn array(&mut self, n: usize) {
        if n < 16 {
            self.buf.push(0x90 | n as u8);
        } else {
            self.len_header(n, [0, 0xdc, 0xdd]);
        }
    }

    /// Starts a map of `n` entries; each key and its value follow.
    pub fn map(&mut self, n: usize) {
        if n < 16 {
            self.buf.push(0x80 | n as u8);
        } else {
            self.len_header(n, [0, 0xde, 0xdf]);
        }
    }

    /// Starts a save file: its map of `fields + 2` entries, `format` and
    /// `version` first. The caller writes the other `fields` entries.
    pub fn file(&mut self, format: &str, version: u64, fields: usize) {
        self.map(fields + 2);
        self.str("format");
        self.str(format);
        self.str("version");
        self.uint(version);
    }

    /// An 8-, 16- or 32-bit length after its marker (a zero marker means
    /// the 8-bit form does not exist for this type).
    fn len_header(&mut self, n: usize, markers: [u8; 3]) {
        if n <= 0xff && markers[0] != 0 {
            self.buf.extend_from_slice(&[markers[0], n as u8]);
        } else if n <= 0xffff {
            self.buf.push(markers[1]);
            self.buf.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            self.buf.push(markers[2]);
            self.buf.extend_from_slice(&(n as u32).to_be_bytes());
        }
    }
}

/// A decoded value. Strings and binary borrow from the input.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    Nil,
    Bool(bool),
    /// Every integer, signed or not, fits here.
    Int(i128),
    F64(f64),
    Str(&'a str),
    Bin(&'a [u8]),
    Array(Vec<Value<'a>>),
    Map(Vec<(Value<'a>, Value<'a>)>),
}

impl<'a> Value<'a> {
    /// The value under a string key of a map.
    pub fn get(&self, key: &str) -> Option<&Value<'a>> {
        match self {
            Value::Map(entries) => entries
                .iter()
                .find(|(k, _)| matches!(k, Value::Str(s) if *s == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            Value::Int(n) => u64::try_from(n).ok(),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        self.as_u64().and_then(|n| u32::try_from(n).ok())
    }

    /// A float, or an integer read as one.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::F64(f) => Some(f),
            Value::Int(n) => Some(n as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&'a str> {
        match *self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bin(&self) -> Option<&'a [u8]> {
        match *self {
            Value::Bin(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value<'a>]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// `None` for nil, else the value. For optional fields written as nil.
    pub fn non_nil(&self) -> Option<&Value<'a>> {
        (!matches!(self, Value::Nil)).then_some(self)
    }
}

/// Decodes a whole document. Bytes left over after the value, a value cut
/// short, a type outside the subset or nesting deeper than 32 levels give `None`.
pub fn decode(bytes: &[u8]) -> Option<Value<'_>> {
    let mut reader = Reader { bytes, pos: 0 };
    let value = reader.value(0)?;
    (reader.pos == bytes.len()).then_some(value)
}

/// Decodes a save file, checking its `format` and `version`.
pub fn decode_file<'a>(bytes: &'a [u8], format: &str, version: u64) -> Option<Value<'a>> {
    let file = decode(bytes)?;
    let ok = file.get("format")?.as_str()? == format && file.get("version")?.as_u64()? == version;
    ok.then_some(file)
}

const MAX_DEPTH: usize = 32;

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn be<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn len(&mut self, size: usize) -> Option<usize> {
        Some(match size {
            1 => usize::from(self.be::<1>()?[0]),
            2 => usize::from(u16::from_be_bytes(self.be()?)),
            _ => u32::from_be_bytes(self.be()?) as usize,
        })
    }

    fn value(&mut self, depth: usize) -> Option<Value<'a>> {
        if depth > MAX_DEPTH {
            return None;
        }
        let marker = self.be::<1>()?[0];
        Some(match marker {
            0x00..=0x7f => Value::Int(i128::from(marker)),
            0x80..=0x8f => self.map(usize::from(marker & 0x0f), depth)?,
            0x90..=0x9f => self.array(usize::from(marker & 0x0f), depth)?,
            0xa0..=0xbf => self.str(usize::from(marker & 0x1f))?,
            0xc0 => Value::Nil,
            0xc2 => Value::Bool(false),
            0xc3 => Value::Bool(true),
            0xc4..=0xc6 => {
                let n = self.len(1 << (marker - 0xc4))?;
                Value::Bin(self.take(n)?)
            }
            0xcb => Value::F64(f64::from_be_bytes(self.be()?)),
            0xcc => Value::Int(i128::from(self.be::<1>()?[0])),
            0xcd => Value::Int(i128::from(u16::from_be_bytes(self.be()?))),
            0xce => Value::Int(i128::from(u32::from_be_bytes(self.be()?))),
            0xcf => Value::Int(i128::from(u64::from_be_bytes(self.be()?))),
            0xd0 => Value::Int(i128::from(i8::from_be_bytes(self.be()?))),
            0xd1 => Value::Int(i128::from(i16::from_be_bytes(self.be()?))),
            0xd2 => Value::Int(i128::from(i32::from_be_bytes(self.be()?))),
            0xd3 => Value::Int(i128::from(i64::from_be_bytes(self.be()?))),
            0xd9..=0xdb => {
                let n = self.len(1 << (marker - 0xd9))?;
                self.str(n)?
            }
            0xdc | 0xdd => {
                let n = self.len(2 << (marker - 0xdc))?;
                self.array(n, depth)?
            }
            0xde | 0xdf => {
                let n = self.len(2 << (marker - 0xde))?;
                self.map(n, depth)?
            }
            0xe0..=0xff => Value::Int(i128::from(marker as i8)),
            _ => return None,
        })
    }

    fn str(&mut self, n: usize) -> Option<Value<'a>> {
        std::str::from_utf8(self.take(n)?).ok().map(Value::Str)
    }

    fn array(&mut self, n: usize, depth: usize) -> Option<Value<'a>> {
        // Every value takes at least a byte, so a count past the end is corrupt.
        if n > self.bytes.len() - self.pos {
            return None;
        }
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            items.push(self.value(depth + 1)?);
        }
        Some(Value::Array(items))
    }

    fn map(&mut self, n: usize, depth: usize) -> Option<Value<'a>> {
        if n > (self.bytes.len() - self.pos) / 2 {
            return None;
        }
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let key = self.value(depth + 1)?;
            entries.push((key, self.value(depth + 1)?));
        }
        Some(Value::Map(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(f: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut w = Writer::new();
        f(&mut w);
        w.into_bytes()
    }

    #[test]
    fn integers_take_their_shortest_form_and_read_back() {
        let cases: &[(i64, &[u8])] = &[
            (0, &[0x00]),
            (127, &[0x7f]),
            (128, &[0xcc, 0x80]),
            (256, &[0xcd, 0x01, 0x00]),
            (65_536, &[0xce, 0, 1, 0, 0]),
            (-1, &[0xff]),
            (-32, &[0xe0]),
            (-33, &[0xd0, 0xdf]),
            (-129, &[0xd1, 0xff, 0x7f]),
        ];
        for &(n, want) in cases {
            let got = bytes(|w| w.int(n));
            assert_eq!(got, want, "{n}");
            assert_eq!(decode(&got), Some(Value::Int(i128::from(n))));
        }
        let max = bytes(|w| w.uint(u64::MAX));
        assert_eq!(decode(&max).unwrap().as_u64(), Some(u64::MAX));
        let min = bytes(|w| w.int(i64::MIN));
        assert_eq!(decode(&min), Some(Value::Int(i128::from(i64::MIN))));
    }

    #[test]
    fn strings_binary_and_containers_round_trip() {
        let long = "가".repeat(40);
        let blob = vec![7u8; 300];
        let doc = bytes(|w| {
            w.map(3);
            w.str("s");
            w.str(&long);
            w.str("b");
            w.bin(&blob);
            w.str("a");
            w.array(20);
            for i in 0..20 {
                w.f64(f64::from(i) / 3.0);
            }
        });
        let v = decode(&doc).unwrap();
        assert_eq!(v.get("s").unwrap().as_str(), Some(long.as_str()));
        assert_eq!(v.get("b").unwrap().as_bin(), Some(blob.as_slice()));
        let a = v.get("a").unwrap().as_array().unwrap();
        assert_eq!(a.len(), 20);
        assert_eq!(a[3].as_f64(), Some(1.0));
        assert!(v.get("missing").is_none());
    }

    #[test]
    fn files_carry_format_and_version() {
        let doc = bytes(|w| {
            w.file("beetle-test", 1, 1);
            w.str("x");
            w.nil();
        });
        assert!(decode_file(&doc, "beetle-test", 1).is_some());
        assert!(decode_file(&doc, "beetle-test", 2).is_none());
        assert!(decode_file(&doc, "beetle-other", 1).is_none());
    }

    #[test]
    fn broken_input_is_rejected() {
        let doc = bytes(|w| {
            w.array(2);
            w.str("abc");
            w.uint(1000);
        });
        for cut in 0..doc.len() {
            assert!(decode(&doc[..cut]).is_none(), "cut at {cut}");
        }
        let mut extra = doc.clone();
        extra.push(0);
        assert!(decode(&extra).is_none());
        assert!(decode(b"#BEETLE_SCORES_V2\n").is_none());
        // A count far past the end does not allocate for it.
        assert!(decode(&[0xdd, 0xff, 0xff, 0xff, 0xff]).is_none());
        assert!(decode(&[0x91; 64]).is_none());
    }
}
