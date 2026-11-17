//! The block coder for `f32` values in two dimensions: zfp's `template/encode*.c`,
//! `template/decode*.c` and `template/codec*.c`, as built without `ZFP_WITH_DAZ` and with
//! its default rounding.

use crate::{BLOCK_LEN, BitStream, Block, Settings, power_of_two, x86_f32_to_i32};

/// Bits of a float's exponent, and its bias.
const EXPONENT_BITS: u32 = 8;
const EXPONENT_BIAS: i32 = 127;

/// The bits of the precision the integers are coded at: 32 for `f32`.
const INT_PRECISION: u32 = 32;

/// Alternating ones and zeros, to map two's-complement integers to "negabinary" so that
/// small magnitudes of either sign have few significant bits.
const NEGABINARY_MASK: u32 = 0xaaaa_aaaa;

/// The coefficients of a transformed block in order of increasing frequency: by `i + j`,
/// then by `i² + j²` (`perm_2`).
const SEQUENCY_ORDER: [usize; BLOCK_LEN] = [0, 1, 4, 5, 2, 8, 6, 9, 3, 12, 10, 7, 13, 11, 14, 15];

/// Codes `block` into `stream` and returns the bits written.
pub(crate) fn encode_block(settings: &Settings, stream: &mut BitStream, block: &Block) -> u32 {
    let max_exponent = block_exponent(block);
    let precision = precision(max_exponent, settings);
    let biased_exponent = if precision == 0 {
        0
    } else {
        (max_exponent + EXPONENT_BIAS) as u32
    };

    if biased_exponent == 0 {
        // A single zero bit says every value is zero.
        stream.write_bit(false);
        return pad_to_min_bits(stream, settings, 1);
    }

    // The exponent, shifted left past a one bit that says the block is not all zero.
    let header_bits = 1 + EXPONENT_BITS;
    stream.write_bits(u64::from(2 * biased_exponent + 1), header_bits);

    // Block floating point: every value as an integer scaled by the largest one's exponent.
    let scale = power_of_two(30 - max_exponent);
    let mut ints = block.map(|value| x86_f32_to_i32(scale * value));

    header_bits
        + encode_ints(
            stream,
            settings.min_bits.saturating_sub(header_bits),
            settings.max_bits - header_bits,
            precision,
            &mut ints,
        )
}

/// Decodes a block from `stream` into `block` and returns the bits read.
pub(crate) fn decode_block(settings: &Settings, stream: &mut BitStream, block: &mut Block) -> u32 {
    if !stream.read_bit() {
        *block = [0.0; BLOCK_LEN];
        return skip_to_min_bits(stream, settings, 1);
    }

    let header_bits = 1 + EXPONENT_BITS;
    let max_exponent = stream.read_bits(EXPONENT_BITS) as i32 - EXPONENT_BIAS;
    let precision = precision(max_exponent, settings);

    let mut ints = [0i32; BLOCK_LEN];
    let bits = decode_ints(
        stream,
        settings.min_bits.saturating_sub(header_bits),
        settings.max_bits - header_bits,
        precision,
        &mut ints,
    );

    let scale = power_of_two(max_exponent - 30);
    *block = ints.map(|int| scale * int as f32);
    header_bits + bits
}

/// Pads an all-zero block of `bits` bits up to the settings' minimum.
fn pad_to_min_bits(stream: &mut BitStream, settings: &Settings, bits: u32) -> u32 {
    if settings.min_bits > bits {
        stream.pad(settings.min_bits - bits);
        settings.min_bits
    } else {
        bits
    }
}

/// Skips the padding [`pad_to_min_bits`] wrote.
fn skip_to_min_bits(stream: &mut BitStream, settings: &Settings, bits: u32) -> u32 {
    if settings.min_bits > bits {
        stream.skip(settings.min_bits - bits);
        settings.min_bits
    } else {
        bits
    }
}

/// The bit planes to code for a block whose largest exponent is `max_exponent`.
fn precision(max_exponent: i32, settings: &Settings) -> u32 {
    let planes = (max_exponent - settings.min_exponent + 6).max(0) as u32;
    settings.max_precision.min(planes)
}

/// The exponent of the largest magnitude in a block, ignoring NaN; -127 for an all-zero
/// block.
fn block_exponent(block: &Block) -> i32 {
    let max = block
        .iter()
        .map(|value| value.abs())
        .fold(0.0f32, |max, value| if max < value { value } else { max });
    if max > 0.0 {
        // As frexpf gives it (0 for infinity), clamped to the smallest normal exponent.
        match max.to_bits() >> 23 {
            0xff => 0,
            biased => (biased as i32 - 126).max(1 - EXPONENT_BIAS),
        }
    } else {
        -EXPONENT_BIAS
    }
}

