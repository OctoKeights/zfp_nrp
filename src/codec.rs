//! The coder of a block of four by four floats: a port of zfp's `template/encode.c`,
//! `template/encodef.c`, `template/encode2.c`, `template/decode*.c` and `template/codec*.c`
//! for floats in two dimensions.

use crate::{BLOCK_SIZE, BitStream, ZfpStream, float_to_int, power_of_two};

/// Of floats: `EBITS`, `EBIAS`, `NBMASK`.
const EBITS: u32 = 8;
const EBIAS: i32 = (1 << (EBITS - 1)) - 1;
const NBMASK: u32 = 0xaaaa_aaaa;

/// `perm_2`: coefficients by i + j, then by i² + j².
const PERM_2: [usize; 16] = [0, 1, 4, 5, 2, 8, 6, 9, 3, 12, 10, 7, 13, 11, 14, 15];

/// `precision`: the planes of bits to code of a block with the exponent `maxexp`.
fn precision(maxexp: i32, maxprec: u32, minexp: i32) -> u32 {
    maxprec.min((maxexp - minexp + 2 * 2 + 2).max(0) as u32)
}

/// `with_maxbits`: whether the most a block can take is more than `maxbits`.
fn with_maxbits(maxbits: u32, maxprec: u32, size: u32) -> bool {
    (maxprec + 1) * size - 1 > maxbits
}

/// `exponent`: the exponent of a value of no less than nothing, -127 for nothing.
fn exponent(x: f32) -> i32 {
    // use e = -EBIAS when x = 0
    let mut e = -EBIAS;
    if x > 0.0 {
        let bits = x.to_bits();
        let biased = (bits >> 23) as i32;
        // (frexpf: 0 for an infinity; a subnormal's own, which is below the clamp.)
        e = if biased == 0xff {
            0
        } else if biased == 0 {
            -126
        } else {
            biased - 126
        };
        // clamp exponent in case x is subnormal; may still result in overflow
        e = e.max(1 - EBIAS);
    }
    e
}

/// `exponent_block`: the exponent of the largest value of a block (what is not a number left
/// out).
fn exponent_block(block: &[f32; BLOCK_SIZE]) -> i32 {
    let mut max = 0f32;
    for &value in block {
        let f = value.abs();
        if max < f {
            max = f;
        }
    }
    exponent(max)
}

/// `fwd_lift`: the forward transform of the four values from `at`, `s` apart.
fn fwd_lift(p: &mut [i32; BLOCK_SIZE], at: usize, s: usize) {
    let (mut x, mut y, mut z, mut w) = (p[at], p[at + s], p[at + 2 * s], p[at + 3 * s]);
    x = x.wrapping_add(w);
    x >>= 1;
    w = w.wrapping_sub(x);
    z = z.wrapping_add(y);
    z >>= 1;
    y = y.wrapping_sub(z);
    x = x.wrapping_add(z);
    x >>= 1;
    z = z.wrapping_sub(x);
    w = w.wrapping_add(y);
    w >>= 1;
    y = y.wrapping_sub(w);
    w = w.wrapping_add(y >> 1);
    y = y.wrapping_sub(w >> 1);
    p[at] = x;
    p[at + s] = y;
    p[at + 2 * s] = z;
    p[at + 3 * s] = w;
}

/// `inv_lift`.
fn inv_lift(p: &mut [i32; BLOCK_SIZE], at: usize, s: usize) {
    let (mut x, mut y, mut z, mut w) = (p[at], p[at + s], p[at + 2 * s], p[at + 3 * s]);
    y = y.wrapping_add(w >> 1);
    w = w.wrapping_sub(y >> 1);
    y = y.wrapping_add(w);
    w = w.wrapping_shl(1);
    w = w.wrapping_sub(y);
    z = z.wrapping_add(x);
    x = x.wrapping_shl(1);
    x = x.wrapping_sub(z);
    y = y.wrapping_add(z);
    z = z.wrapping_shl(1);
    z = z.wrapping_sub(y);
    w = w.wrapping_add(x);
    x = x.wrapping_shl(1);
    x = x.wrapping_sub(w);
    p[at] = x;
    p[at + s] = y;
    p[at + 2 * s] = z;
    p[at + 3 * s] = w;
}

/// `encode_few_ints`: the planes of bits of up to 64 integers, no more than `maxbits` bits.
fn encode_few_ints(s: &mut BitStream, maxbits: u32, maxprec: u32, data: &[u32; BLOCK_SIZE]) -> u32 {
    let size = BLOCK_SIZE as u32;
    let intprec = 32u32;
    let kmin = intprec.saturating_sub(maxprec);
    let mut bits = maxbits;
    let (mut k, mut n) = (intprec, 0u32);

    // encode one bit plane at a time from MSB to LSB
    while bits != 0 && k > kmin {
        k -= 1;
        // step 1: extract bit plane #k to x
        let mut x = 0u64;
        for (i, &value) in data.iter().enumerate() {
            x += u64::from((value >> k) & 1) << i;
        }
        // step 2: encode first n bits of bit plane
        let m = n.min(bits);
        bits -= m;
        x = s.write_bits(x, m);
        // step 3: unary run-length encode remainder of bit plane
        while bits != 0 && n < size {
            bits -= 1;
            if s.write_bit(u32::from(x != 0)) != 0 {
                // positive group test (x != 0); scan for one-bit
                while bits != 0 && n < size - 1 {
                    bits -= 1;
                    if s.write_bit((x & 1) as u32) != 0 {
                        break;
                    }
                    x >>= 1;
                    n += 1;
                }
            } else {
                // negative group test (x == 0); done with bit plane
                break;
            }
            x >>= 1;
            n += 1;
        }
    }

    maxbits - bits
}

