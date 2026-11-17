//! ZFP compression of floating-point blocks in pure, safe Rust: a native port of the block
//! coder of the [zfp](https://github.com/LLNL/zfp) library v1.0.1, by Peter Lindstrom and
//! others at Lawrence Livermore National Laboratory.
//!
//! zfp compresses a two-dimensional field of `f32` values in blocks of 4 × 4. Each block is
//! stored with the exponent of its largest value, then as integers decorrelated by a
//! reversible transform, then one bit plane at a time from the most significant down: as many
//! bits as a fixed rate allows, as many planes as a fixed precision asks for, or down to a
//! fixed error tolerance. The bits this crate writes are zfp's, bit for bit.
//!
//! - [`Settings`] chooses the mode: [`Settings::fixed_rate`], [`Settings::fixed_precision`]
//!   or [`Settings::fixed_accuracy`].
//!
//! - [`Settings::compress`] and [`Settings::decompress`] code a run of blocks to bytes and
//!   back.
//!
//! - [`Settings::encode_block`] and [`Settings::decode_block`] code one block at a time into
//!   a [`BitStream`], for callers that interleave blocks with data of their own.
//!
//! ```
//! use zfp_nrp::{Block, Settings};
//!
//! let settings = Settings::fixed_accuracy(1e-3);
//! let block: Block = core::array::from_fn(|k| (k as f32 * 0.37).sin());
//!
//! let bytes = settings.compress(&[block]);
//! let back = settings.decompress(&bytes, 1);
//! assert!(block.iter().zip(&back[0]).all(|(a, b)| (a - b).abs() <= 1e-3));
//! ```
//!
//! Only this part of zfp is ported: doubles, integers, other dimensions, whole fields with
//! partial blocks and headers, and the reversible mode are not.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bitstream;
mod codec;

pub use bitstream::BitStream;

/// The fewest bits a block can take (`ZFP_MIN_BITS`).
pub const MIN_BITS: u32 = 1;

/// The most bits a block can take (`ZFP_MAX_BITS`).
pub const MAX_BITS: u32 = 16_658;

/// The most bit planes that can be coded (`ZFP_MAX_PREC`).
pub const MAX_PRECISION: u32 = 64;

/// The lowest exponent of a bit that can be coded (`ZFP_MIN_EXP`).
pub const MIN_EXPONENT: i32 = -1074;

/// The values in a block: 4 × 4.
pub const BLOCK_LEN: usize = 16;

/// A block of 4 × 4 values, row by row.
pub type Block = [f32; BLOCK_LEN];

/// The most bits a stream header can take (`ZFP_HEADER_MAX_BITS`). This crate writes no
/// header, but zfp leaves room for one in its maximum stream size.
const HEADER_MAX_BITS: usize = 148;

/// The values a block of `f32`s holds, for the mode constructors.
const VALUES_PER_BLOCK: u32 = BLOCK_LEN as u32;

/// How blocks are coded: the bits a block may take, the bit planes coded, and the lowest
/// exponent of a bit coded (`zfp_stream`, without its bit stream).
///
/// Each constructor sets one of zfp's modes, so every value of this type is a valid setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    min_bits: u32,
    max_bits: u32,
    max_precision: u32,
    min_exponent: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

impl Settings {
    /// zfp's defaults (`zfp_stream_open`): every limit at its widest, which codes blocks
    /// nearly losslessly.
    pub const fn new() -> Self {
        Self {
            min_bits: MIN_BITS,
            max_bits: MAX_BITS,
            max_precision: MAX_PRECISION,
            min_exponent: MIN_EXPONENT,
        }
    }

    /// Fixed-rate mode (`zfp_stream_set_rate` for floats in two dimensions, unaligned): every
    /// block takes `rate` bits per value, rounded to a whole number of bits per block and at
    /// least 9.
    ///
    /// A rate that is not a number, or too large for a block, is converted as zfp converts
    /// it on x86-64.
    pub fn fixed_rate(rate: f64) -> Self {
        let bits = x86_f64_to_u32((f64::from(VALUES_PER_BLOCK) * rate + 0.5).floor()).max(9);
        Self {
            min_bits: bits,
            max_bits: bits,
            ..Self::new()
        }
    }

