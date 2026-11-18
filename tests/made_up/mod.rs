//! Synthetic data, generated from a seed exactly as the C side of the comparison
//! (`compare/dump_zfp.c`) generates it, and the dump that both the golden tests and
//! `examples/dump.rs` run.

// (The golden tests and the dump each use only part of this module.)
#![allow(dead_code)]

/// The two programs make the same blocks from the same seed: a 64-bit linear congruential
/// generator, the high half of its state.
pub struct Random {
    pub state: u64,
}

impl Random {
    pub fn next(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
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
            value: 14_695_981_039_346_656_037,
        }
    }

    pub fn add(&mut self, data: &[u8]) {
        for &byte in data {
            self.value ^= u64::from(byte);
            self.value = self.value.wrapping_mul(1_099_511_628_211);
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
            let bits = if !r.next().is_multiple_of(2) {
                0x8000_0000
            } else {
                0
            };
            block = [f32::from_bits(bits); 16];
        }
        1 => {
            // A plane, scaled by a power of two.
            let base = (r.next() % 2001) as i32 - 1000;
            let slope_x = (r.next() % 201) as i32 - 100;
            let slope_y = (r.next() % 201) as i32 - 100;
            let exponent = (r.next() % 60) as i32 - 30;
            let scale = f32::from_bits(((exponent + 127) as u32) << 23);
            for (k, value) in block.iter_mut().enumerate() {
                let (x, y) = ((k % 4) as i32, (k / 4) as i32);
                *value = (base + slope_x * x + slope_y * y) as f32 * scale;
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

/// The synthetic settings, blocks and streams of `compare/dump_zfp.c`, hashed.
pub fn zfp(seed: u64, count: u32) -> String {
    use std::fmt::Write;
    use zfp_nrp::{BitStream, Settings};
    fn add(hash: &mut Hash, value: u32) {
        hash.add(&value.to_le_bytes());
    }
    fn decode_all(settings: &Settings, hash: &mut Hash, bytes: &[u8], words: usize, blocks: usize) {
        let mut stream = BitStream::from_bytes(bytes, words);
        for _ in 0..blocks {
            let mut block = [0f32; 16];
            add(hash, settings.decode_block(&mut stream, &mut block) as u32);
            for value in block {
                add(hash, value.to_bits());
            }
        }
        add(hash, stream.read_position() as u32);
    }
    let mut r = Random { state: seed };
    let mut hash = Hash::new();
    let mut out = String::new();
    let mut total = 0u64;
    for round in 0..count {
        // What zfp's zfp_stream_set_* functions return is hashed too: the rate, the precision
        // or the tolerance they set.
        let mode = r.next() % 4;
        add(&mut hash, mode);
        let settings = match mode {
            0 => {
                let settings = Settings::fixed_rate(make_rate(&mut r));
                let rate = f64::from(settings.max_bits()) / 16.0;
                hash.add(&rate.to_bits().to_le_bytes());
                settings
            }
            1 => {
                let settings = Settings::fixed_precision(r.next() % 70);
                add(&mut hash, settings.max_precision());
                settings
            }
            2 => {
                let tolerance = make_tolerance(&mut r);
                let settings = Settings::fixed_accuracy(tolerance);
                let set = if tolerance > 0.0 {
                    power_of_two(settings.min_exponent())
                } else {
                    0.0
                };
                hash.add(&set.to_bits().to_le_bytes());
                settings
            }
            _ => Settings::new(),
        };
        add(&mut hash, settings.min_bits());
        add(&mut hash, settings.max_bits());
        add(&mut hash, settings.max_precision());
        add(&mut hash, settings.min_exponent() as u32);
        let (nx, ny) = ((r.next() % 300) as usize, (r.next() % 300) as usize);
        add(&mut hash, settings.max_stream_len(nx, ny) as u32);

        let blocks = 1 + (r.next() % 40) as usize;
        let words = settings.max_stream_len(4 * blocks, 4) / 8;
        // (Settings of more than eight megabytes a stream are left at that.)
        if words <= 1 << 20 {
            let mut stream = BitStream::new(vec![0; words]);
            for _ in 0..blocks {
                let block = make_block(&mut r);
                add(&mut hash, settings.encode_block(&mut stream, &block) as u32);
            }
            add(&mut hash, stream.flush());
            let mut bytes = stream.to_bytes();
            add(&mut hash, bytes.len() as u32);
            hash.add(&bytes);
            decode_all(&settings, &mut hash, &bytes, words, blocks);

            if r.next().is_multiple_of(2) && !bytes.is_empty() {
                let flips = 1 + r.next() % 4;
                for _ in 0..flips {
                    let bit = r.next() % (bytes.len() as u32 * 8);
                    bytes[(bit >> 3) as usize] ^= 1 << (bit & 7);
                }
                decode_all(&settings, &mut hash, &bytes, words, blocks);
            }
            if round % 4 == 0 {
                let garbage: Vec<u8> = (0..words * 8).map(|_| r.next() as u8).collect();
                decode_all(&settings, &mut hash, &garbage, words, blocks);
            }
            total += blocks as u64;
        }
        if (round + 1) % 200 == 0 || round + 1 == count {
            writeln!(out, "zfp {} {:016x}", round + 1, hash.value).unwrap();
        }
    }
    writeln!(out, "zfp blocks: {total}").unwrap();
    out
}
