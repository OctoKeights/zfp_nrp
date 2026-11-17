//! ZFP for floats in two dimensions: a native Rust port of the parts of the
//! [zfp](https://github.com/LLNL/zfp) library v1.0.1, by Peter Lindstrom and others at Lawrence
//! Livermore National Laboratory, that code blocks of four by four floats.
//!
//! - [`ZfpStream`]: the settings of a stream (`zfp.c`): fixed rate, fixed precision or fixed
//!   accuracy, and the most bytes a field's stream takes.
//! - [`BitStream`]: the stream of bits (`bitstream.inl`).
//! - [`encode_block_float_2`] and [`decode_block_float_2`]: the coder of a block
//!   (`template/encode*.c`, `template/decode*.c`, `template/codec*.c`, built for floats in two
//!   dimensions, without `ZFP_WITH_DAZ` and with the rounding it has by default).
//!
//! Each block is stored with the exponent of its largest value, as integers turned by a
//! transform that leaves most of them small, and then one plane of bits at a time, from the
//! top: as many bits as a fixed rate allows, as many planes as a precision says, or down to a
//! tolerance. The bits are zfp's, bit for bit.
//!
//! The rest of zfp is not ported: other types and dimensions, whole fields (`zfp_compress`,
//! `zfp_decompress`) and their headers, partial blocks, the reversible mode, and threads.
//!
//! ```
//! use zfp_nrp::{BitStream, ZfpStream, decode_block_float_2, encode_block_float_2};
//!
//! let mut zfp = ZfpStream::open();
//! zfp.set_accuracy(1e-3);
//! let block: [f32; 16] = core::array::from_fn(|k| (k as f32 * 0.37).sin());
//!
//! let mut stream = BitStream::new(vec![0; zfp.maximum_size(4, 4) / 8]);
//! encode_block_float_2(&zfp, &mut stream, &block);
//! stream.flush();
//!
//! stream.rewind();
//! let mut back = [0f32; 16];
//! decode_block_float_2(&zfp, &mut stream, &mut back);
//! assert!(block.iter().zip(&back).all(|(a, b)| (a - b).abs() <= 1e-3));
//! ```
#![forbid(unsafe_code)]
// The code follows the original's line for line, and these are its shapes.
#![allow(
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::manual_range_contains
)]

mod bitstream;
mod codec;

pub use bitstream::BitStream;
pub use codec::{decode_block_float_2, encode_block_float_2};

/// `ZFP_MIN_BITS`: the fewest bits a block takes.
pub const ZFP_MIN_BITS: u32 = 1;
/// `ZFP_MAX_BITS`: the most bits a block takes.
pub const ZFP_MAX_BITS: u32 = 16658;
/// `ZFP_MAX_PREC`: the most planes of bits coded.
pub const ZFP_MAX_PREC: u32 = 64;
/// `ZFP_MIN_EXP`: the least exponent of a bit coded.
pub const ZFP_MIN_EXP: i32 = -1074;

/// `ZFP_HEADER_MAX_BITS`: the most bits of a header (which the port does not write).
pub const ZFP_HEADER_MAX_BITS: usize = 148;

/// The values of a block of two dimensions.
pub const BLOCK_SIZE: usize = 16;

/// The settings of a stream (`zfp_stream`, but its stream of bits, which is a [`BitStream`]
/// of its own): the fewest and the most bits a block takes, the most planes of bits coded,
/// and the least exponent of a bit coded. One of the `set_` functions makes them for one of
/// zfp's modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZfpStream {
    minbits: u32,
    maxbits: u32,
    maxprec: u32,
    minexp: i32,
}

impl ZfpStream {
    /// `zfp_stream_open`: zfp's defaults (its expert mode, with every setting at its widest).
    pub fn open() -> Self {
        ZfpStream {
            minbits: ZFP_MIN_BITS,
            maxbits: ZFP_MAX_BITS,
            maxprec: ZFP_MAX_PREC,
            minexp: ZFP_MIN_EXP,
        }
    }

    /// `zfp_stream_set_rate` for floats in two dimensions, without alignment for writing at
    /// random: fixed-rate mode, `rate` bits a value (at least 9 bits a block of 16). The
    /// answer is the rate set.
    pub fn set_rate(&mut self, rate: f64) -> f64 {
        let n = 16u32;
        // (`(uint)floor(n * rate + 0.5)`, which for what is not a number, or does not fit,
        // gives what the processor gives: the 64-bit conversion's low 32 bits.)
        let bits = double_to_uint((f64::from(n) * rate + 0.5).floor()).max(1 + 8);
        self.minbits = bits;
        self.maxbits = bits;
        self.maxprec = ZFP_MAX_PREC;
        self.minexp = ZFP_MIN_EXP;
        f64::from(bits) / f64::from(n)
    }

    /// `zfp_stream_set_precision`: fixed-precision mode, `precision` planes of bits (all of
    /// them for 0). The answer is the precision set.
    pub fn set_precision(&mut self, precision: u32) -> u32 {
        self.minbits = ZFP_MIN_BITS;
        self.maxbits = ZFP_MAX_BITS;
        self.maxprec = if precision != 0 {
            precision.min(ZFP_MAX_PREC)
        } else {
            ZFP_MAX_PREC
        };
        self.minexp = ZFP_MIN_EXP;
        self.maxprec
    }

