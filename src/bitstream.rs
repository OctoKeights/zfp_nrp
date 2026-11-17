//! The stream of bits: a port of zfp's `bitstream.inl`, with its default word of 64 bits.

/// A stream of bits in words of 64 bits (`bitstream.inl`, with its default word): written and
/// read from the least significant bit of each word up.
#[derive(Clone, Debug)]
pub struct BitStream {
    /// Buffered bits, from 0 to 63.
    bits: u32,
    buffer: u64,
    /// The next word to read or write.
    ptr: usize,
    words: Vec<u64>,
}

impl BitStream {
    /// `stream_open` and `stream_rewind`, over `words`: as many as `zfp_stream_maximum_size`
    /// says (its bytes over eight) for writing a field.
    pub fn new(words: Vec<u64>) -> Self {
        BitStream {
            bits: 0,
            buffer: 0,
            ptr: 0,
            words,
        }
    }

    fn read_word(&mut self) -> u64 {
        // (Past the end of the words are zeros: the original reads on into whatever memory
        // follows. A stream `maximum_size` long has room for all its blocks read, whatever
        // their bits say.)
        let word = self.words.get(self.ptr).copied().unwrap_or(0);
        self.ptr += 1;
        word
    }

    fn write_word(&mut self, value: u64) {
        // (The original writes on past the end, which `maximum_size` leaves room for.)
        if self.ptr == self.words.len() {
            self.words.push(value);
        } else {
            self.words[self.ptr] = value;
        }
        self.ptr += 1;
    }

    /// `stream_read_bit`.
    pub fn read_bit(&mut self) -> u32 {
        if self.bits == 0 {
            self.buffer = self.read_word();
            self.bits = 64;
        }
        self.bits -= 1;
        let bit = (self.buffer & 1) as u32;
        self.buffer >>= 1;
        bit
    }

    /// `stream_write_bit`.
    pub fn write_bit(&mut self, bit: u32) -> u32 {
        self.buffer = self.buffer.wrapping_add(u64::from(bit) << self.bits);
        self.bits += 1;
        if self.bits == 64 {
            let buffer = self.buffer;
            self.write_word(buffer);
            self.buffer = 0;
            self.bits = 0;
        }
        bit
    }

    /// `stream_read_bits`: `n` bits, 0 to 64 of them, the first the least significant.
    pub fn read_bits(&mut self, n: u32) -> u64 {
        let mut value = self.buffer;
        if self.bits < n {
            // keep fetching wsize bits until enough bits are buffered (one word does it)
            self.buffer = self.read_word();
            value = value.wrapping_add(self.buffer << self.bits);
            self.bits += 64;
            self.bits -= n;
            if self.bits == 0 {
                // value holds exactly n bits; no need for masking
                self.buffer = 0;
            } else {
                self.buffer >>= 64 - self.bits;
                value &= (2u64 << (n - 1)).wrapping_sub(1);
            }
        } else {
            self.bits -= n;
            // (A shift by the whole word, for 64 bits of a buffer of 64, is not made.)
            self.buffer = if n < 64 { self.buffer >> n } else { 0 };
            value &= if n < 64 { (1u64 << n) - 1 } else { u64::MAX };
        }
        value
    }

    /// `stream_write_bits`: the `n` least significant bits of `value`, 0 to 64 of them; what
    /// was not written comes back, shifted down.
    pub fn write_bits(&mut self, mut value: u64, mut n: u32) -> u64 {
        // append bit string to buffer
        self.buffer = self.buffer.wrapping_add(value << self.bits);
        self.bits += n;
        // is buffer full?
        if self.bits >= 64 {
            // 1 <= n <= 64; decrement n to ensure valid right shifts below
            value >>= 1;
            n -= 1;
            // output wsize bits while buffer is full
            self.bits -= 64;
            let buffer = self.buffer;
            self.write_word(buffer);
            self.buffer = value >> (n - self.bits);
        }
        self.buffer &= (1u64 << self.bits) - 1;
        value >> n
    }

    /// `stream_rtell`.
    pub fn rtell(&self) -> u64 {
        self.ptr as u64 * 64 - u64::from(self.bits)
    }

    /// `stream_wtell`.
    pub fn wtell(&self) -> u64 {
        self.ptr as u64 * 64 + u64::from(self.bits)
    }

    /// `stream_rseek`.
    pub fn rseek(&mut self, offset: u64) {
        let n = (offset % 64) as u32;
        self.ptr = (offset / 64) as usize;
        if n != 0 {
            self.buffer = self.read_word() >> n;
            self.bits = 64 - n;
        } else {
            self.buffer = 0;
            self.bits = 0;
        }
    }

    /// `stream_skip`.
    pub fn skip(&mut self, n: u32) {
        let offset = self.rtell() + u64::from(n);
        self.rseek(offset);
    }

    /// `stream_pad`.
    pub fn pad(&mut self, n: u32) {
        let mut bits = u64::from(self.bits) + u64::from(n);
        while bits >= 64 {
            let buffer = self.buffer;
            self.write_word(buffer);
            self.buffer = 0;
            bits -= 64;
        }
        self.bits = bits as u32;
    }

    /// `stream_flush`: the bits buffered for writing written, with zeros to the end of their
    /// word. The answer is the zeros.
    pub fn flush(&mut self) -> u32 {
        let bits = (64 - self.bits) % 64;
        if bits != 0 {
            self.pad(bits);
        }
        bits
    }

    /// `stream_rewind`: back to the first word, to read or to write.
    pub fn rewind(&mut self) {
        self.bits = 0;
        self.buffer = 0;
        self.ptr = 0;
    }

    /// `stream_size`: the bytes of the words written or read so far.
    pub fn size(&self) -> usize {
        self.ptr * 8
    }

    /// All the words of the stream.
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// The words written so far (after a [`flush`](Self::flush), all that was written), as
    /// bytes: the stream as the original leaves it in memory on a little-endian machine.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.words[..self.ptr]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    /// A stream over `bytes` (as [`to_bytes`](Self::to_bytes) gives them, the last word filled
    /// out with zeros) and `words` words in all (at least as many as the bytes take).
    pub fn from_bytes(bytes: &[u8], words: usize) -> Self {
        let mut all = vec![0u64; words.max(bytes.len().div_ceil(8))];
        for (word, eight) in all.iter_mut().zip(bytes.chunks(8)) {
            let mut le = [0u8; 8];
            le[..eight.len()].copy_from_slice(eight);
            *word = u64::from_le_bytes(le);
        }
        BitStream::new(all)
    }
}