/// `encode_few_ints_prec`: the planes of bits of up to 64 integers, without a limit.
fn encode_few_ints_prec(s: &mut BitStream, maxprec: u32, data: &[u32; BLOCK_SIZE]) -> u32 {
    let offset = s.wtell();
    let size = BLOCK_SIZE as u32;
    let intprec = 32u32;
    let kmin = intprec.saturating_sub(maxprec);
    let (mut k, mut n) = (intprec, 0u32);

    // encode one bit plane at a time from MSB to LSB
    while k > kmin {
        k -= 1;
        // step 1: extract bit plane #k to x
        let mut x = 0u64;
        for (i, &value) in data.iter().enumerate() {
            x += u64::from((value >> k) & 1) << i;
        }
        // step 2: encode first n bits of bit plane
        x = s.write_bits(x, n);
        // step 3: unary run-length encode remainder of bit plane
        while n < size && s.write_bit(u32::from(x != 0)) != 0 {
            while n < size - 1 && s.write_bit((x & 1) as u32) == 0 {
                x >>= 1;
                n += 1;
            }
            x >>= 1;
            n += 1;
        }
    }

    (s.wtell() - offset) as u32
}

/// `decode_few_ints`.
fn decode_few_ints(
    s: &mut BitStream,
    maxbits: u32,
    maxprec: u32,
    data: &mut [u32; BLOCK_SIZE],
) -> u32 {
    let size = BLOCK_SIZE as u32;
    let intprec = 32u32;
    let kmin = intprec.saturating_sub(maxprec);
    let mut bits = maxbits;
    let (mut k, mut n) = (intprec, 0u32);

    // initialize data array to all zeros
    *data = [0; BLOCK_SIZE];

    // decode one bit plane at a time from MSB to LSB
    while bits != 0 && k > kmin {
        k -= 1;
        // step 1: decode first n bits of bit plane #k
        let m = n.min(bits);
        bits -= m;
        let mut x = s.read_bits(m);
        // step 2: unary run-length decode remainder of bit plane
        while bits != 0 && n < size {
            bits -= 1;
            if s.read_bit() != 0 {
                // positive group test; scan for next one-bit
                while bits != 0 && n < size - 1 {
                    bits -= 1;
                    if s.read_bit() != 0 {
                        break;
                    }
                    n += 1;
                }
                // set bit and continue decoding bit plane
                x += 1u64 << n;
            } else {
                // negative group test; done with bit plane
                break;
            }
            n += 1;
        }
        // step 3: deposit bit plane from x
        let mut i = 0;
        while x != 0 {
            data[i] = data[i].wrapping_add(((x & 1) as u32) << k);
            i += 1;
            x >>= 1;
        }
    }

    maxbits - bits
}

/// `decode_few_ints_prec`.
fn decode_few_ints_prec(s: &mut BitStream, maxprec: u32, data: &mut [u32; BLOCK_SIZE]) -> u32 {
    let offset = s.rtell();
    let size = BLOCK_SIZE as u32;
    let intprec = 32u32;
    let kmin = intprec.saturating_sub(maxprec);
    let (mut k, mut n) = (intprec, 0u32);

    // initialize data array to all zeros
    *data = [0; BLOCK_SIZE];

    // decode one bit plane at a time from MSB to LSB
    while k > kmin {
        k -= 1;
        // step 1: decode first n bits of bit plane #k
        let mut x = s.read_bits(n);
        // step 2: unary run-length decode remainder of bit plane
        while n < size && s.read_bit() != 0 {
            while n < size - 1 && s.read_bit() == 0 {
                n += 1;
            }
            x += 1u64 << n;
            n += 1;
        }
        // step 3: deposit bit plane from x
        let mut i = 0;
        while x != 0 {
            data[i] = data[i].wrapping_add(((x & 1) as u32) << k);
            i += 1;
            x >>= 1;
        }
    }

    (s.rtell() - offset) as u32
}