/// Transforms, reorders and codes a block of integers, padding to `min_bits`. Returns the
/// bits written.
fn encode_ints(
    stream: &mut BitStream,
    min_bits: u32,
    max_bits: u32,
    precision: u32,
    ints: &mut [i32; BLOCK_LEN],
) -> u32 {
    forward_transform(ints);
    let coefficients = SEQUENCY_ORDER.map(|at| to_negabinary(ints[at]));

    let bits = if is_rate_limited(max_bits, precision) {
        encode_planes_limited(stream, max_bits, precision, &coefficients)
    } else {
        encode_planes(stream, precision, &coefficients)
    };
    if bits < min_bits {
        stream.pad(min_bits - bits);
        min_bits
    } else {
        bits
    }
}

/// Decodes, reorders and inverse-transforms a block of integers, skipping to `min_bits`.
/// Returns the bits read.
fn decode_ints(
    stream: &mut BitStream,
    min_bits: u32,
    max_bits: u32,
    precision: u32,
    ints: &mut [i32; BLOCK_LEN],
) -> u32 {
    let mut coefficients = [0u32; BLOCK_LEN];
    let mut bits = if is_rate_limited(max_bits, precision) {
        decode_planes_limited(stream, max_bits, precision, &mut coefficients)
    } else {
        decode_planes(stream, precision, &mut coefficients)
    };
    if bits < min_bits {
        stream.skip(min_bits - bits);
        bits = min_bits;
    }

    for (&coefficient, &at) in coefficients.iter().zip(&SEQUENCY_ORDER) {
        ints[at] = from_negabinary(coefficient);
    }
    inverse_transform(ints);
    bits
}

/// Whether `max_bits` can cut a block short: the most a block can take at `precision` is
/// more than it.
fn is_rate_limited(max_bits: u32, precision: u32) -> bool {
    let len = BLOCK_LEN as u32;
    (precision + 1) * len - 1 > max_bits
}

#[inline]
fn to_negabinary(value: i32) -> u32 {
    (value as u32).wrapping_add(NEGABINARY_MASK) ^ NEGABINARY_MASK
}

#[inline]
fn from_negabinary(value: u32) -> i32 {
    (value ^ NEGABINARY_MASK).wrapping_sub(NEGABINARY_MASK) as i32
}

/// The decorrelating transform: along each row, then along each column.
fn forward_transform(ints: &mut [i32; BLOCK_LEN]) {
    for row in 0..4 {
        forward_lift(ints, 4 * row, 1);
    }
    for column in 0..4 {
        forward_lift(ints, column, 4);
    }
}

/// The inverse of [`forward_transform`]: along each column, then along each row.
fn inverse_transform(ints: &mut [i32; BLOCK_LEN]) {
    for column in 0..4 {
        inverse_lift(ints, column, 4);
    }
    for row in 0..4 {
        inverse_lift(ints, 4 * row, 1);
    }
}

/// The forward lifting transform of the four values from `start`, `stride` apart.
#[inline]
fn forward_lift(ints: &mut [i32; BLOCK_LEN], start: usize, stride: usize) {
    let at = [
        start,
        start + stride,
        start + 2 * stride,
        start + 3 * stride,
    ];
    let [mut x, mut y, mut z, mut w] = at.map(|i| ints[i]);
    x = x.wrapping_add(w) >> 1;
    w = w.wrapping_sub(x);
    z = z.wrapping_add(y) >> 1;
    y = y.wrapping_sub(z);
    x = x.wrapping_add(z) >> 1;
    z = z.wrapping_sub(x);
    w = w.wrapping_add(y) >> 1;
    y = y.wrapping_sub(w);
    w = w.wrapping_add(y >> 1);
    y = y.wrapping_sub(w >> 1);
    for (i, value) in at.into_iter().zip([x, y, z, w]) {
        ints[i] = value;
    }
}

/// The inverse of [`forward_lift`].
#[inline]
fn inverse_lift(ints: &mut [i32; BLOCK_LEN], start: usize, stride: usize) {
    let at = [
        start,
        start + stride,
        start + 2 * stride,
        start + 3 * stride,
    ];
    let [mut x, mut y, mut z, mut w] = at.map(|i| ints[i]);
    y = y.wrapping_add(w >> 1);
    w = w.wrapping_sub(y >> 1);
    y = y.wrapping_add(w);
    w = (w << 1).wrapping_sub(y);
    z = z.wrapping_add(x);
    x = (x << 1).wrapping_sub(z);
    y = y.wrapping_add(z);
    z = (z << 1).wrapping_sub(y);
    w = w.wrapping_add(x);
    x = (x << 1).wrapping_sub(w);
    for (i, value) in at.into_iter().zip([x, y, z, w]) {
        ints[i] = value;
    }
}

