//! Throughput of the block coder in each mode, on smooth synthetic data.
//!
//! ```sh
//! cargo run --release --example bench
//! ```
//!
//! Each figure is the best of several runs, in megabytes of `f32` values per second.

use std::time::Instant;
use zfp_nrp::{BitStream, Block, Settings};

const BLOCKS: usize = 200_000;
const RUNS: usize = 9;

/// Smooth values with a little noise, as simulation or image data tends to be.
fn synthetic_blocks(count: usize) -> Vec<Block> {
    let mut state = 12345u64;
    let mut noise = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    (0..count)
        .map(|b| {
            let base = (b as f32 * 0.01).sin() * 100.0;
            core::array::from_fn(|k| {
                base + (k % 4) as f32 * 0.3 + (k / 4) as f32 * 0.2 + noise() * 0.05
            })
        })
        .collect()
}

fn main() {
    let blocks = synthetic_blocks(BLOCKS);
    let modes = [
        ("fixed rate 8", Settings::fixed_rate(8.0)),
        ("fixed precision 20", Settings::fixed_precision(20)),
        ("fixed accuracy 1e-3", Settings::fixed_accuracy(1e-3)),
        ("defaults", Settings::new()),
    ];
    let megabytes = (blocks.len() * size_of::<Block>()) as f64 / 1e6;
    for (name, settings) in modes {
        let words = settings.max_stream_len(4 * blocks.len(), 4) / 8;
        let (mut encode, mut decode) = (f64::MAX, f64::MAX);
        let mut checksum = 0u64;
        for _ in 0..RUNS {
            let start = Instant::now();
            let mut stream = BitStream::new(vec![0; words]);
            for block in &blocks {
                settings.encode_block(&mut stream, block);
            }
            stream.flush();
            encode = encode.min(start.elapsed().as_secs_f64());

            let bytes = stream.to_bytes();
            let start = Instant::now();
            let mut stream = BitStream::from_bytes(&bytes, words);
            let mut block = [0.0; 16];
            for _ in 0..blocks.len() {
                settings.decode_block(&mut stream, &mut block);
                checksum = checksum.wrapping_add(u64::from(block[5].to_bits()));
            }
            decode = decode.min(start.elapsed().as_secs_f64());
        }
        println!(
            "{name:20} encode {:6.1} MB/s   decode {:6.1} MB/s   (checksum {checksum:x})",
            megabytes / encode,
            megabytes / decode
        );
    }
}
