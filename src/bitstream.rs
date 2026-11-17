//! The bit stream blocks are coded into: zfp's `bitstream.inl` with its default 64-bit word.

/// A stream of bits in 64-bit words, written and read from the least significant bit of each
/// word up.
///
/// One stream is either written or read at a time; [`rewind`](Self::rewind) goes back to the
/// start for the other. Reading past the last word gives zeros, and writing past it adds
/// words.
#[derive(Clone, Debug, Default)]
pub struct BitStream {
    words: Vec<u64>,
    /// The next word to read or write.
    next: usize,
    /// Bits held between whole words: those still to read, or those written but not yet
    /// stored.
    buffer: u64,
    /// How many bits `buffer` holds, 0 to 63.
    buffered: u32,
}

impl BitStream {
    /// A stream over `words`, positioned at the start. To write a field, give it as many
    /// words as [`Settings::max_stream_len`](crate::Settings::max_stream_len) allows (bytes
    /// divided by eight).
    pub fn new(words: Vec<u64>) -> Self {
        Self {
            words,
            ..Self::default()
        }
    }

    /// A stream over `bytes`, read as little-endian words, with at least `min_words` words
    /// (zeros after the bytes).
    pub fn from_bytes(bytes: &[u8], min_words: usize) -> Self {
        let mut words: Vec<u64> = bytes
            .chunks(8)
            .map(|chunk| {
                let mut word = [0; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                u64::from_le_bytes(word)
            })
            .collect();
        if words.len() < min_words {
            words.resize(min_words, 0);
        }
        Self::new(words)
    }

    /// The words written so far, as little-endian bytes. Call [`flush`](Self::flush) first
    /// to include the last, partial word.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.words[..self.next]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    /// All the stream's words.
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// The bytes of the whole words written or read so far (`stream_size`).
    pub fn byte_len(&self) -> usize {
        self.next * 8
    }

    /// The position of the next bit to read (`stream_rtell`).
    pub fn read_position(&self) -> u64 {
        self.next as u64 * 64 - u64::from(self.buffered)
    }

    /// The position of the next bit to write (`stream_wtell`).
    pub fn write_position(&self) -> u64 {
        self.next as u64 * 64 + u64::from(self.buffered)
    }

    /// Back to the start, to read or to write (`stream_rewind`).
    pub fn rewind(&mut self) {
        self.next = 0;
        self.buffer = 0;
        self.buffered = 0;
    }

    /// Moves the read position to `position` (`stream_rseek`).
    pub fn seek(&mut self, position: u64) {
        self.next = (position / 64) as usize;
        let offset = (position % 64) as u32;
        if offset == 0 {
            self.buffer = 0;
            self.buffered = 0;
        } else {
            self.buffer = self.read_word() >> offset;
            self.buffered = 64 - offset;
        }
    }

    /// Skips `n` bits when reading (`stream_skip`).
    pub fn skip(&mut self, n: u32) {
        self.seek(self.read_position() + u64::from(n));
    }

    /// Reads one bit (`stream_read_bit`).
    #[inline]
    pub fn read_bit(&mut self) -> bool {
        if self.buffered == 0 {
            self.buffer = self.read_word();
            self.buffered = 64;
        }
        self.buffered -= 1;
        let bit = self.buffer & 1 != 0;
        self.buffer >>= 1;
        bit
    }

    /// Reads `n` bits, 0 to 64, the first into the least significant place
    /// (`stream_read_bits`).
    #[inline]
    pub fn read_bits(&mut self, n: u32) -> u64 {
        debug_assert!(n <= 64);
        let mut value = self.buffer;
        if self.buffered < n {
            // One more word always holds enough.
            let word = self.read_word();
            value = value.wrapping_add(word << self.buffered);
            self.buffered += 64 - n;
            if self.buffered == 0 {
                self.buffer = 0;
            } else {
                self.buffer = word >> (64 - self.buffered);
                value &= (2u64 << (n - 1)).wrapping_sub(1);
            }
        } else {
            self.buffered -= n;
            // (A shift by the whole word is not made: n is 64 only with 64 buffered bits.)
            if n < 64 {
                self.buffer >>= n;
                value &= (1 << n) - 1;
            } else {
                self.buffer = 0;
            }
        }
        value
    }

    /// Writes one bit and returns it (`stream_write_bit`).
    #[inline]
    pub fn write_bit(&mut self, bit: bool) -> bool {
        self.buffer = self.buffer.wrapping_add(u64::from(bit) << self.buffered);
        self.buffered += 1;
        if self.buffered == 64 {
            self.write_word(self.buffer);
            self.buffer = 0;
            self.buffered = 0;
        }
        bit
    }

    /// Writes the `n` least significant bits of `value`, 0 to 64, and returns the bits of
    /// `value` above them (`stream_write_bits`).
    #[inline]
    pub fn write_bits(&mut self, value: u64, n: u32) -> u64 {
        debug_assert!(n <= 64);
        self.buffer = self.buffer.wrapping_add(value << self.buffered);
        self.buffered += n;
        if self.buffered >= 64 {
            // n is at least 1 here; shifting in two steps keeps each shift below 64.
            let (value, n) = (value >> 1, n - 1);
            self.buffered -= 64;
            self.write_word(self.buffer);
            self.buffer = value >> (n - self.buffered);
            self.buffer &= (1 << self.buffered) - 1;
            return value >> n;
        }
        // Here fewer than 64 bits are buffered, so n is below 64.
        self.buffer &= (1 << self.buffered) - 1;
        value >> n
    }

    /// Writes `n` zero bits (`stream_pad`).
    pub fn pad(&mut self, n: u32) {
        let mut buffered = u64::from(self.buffered) + u64::from(n);
        while buffered >= 64 {
            self.write_word(self.buffer);
            self.buffer = 0;
            buffered -= 64;
        }
        self.buffered = buffered as u32;
    }

    /// Writes zeros to the end of the current word, so that everything written is stored.
    /// Returns the zero bits added (`stream_flush`).
    pub fn flush(&mut self) -> u32 {
        let padding = (64 - self.buffered) % 64;
        if padding != 0 {
            self.pad(padding);
        }
        padding
    }

    #[inline]
    fn read_word(&mut self) -> u64 {
        let word = self.words.get(self.next).copied().unwrap_or(0);
        self.next += 1;
        word
    }

    #[inline]
    fn write_word(&mut self, word: u64) {
        match self.words.get_mut(self.next) {
            Some(slot) => *slot = word,
            None => self.words.push(word),
        }
        self.next += 1;
    }
}