    /// `zfp_stream_set_accuracy`: fixed-accuracy mode, no error larger than `tolerance`
    /// (none for no more than nothing). The answer is the tolerance set: the power of two
    /// that is no larger than `tolerance`, or nothing.
    pub fn set_accuracy(&mut self, tolerance: f64) -> f64 {
        let mut emin = ZFP_MIN_EXP;
        if tolerance > 0.0 {
            // tolerance = x * 2^emin, with 0.5 <= x < 1
            emin = frexp_exponent(tolerance);
            emin -= 1;
        }
        self.minbits = ZFP_MIN_BITS;
        self.maxbits = ZFP_MAX_BITS;
        self.maxprec = ZFP_MAX_PREC;
        self.minexp = emin;
        if tolerance > 0.0 {
            power_of_two_f64(emin)
        } else {
            0.0
        }
    }

    /// The fewest bits a block takes (`minbits`).
    pub fn minbits(&self) -> u32 {
        self.minbits
    }

    /// The most bits a block takes (`maxbits`).
    pub fn maxbits(&self) -> u32 {
        self.maxbits
    }

    /// The most planes of bits coded (`maxprec`).
    pub fn maxprec(&self) -> u32 {
        self.maxprec
    }

    /// The least exponent of a bit coded (`minexp`).
    pub fn minexp(&self) -> i32 {
        self.minexp
    }

    /// `zfp_stream_maximum_size` of a field of floats, `nx` by `ny` (`zfp_field_2d`): the
    /// bytes its stream can take, a multiple of eight. (As zfp has it, a field `nx` wide and
    /// none high is one of one dimension, and one none wide has no size.)
    pub fn maximum_size(&self, nx: usize, ny: usize) -> usize {
        let (dims, blocks) = if nx == 0 {
            return 0;
        } else if ny == 0 {
            (1, nx.div_ceil(4))
        } else {
            (2, nx.div_ceil(4).wrapping_mul(ny.div_ceil(4)))
        };
        let values = 1u32 << (2 * dims);
        // (1 + 8 for a float's exponent.)
        let mut maxbits = 1 + 8;
        maxbits += values - 1 + values * self.maxprec.min(32);
        maxbits = maxbits.min(self.maxbits);
        maxbits = maxbits.max(self.minbits);
        (ZFP_HEADER_MAX_BITS
            .wrapping_add(blocks.wrapping_mul(maxbits as usize))
            .wrapping_add(63)
            & !63)
            / 8
    }
}

impl Default for ZfpStream {
    fn default() -> Self {
        ZfpStream::open()
    }
}

/// `(uint)` of a double as x86-64 converts it: through a 64-bit integer, of which the low 32
/// bits are kept; what is not a number, or is out of that integer's reach, gives its least.
fn double_to_uint(value: f64) -> u32 {
    let wide = if value.is_nan()
        || value >= 9_223_372_036_854_775_808.0
        || value < -9_223_372_036_854_775_808.0
    {
        i64::MIN
    } else {
        value as i64
    };
    wide as u32
}

/// `(int32)` of a float as x86-64 converts it: what is not a number, or does not fit, gives
/// the least integer.
fn float_to_int(value: f32) -> i32 {
    if value.is_nan() || value >= 2_147_483_648.0 || value < -2_147_483_648.0 {
        i32::MIN
    } else {
        value as i32
    }
}

/// The exponent `frexp` gives a positive double (the C library's: 0 for an infinity).
fn frexp_exponent(value: f64) -> i32 {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    if exponent == 0x7ff {
        0
    } else if exponent == 0 {
        let mantissa = bits & ((1u64 << 52) - 1);
        if mantissa == 0 {
            0
        } else {
            // A subnormal: as 0.5 times two to one more than the top bit's place.
            -1022 - (mantissa.leading_zeros() as i32 - 12) + 1 - 1
        }
    } else {
        exponent - 1022
    }
}

/// `ldexpf(1, k)`: two to `k` as a float, rounded as the C library rounds it (to infinity
/// above, to nothing below the least subnormal, the half of which rounds to even, nothing).
fn power_of_two(k: i32) -> f32 {
    if k > 127 {
        f32::INFINITY
    } else if k >= -126 {
        f32::from_bits(((k + 127) as u32) << 23)
    } else if k >= -149 {
        f32::from_bits(1u32 << (k + 149))
    } else {
        0.0
    }
}

/// `ldexp(1.0, k)` for the exponents `set_accuracy` makes (-1074 to 1023), which are exact.
fn power_of_two_f64(k: i32) -> f64 {
    if k >= -1022 {
        f64::from_bits(((k + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (k + 1074))
    }
}

/// (The README's examples, compiled and run with the tests.)
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frexp_and_powers_of_two() {
        for value in [1.0f64, 0.75, 3.0, 1e-300, 5e-324, 1e-310, 1e300] {
            let e = frexp_exponent(value);
            let mantissa = value / 2f64.powi(e);
            // (powi of very small exponents loses the value; compare on the bits instead.)
            if value > 1e-290 {
                assert!((0.5..1.0).contains(&mantissa), "{value}: {e}");
            }
        }
        assert_eq!(frexp_exponent(5e-324), -1073);
        assert_eq!(frexp_exponent(f64::INFINITY), 0);
        assert_eq!(frexp_exponent(f64::MAX), 1024);
        assert_eq!(power_of_two(0), 1.0);
        assert_eq!(power_of_two(-149), f32::from_bits(1));
        assert_eq!(power_of_two(-150), 0.0);
        assert_eq!(power_of_two(128), f32::INFINITY);
        assert_eq!(power_of_two_f64(-1074), 5e-324);
        assert_eq!(power_of_two_f64(-3), 0.125);
        assert_eq!(power_of_two_f64(1023), 2f64.powi(1023));
    }
}