/// Bit plane `plane` of the coefficients, coefficient `i` in bit `i`.
#[inline]
fn bit_plane(coefficients: &[u32; BLOCK_LEN], plane: u32) -> u64 {
    coefficients
        .iter()
        .enumerate()
        .fold(0, |bits, (i, &value)| {
            bits | u64::from((value >> plane) & 1) << i
        })
}

/// Adds bit plane `plane`, coefficient `i` in bit `i` of `bits`, to the coefficients.
#[inline]
fn deposit_bit_plane(coefficients: &mut [u32; BLOCK_LEN], plane: u32, mut bits: u64) {
    // (A plain scan up to the highest set bit measured fastest across the modes.)
    let mut i = 0;
    while bits != 0 {
        coefficients[i] += ((bits & 1) as u32) << plane;
        bits >>= 1;
        i += 1;
    }
}

/// The lowest bit plane coded at `precision`: planes `INT_PRECISION - 1` down to it are
/// coded, most significant first.
#[inline]
fn lowest_plane(precision: u32) -> u32 {
    INT_PRECISION.saturating_sub(precision)
}

/// Codes the bit planes of the coefficients, at most `max_bits` bits in all. Returns the
/// bits written.
///
/// Each plane starts with the bits of the coefficients already known to be significant;
/// the rest are run-length coded with group tests: a one bit says another significant
/// coefficient follows, found by scanning for its one bit.
fn encode_planes_limited(
    stream: &mut BitStream,
    max_bits: u32,
    precision: u32,
    coefficients: &[u32; BLOCK_LEN],
) -> u32 {
    let len = BLOCK_LEN as u32;
    let mut bits = max_bits;
    let mut significant = 0;
    for plane in (lowest_plane(precision)..INT_PRECISION).rev() {
        if bits == 0 {
            break;
        }
        let mut x = bit_plane(coefficients, plane);
        let verbatim = significant.min(bits);
        bits -= verbatim;
        x = stream.write_bits(x, verbatim);

        while bits != 0 && significant < len {
            bits -= 1;
            if !stream.write_bit(x != 0) {
                break;
            }
            while bits != 0 && significant < len - 1 {
                bits -= 1;
                if stream.write_bit(x & 1 != 0) {
                    break;
                }
                x >>= 1;
                significant += 1;
            }
            x >>= 1;
            significant += 1;
        }
    }
    max_bits - bits
}

/// Codes the bit planes of the coefficients without a limit on the bits. Returns the bits
/// written.
fn encode_planes(stream: &mut BitStream, precision: u32, coefficients: &[u32; BLOCK_LEN]) -> u32 {
    let len = BLOCK_LEN as u32;
    let start = stream.write_position();
    let mut significant = 0;
    for plane in (lowest_plane(precision)..INT_PRECISION).rev() {
        let mut x = stream.write_bits(bit_plane(coefficients, plane), significant);
        while significant < len && stream.write_bit(x != 0) {
            while significant < len - 1 && !stream.write_bit(x & 1 != 0) {
                x >>= 1;
                significant += 1;
            }
            x >>= 1;
            significant += 1;
        }
    }
    (stream.write_position() - start) as u32
}

/// Decodes what [`encode_planes_limited`] writes. Returns the bits read.
fn decode_planes_limited(
    stream: &mut BitStream,
    max_bits: u32,
    precision: u32,
    coefficients: &mut [u32; BLOCK_LEN],
) -> u32 {
    let len = BLOCK_LEN as u32;
    let mut bits = max_bits;
    let mut significant = 0;
    for plane in (lowest_plane(precision)..INT_PRECISION).rev() {
        if bits == 0 {
            break;
        }
        let verbatim = significant.min(bits);
        bits -= verbatim;
        let mut x = stream.read_bits(verbatim);

        while bits != 0 && significant < len {
            bits -= 1;
            if !stream.read_bit() {
                break;
            }
            while bits != 0 && significant < len - 1 {
                bits -= 1;
                if stream.read_bit() {
                    break;
                }
                significant += 1;
            }
            x += 1 << significant;
            significant += 1;
        }
        deposit_bit_plane(coefficients, plane, x);
    }
    max_bits - bits
}

/// Decodes what [`encode_planes`] writes. Returns the bits read.
fn decode_planes(
    stream: &mut BitStream,
    precision: u32,
    coefficients: &mut [u32; BLOCK_LEN],
) -> u32 {
    let len = BLOCK_LEN as u32;
    let start = stream.read_position();
    let mut significant = 0;
    for plane in (lowest_plane(precision)..INT_PRECISION).rev() {
        let mut x = stream.read_bits(significant);
        while significant < len && stream.read_bit() {
            while significant < len - 1 && !stream.read_bit() {
                significant += 1;
            }
            x += 1 << significant;
            significant += 1;
        }
        deposit_bit_plane(coefficients, plane, x);
    }
    (stream.read_position() - start) as u32
}
