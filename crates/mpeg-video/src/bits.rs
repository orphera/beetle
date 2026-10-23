//! A big-endian bit reader over an elementary stream.

pub struct Bits<'a> {
    data: &'a [u8],
    /// Position in bits.
    pos: usize,
}

impl<'a> Bits<'a> {
    #[cfg(test)]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], byte: usize) -> Self {
        Self {
            data,
            pos: byte * 8,
        }
    }

    #[cfg(test)]
    pub fn byte_pos(&self) -> usize {
        self.pos.div_ceil(8)
    }

    pub fn bits_left(&self) -> isize {
        self.data.len() as isize * 8 - self.pos as isize
    }

    /// The next `n` (≤ 32) bits without moving; zeros past the end.
    pub fn peek(&self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let byte = self.pos / 8;
        let mut word: u64 = 0;
        for i in 0..8 {
            word = (word << 8) | *self.data.get(byte + i).unwrap_or(&0) as u64;
        }
        let shift = 64 - (self.pos % 8) as u32 - n;
        ((word >> shift) & ((1u64 << n) - 1)) as u32
    }

    pub fn skip(&mut self, n: u32) {
        self.pos += n as usize;
    }

    pub fn read(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.skip(n);
        v
    }

    pub fn bit(&mut self) -> bool {
        self.read(1) == 1
    }

    /// `n` bits as a two's complement number.
    pub fn read_signed(&mut self, n: u32) -> i32 {
        let v = self.read(n) as i32;
        (v << (32 - n)) >> (32 - n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_across_bytes() {
        let mut b = Bits::new(&[0b1010_1100, 0b0101_0000]);
        assert_eq!(b.read(3), 0b101);
        assert_eq!(b.peek(7), 0b011_0001);
        assert_eq!(b.read(7), 0b011_0001);
        assert_eq!(b.read_signed(3), 0b010);
        let mut b = Bits::new(&[0xf0]);
        assert_eq!(b.read_signed(4), -1);
    }
}