    /// Fixed-precision mode (`zfp_stream_set_precision`): `precision` bit planes per block,
    /// at most 64; 0 codes them all.
    pub fn fixed_precision(precision: u32) -> Self {
        let max_precision = match precision {
            0 => MAX_PRECISION,
            p => p.min(MAX_PRECISION),
        };
        Self {
            max_precision,
            ..Self::new()
        }
    }

    /// Fixed-accuracy mode (`zfp_stream_set_accuracy`): no value off by more than
    /// `tolerance`. zfp codes down to the largest power of two no larger than it; a
    /// tolerance of zero or less (or not a number) codes as finely as it can.
    pub fn fixed_accuracy(tolerance: f64) -> Self {
        let min_exponent = if tolerance > 0.0 {
            frexp_exponent(tolerance) - 1
        } else {
            MIN_EXPONENT
        };
        Self {
            min_exponent,
            ..Self::new()
        }
    }

    /// The fewest bits a block takes; shorter blocks are padded with zeros.
    pub const fn min_bits(&self) -> u32 {
        self.min_bits
    }

    /// The most bits a block takes.
    pub const fn max_bits(&self) -> u32 {
        self.max_bits
    }

    /// The most bit planes coded.
    pub const fn max_precision(&self) -> u32 {
        self.max_precision
    }

    /// The lowest exponent of a bit coded.
    pub const fn min_exponent(&self) -> i32 {
        self.min_exponent
    }

    /// The most bytes a field of `width` × `height` values can take
    /// (`zfp_stream_maximum_size`), a multiple of eight.
    ///
    /// As in zfp, a field with a width but no height counts as one-dimensional, and a field
    /// with no width takes nothing.
    pub fn max_stream_len(&self, width: usize, height: usize) -> usize {
        let (dimensions, blocks) = match (width, height) {
            (0, _) => return 0,
            (w, 0) => (1, w.div_ceil(4)),
            (w, h) => (2, w.div_ceil(4).wrapping_mul(h.div_ceil(4))),
        };
        let values = 1u32 << (2 * dimensions);
        // A sign bit and 8 exponent bits, then the bit planes and their group tests.
        let block_bits = (9 + values - 1 + values * self.max_precision.min(32))
            .min(self.max_bits)
            .max(self.min_bits);
        let bits = HEADER_MAX_BITS.wrapping_add(blocks.wrapping_mul(block_bits as usize));
        (bits.wrapping_add(63) & !63) / 8
    }

    /// Codes `block` into `stream` (`zfp_encode_block_float_2`). Returns the bits written.
    pub fn encode_block(&self, stream: &mut BitStream, block: &Block) -> usize {
        codec::encode_block(self, stream, block) as usize
    }

    /// Decodes a block from `stream` (`zfp_decode_block_float_2`), with the settings it was
    /// coded with. Returns the bits read.
    ///
    /// Any bits decode to some block, as in zfp; past the end of the stream, zeros are read.
    pub fn decode_block(&self, stream: &mut BitStream, block: &mut Block) -> usize {
        codec::decode_block(self, stream, block) as usize
    }

    /// Codes `blocks` one after another and returns the stream's bytes, in whole 64-bit
    /// words, little-endian.
    pub fn compress(&self, blocks: &[Block]) -> Vec<u8> {
        let words = self.max_stream_len(BLOCK_LEN * blocks.len(), 4) / 8;
        let mut stream = BitStream::new(vec![0; words]);
        for block in blocks {
            self.encode_block(&mut stream, block);
        }
        stream.flush();
        stream.to_bytes()
    }