/// `encode_block` of integers: transformed, reordered, and their planes of bits, with at
/// least `minbits` bits.
fn encode_int_block(
    s: &mut BitStream,
    minbits: u32,
    maxbits: u32,
    maxprec: u32,
    iblock: &mut [i32; BLOCK_SIZE],
) -> u32 {
    // perform decorrelating transform: along x, then along y
    for y in 0..4 {
        fwd_lift(iblock, 4 * y, 1);
    }
    for x in 0..4 {
        fwd_lift(iblock, x, 4);
    }
    // reorder signed coefficients and convert to unsigned integer
    let mut ublock = [0u32; BLOCK_SIZE];
    for (u, &at) in ublock.iter_mut().zip(PERM_2.iter()) {
        *u = (iblock[at] as u32).wrapping_add(NBMASK) ^ NBMASK;
    }
    // encode integer coefficients
    let mut bits = if with_maxbits(maxbits, maxprec, BLOCK_SIZE as u32) {
        encode_few_ints(s, maxbits, maxprec, &ublock)
    } else {
        encode_few_ints_prec(s, maxprec, &ublock)
    };
    // write at least minbits bits by padding with zeros
    if bits < minbits {
        s.pad(minbits - bits);
        bits = minbits;
    }
    bits
}

/// `decode_block` of integers.
fn decode_int_block(
    s: &mut BitStream,
    minbits: u32,
    maxbits: u32,
    maxprec: u32,
    iblock: &mut [i32; BLOCK_SIZE],
) -> u32 {
    let mut ublock = [0u32; BLOCK_SIZE];
    // decode integer coefficients
    let mut bits = if with_maxbits(maxbits, maxprec, BLOCK_SIZE as u32) {
        decode_few_ints(s, maxbits, maxprec, &mut ublock)
    } else {
        decode_few_ints_prec(s, maxprec, &mut ublock)
    };
    // read at least minbits bits
    if bits < minbits {
        s.skip(minbits - bits);
        bits = minbits;
    }
    // reorder unsigned coefficients and convert to signed integer
    for (&u, &at) in ublock.iter().zip(PERM_2.iter()) {
        iblock[at] = (u ^ NBMASK).wrapping_sub(NBMASK) as i32;
    }
    // perform decorrelating transform: along y, then along x
    for x in 0..4 {
        inv_lift(iblock, x, 4);
    }
    for y in 0..4 {
        inv_lift(iblock, 4 * y, 1);
    }
    bits
}

/// `zfp_encode_block_float_2`: a block of four by four floats, row by row, coded into `s` with
/// the settings `zfp`. The answer is the bits written.
pub fn encode_block_float_2(zfp: &ZfpStream, s: &mut BitStream, fblock: &[f32; BLOCK_SIZE]) -> u32 {
    let mut bits = 1u32;
    // compute maximum exponent
    let emax = exponent_block(fblock);
    let maxprec = precision(emax, zfp.maxprec, zfp.minexp);
    let e = if maxprec != 0 {
        (emax + EBIAS) as u32
    } else {
        0
    };
    // encode block only if biased exponent is nonzero
    if e != 0 {
        // encode common exponent; LSB indicates that exponent is nonzero
        bits += EBITS;
        s.write_bits(u64::from(2 * e + 1), bits);
        // perform forward block-floating-point transform
        let scale = power_of_two(30 - emax);
        let mut iblock = [0i32; BLOCK_SIZE];
        for (i, &f) in iblock.iter_mut().zip(fblock.iter()) {
            *i = float_to_int(scale * f);
        }
        // encode integer block
        bits += encode_int_block(
            s,
            zfp.minbits - bits.min(zfp.minbits),
            zfp.maxbits - bits,
            maxprec,
            &mut iblock,
        );
    } else {
        // write single zero-bit to indicate that all values are zero
        s.write_bit(0);
        if zfp.minbits > bits {
            s.pad(zfp.minbits - bits);
            bits = zfp.minbits;
        }
    }
    bits
}

/// `zfp_decode_block_float_2`: a block of four by four floats, row by row, decoded from `s`
/// with the settings `zfp` (those it was coded with). The answer is the bits read.
pub fn decode_block_float_2(
    zfp: &ZfpStream,
    s: &mut BitStream,
    fblock: &mut [f32; BLOCK_SIZE],
) -> u32 {
    let mut bits = 1u32;
    // test if block has nonzero values
    if s.read_bit() != 0 {
        // decode common exponent
        bits += EBITS;
        let emax = s.read_bits(EBITS) as i32 - EBIAS;
        let maxprec = precision(emax, zfp.maxprec, zfp.minexp);
        // decode integer block
        let mut iblock = [0i32; BLOCK_SIZE];
        bits += decode_int_block(
            s,
            zfp.minbits - bits.min(zfp.minbits),
            zfp.maxbits - bits,
            maxprec,
            &mut iblock,
        );
        // perform inverse block-floating-point transform
        let scale = power_of_two(emax - 30);
        for (f, &i) in fblock.iter_mut().zip(iblock.iter()) {
            *f = scale * i as f32;
        }
    } else {
        // set all values to zero
        *fblock = [0.0; BLOCK_SIZE];
        if zfp.minbits > bits {
            s.skip(zfp.minbits - bits);
            bits = zfp.minbits;
        }
    }
    bits
}
