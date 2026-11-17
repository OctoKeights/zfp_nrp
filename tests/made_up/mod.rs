//! Made-up data, the same from a seed here as in the C side of the comparison
//! (`compare/`), and the dumps the tests run as well as the comparison (`examples/dump.rs`).

// (The arithmetic is written as the C side has it; not every user of this module uses all of
// it.)
#![allow(
    clippy::manual_is_multiple_of,
    clippy::needless_range_loop,
    clippy::manual_div_ceil,
    clippy::manual_checked_ops,
    dead_code
)]

/// The two programs make the same blocks from the same seed: a 64-bit linear congruential
/// generator, the high half of its state.
pub struct Random {
    pub state: u64,
}

impl Random {
    pub fn next(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }
}

/// FNV-1a, 64 bits.
pub struct Hash {
    pub value: u64,
}

impl Hash {
    pub fn new() -> Hash {
        Hash {
            value: 14695981039346656037,
        }
    }

    pub fn add(&mut self, data: &[u8]) {
        for &byte in data {
            self.value ^= u64::from(byte);
            self.value = self.value.wrapping_mul(1099511628211);
        }
    }
}

/// Two to `e`, for `e` from -1074 to 1023.
fn power_of_two(e: i32) -> f64 {
    if e >= -1022 {
        f64::from_bits(((e + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (e + 1074))
    }
}

fn make_rate(r: &mut Random) -> f64 {
    match r.next() % 8 {
        0 => f64::from(r.next() % 64) / 4.0,
        1 => f64::from(r.next() % 2000) / 16.0 - 10.0,
        2 => {
            let (lo, hi) = (r.next(), r.next());
            f64::from_bits(u64::from(lo) | (u64::from(hi) << 32))
        }
        3 => 1e10 * f64::from(r.next() % 10),
        4 => 0.5 + f64::from(r.next() % 100) * 0.01,
        5 => f64::from(r.next() % 33),
        6 => -1.5,
        _ => f64::from(r.next() % 1100),
    }
}

fn make_tolerance(r: &mut Random) -> f64 {
    match r.next() % 8 {
        0 => 0.0,
        1 => {
            let e = (r.next() % 60) as i32 - 30;
            power_of_two(e) * (1.0 + f64::from(r.next() % 100) / 100.0)
        }
        2 => {
            let (lo, hi) = (r.next(), r.next());
            f64::from_bits(u64::from(lo) | (u64::from(hi) << 32))
        }
        3 => {
            let (lo, hi) = (r.next(), r.next());
            f64::from_bits(u64::from(lo) | (u64::from(hi & 0xfffff) << 32))
        }
        4 => -1.0,
        5 => f64::INFINITY,
        6 => power_of_two(-((r.next() % 1074) as i32)),
        _ => f64::from(r.next() % 1000) * 1e-6,
    }
}

/// A block of one of eight kinds.
fn make_block(r: &mut Random) -> [f32; 16] {
    let mut block = [0f32; 16];
    match r.next() % 8 {
        0 => {
            let bits = if r.next() % 2 != 0 { 0x8000_0000 } else { 0 };
            block = [f32::from_bits(bits); 16];
        }
        1 => {
            let a = (r.next() % 2001) as i32 - 1000;
            let b = (r.next() % 201) as i32 - 100;
            let c = (r.next() % 201) as i32 - 100;
            let e = (r.next() % 60) as i32 - 30;
            let s = f32::from_bits(((e + 127) as u32) << 23);
            for j in 0..4 {
                for i in 0..4 {
                    block[j * 4 + i] = (a + b * i as i32 + c * j as i32) as f32 * s;
                }
            }
        }
        2 => {
            for v in &mut block {
                *v = f32::from_bits(r.next());
            }
        }
        3 => {
            for v in &mut block {
                let (a, b) = (r.next(), r.next());
                *v = f32::from_bits((a & 0x807f_ffff) | ((100 + b % 55) << 23));
            }
        }
        4 => {
            for v in &mut block {
                *v = f32::from_bits(r.next() & 0x807f_ffff);
            }
        }
        5 => {
            let k = (r.next() % 16) as usize;
            let (a, b) = (r.next(), r.next());
            block[k] = f32::from_bits((a & 0x807f_ffff) | ((1 + b % 254) << 23));
        }
        6 => {
            for v in &mut block {
                let (a, b) = (r.next(), r.next());
                *v = f32::from_bits((a & 0x807f_ffff) | ((250 + b % 5) << 23));
            }
        }
        _ => {
            for v in &mut block {
                *v = ((r.next() % 9) as i32 - 4) as f32;
            }
        }
    }
    block
}

/// The made-up settings, blocks and streams of `compare/dump_zfp.c`, hashed.
pub fn zfp(seed: u64, count: u32) -> String {
    use zfp_nrp::{BitStream, ZfpStream, decode_block_float_2, encode_block_float_2};
    let mut r = Random { state: seed };
    let mut hash = Hash::new();
    let mut out = String::new();
    let mut total = 0u64;
    fn add(hash: &mut Hash, value: u32) {
        hash.add(&value.to_le_bytes());
    }
    fn decode_all(zfp: &ZfpStream, hash: &mut Hash, bytes: &[u8], words: usize, nblocks: usize) {
        let mut s = BitStream::from_bytes(bytes, words);
        for _ in 0..nblocks {
            let mut block = [0f32; 16];
            add(hash, decode_block_float_2(zfp, &mut s, &mut block));
            for v in block {
                add(hash, v.to_bits());
            }
        }
        add(hash, s.rtell() as u32);
    }
    for round in 0..count {
        let mut zfp = ZfpStream::open();
        let mode = r.next() % 4;
        add(&mut hash, mode);
        if mode == 0 {
            let rate = make_rate(&mut r);
            hash.add(&zfp.set_rate(rate).to_bits().to_le_bytes());
        } else if mode == 1 {
            let precision = r.next() % 70;
            add(&mut hash, zfp.set_precision(precision));
        } else if mode == 2 {
            let tolerance = make_tolerance(&mut r);
            hash.add(&zfp.set_accuracy(tolerance).to_bits().to_le_bytes());
        }
        add(&mut hash, zfp.minbits());
        add(&mut hash, zfp.maxbits());
        add(&mut hash, zfp.maxprec());
        add(&mut hash, zfp.minexp() as u32);
        let (nx, ny) = ((r.next() % 300) as usize, (r.next() % 300) as usize);
        add(&mut hash, zfp.maximum_size(nx, ny) as u32);

        let nblocks = 1 + (r.next() % 40) as usize;
        let words = zfp.maximum_size(4 * nblocks, 4) / 8;
        // (Settings of more than eight megabytes a stream are left at that.)
        if words <= 1 << 20 {
            let mut s = BitStream::new(vec![0; words]);
            for _ in 0..nblocks {
                let block = make_block(&mut r);
                add(&mut hash, encode_block_float_2(&zfp, &mut s, &block));
            }
            add(&mut hash, s.flush());
            let mut bytes = s.to_bytes();
            add(&mut hash, bytes.len() as u32);
            hash.add(&bytes);
            decode_all(&zfp, &mut hash, &bytes, words, nblocks);

            if r.next() % 2 == 0 && !bytes.is_empty() {
                let flips = 1 + r.next() % 4;
                for _ in 0..flips {
                    let bit = r.next() % (bytes.len() as u32 * 8);
                    bytes[(bit >> 3) as usize] ^= 1 << (bit & 7);
                }
                decode_all(&zfp, &mut hash, &bytes, words, nblocks);
            }
            if round % 4 == 0 {
                let garbage: Vec<u8> = (0..words * 8).map(|_| r.next() as u8).collect();
                decode_all(&zfp, &mut hash, &garbage, words, nblocks);
            }
            total += nblocks as u64;
        }
        if (round + 1) % 200 == 0 || round + 1 == count {
            out += &format!("zfp {} {:016x}\n", round + 1, hash.value);
        }
    }
    out += &format!("zfp blocks: {total}\n");
    out
}