    /// Decodes `count` blocks from `bytes`, as [`compress`](Self::compress) makes them.
    pub fn decompress(&self, bytes: &[u8], count: usize) -> Vec<Block> {
        let mut stream = BitStream::from_bytes(bytes, 0);
        let mut blocks = vec![[0.0; BLOCK_LEN]; count];
        for block in &mut blocks {
            self.decode_block(&mut stream, block);
        }
        blocks
    }
}

/// `(uint)` of a double as x86-64 computes it: through a 64-bit integer, keeping the low 32
/// bits. Out-of-range values and NaN become that integer's minimum, as the instruction gives.
fn x86_f64_to_u32(value: f64) -> u32 {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0; // 2^63
    let wide = if (-LIMIT..LIMIT).contains(&value) {
        value as i64
    } else {
        i64::MIN
    };
    wide as u32
}

/// `(int32)` of a float as x86-64 computes it: out-of-range values and NaN become
/// `i32::MIN`, where Rust's `as` would saturate.
#[inline]
fn x86_f32_to_i32(value: f32) -> i32 {
    const LIMIT: f32 = 2_147_483_648.0; // 2^31
    if (-LIMIT..LIMIT).contains(&value) {
        value as i32
    } else {
        i32::MIN
    }
}

/// The exponent `frexp` gives a positive double, as the C library computes it (0 for
/// infinity).
fn frexp_exponent(value: f64) -> i32 {
    let bits = value.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & ((1 << 52) - 1);
    match biased {
        0x7ff => 0,
        0 if mantissa == 0 => 0,
        // Subnormal: normalise by the position of the top set bit.
        0 => -1010 - mantissa.leading_zeros() as i32,
        _ => biased - 1022,
    }
}

/// `ldexpf(1, k)`: 2^k as a float, as the C library rounds it (infinity above the range,
/// zero below the smallest subnormal).
#[inline]
fn power_of_two(k: i32) -> f32 {
    match k {
        128.. => f32::INFINITY,
        -126..=127 => f32::from_bits(((k + 127) as u32) << 23),
        -149..=-127 => f32::from_bits(1 << (k + 149)),
        _ => 0.0,
    }
}

/// The README's examples, compiled and run as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// The Guide's examples, compiled and run as doctests.
#[cfg(doctest)]
#[doc = include_str!("../docs/guide.md")]
struct GuideDoctests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frexp_matches_the_c_library() {
        for value in [1.0f64, 0.75, 3.0, 1e-300, 1e300] {
            let mantissa = value / 2f64.powi(frexp_exponent(value));
            assert!((0.5..1.0).contains(&mantissa), "{value}");
        }
        assert_eq!(frexp_exponent(5e-324), -1073);
        assert_eq!(frexp_exponent(1e-310), -1029);
        assert_eq!(frexp_exponent(f64::INFINITY), 0);
        assert_eq!(frexp_exponent(f64::MAX), 1024);
    }

    #[test]
    fn powers_of_two_round_as_ldexpf() {
        assert_eq!(power_of_two(0), 1.0);
        assert_eq!(power_of_two(-149), f32::from_bits(1));
        assert_eq!(power_of_two(-150), 0.0);
        assert_eq!(power_of_two(128), f32::INFINITY);
    }

    #[test]
    fn modes_set_zfps_limits() {
        assert_eq!(Settings::fixed_rate(8.0).max_bits(), 128);
        assert_eq!(Settings::fixed_rate(f64::NAN).max_bits(), 9);
        assert_eq!(Settings::fixed_precision(0).max_precision(), 64);
        assert_eq!(Settings::fixed_accuracy(0.125).min_exponent(), -3);
        assert_eq!(Settings::fixed_accuracy(-1.0), Settings::new());
    }

    #[test]
    fn blocks_round_trip() {
        let block: Block = core::array::from_fn(|k| k as f32 - 7.5);
        for settings in [Settings::new(), Settings::fixed_precision(32)] {
            assert_eq!(
                settings.decompress(&settings.compress(&[block]), 1)[0],
                block
            );
        }
    }
}
