# zfp_nrp

**ZFP in pure, safe Rust: a native Rust port of the zfp library's block coder for floats.**

zfp_nrp is a line-for-line port of the parts of [zfp](https://github.com/LLNL/zfp) v1.0.1 that
code blocks of four by four floats: the settings of a stream in its fixed-rate,
fixed-precision and fixed-accuracy modes, its stream of bits, and its coder, which writes the
bits zfp writes and reads them back to the floats zfp reads.

## Highlights

- **Bit-identical blocks.** The block-floating-point transform, the decorrelating transform
  and the embedded coding of planes of bits, as zfp has them, at every rate, precision and
  tolerance.

- **Every stream read.** A damaged stream decodes to what zfp decodes it to.

- **Zero dependencies.** Standard library only: no C compiler, no build script.

- **No `unsafe` code.** Enforced by `#![forbid(unsafe_code)]`.

## Installation

```toml
[dependencies]
zfp_nrp = "0.1"
```

## Quick start

```rust
use zfp_nrp::{BitStream, ZfpStream, decode_block_float_2, encode_block_float_2};

// Eight bits a value: a fixed rate, 128 bits a block.
let mut zfp = ZfpStream::open();
assert_eq!(zfp.set_rate(8.0), 8.0);

// A stream with room for a field of 8 by 4 floats: two blocks.
let mut stream = BitStream::new(vec![0; zfp.maximum_size(8, 4) / 8]);
let blocks: [[f32; 16]; 2] = [
    core::array::from_fn(|k| k as f32 * 0.25),
    core::array::from_fn(|k| (k as f32).sqrt()),
];
for block in &blocks {
    assert_eq!(encode_block_float_2(&zfp, &mut stream, block), 128);
}
stream.flush();
let bytes = stream.to_bytes();
assert_eq!(bytes.len(), 32);

// Back again, nearly as they were.
let mut stream = BitStream::from_bytes(&bytes, bytes.len() / 8);
for block in &blocks {
    let mut back = [0f32; 16];
    decode_block_float_2(&zfp, &mut stream, &mut back);
    assert!(block.iter().zip(&back).all(|(a, b)| (a - b).abs() < 0.05));
}
```

## What is ported

| Original | What it does |
|---|---|
| `zfp.c` | The settings of a stream (`ZfpStream`): `zfp_stream_open`, `zfp_stream_set_rate` (for floats in two dimensions, unaligned), `zfp_stream_set_precision`, `zfp_stream_set_accuracy`, `zfp_stream_maximum_size` (for a field of floats in two dimensions) |
| `bitstream.inl` | The stream of bits (`BitStream`), with its default word of 64 bits |
| `template/encode*.c`, `template/decode*.c`, `template/codec*.c` | A block of four by four floats coded and decoded (`encode_block_float_2`, `decode_block_float_2`), without `ZFP_WITH_DAZ` and with the rounding zfp has by default |

Not ported: other types (doubles and integers) and dimensions, whole fields (`zfp_compress`,
`zfp_decompress`) with their headers and partial blocks, the reversible and expert modes,
other words for the stream, and threads.

## Differences from the original

The code is translated from C to Rust. The settings are a struct of their own and the stream
of bits is passed beside them, rather than held in them; the settings are set only by the
three modes (and zfp's defaults), so none of them can ask for the reversible mode, which is
not ported. The stream's words are a `Vec`: past its end, a read gives zeros and a write adds a
word, where the original reads and writes whatever memory follows (a stream as long as
`maximum_size` says has room for all its blocks, written or read, whatever their bits say).
Where the original converts a double or a float to an integer out of its range or not a
number, the port does as x86-64 does. What they compute is kept: the same bits, bit for bit, as
zfp writes for a block with the same settings, and the same floats from any bits.

## Verification

`compare/compare.sh` builds zfp's library from a checkout, `compare/dump_zfp.c` against it, and
`examples/dump.rs` against the port; it runs both over the same made-up data and compares what
they print byte for byte:

```sh
compare/compare.sh <checkout of zfp> [count] [seeds] [dumps]
```

| Dump | What it covers | Last full run |
|---|---|---|
| `zfp` | Made-up settings in each mode, among them zero, negative, huge, infinite and not-a-number rates and tolerances, with the most bytes of a made-up field; blocks of eight kinds (zeros, smooth values, random bits with infinities and not-a-number values, subnormals, a single value, the largest exponents, small integers) coded with them; each stream decoded whole, with some of its bits flipped, and made-up words decoded | 600,000 streams of up to 40 blocks, 11.5 million blocks, identical (built by gcc and by clang) |

The coder is also what [tinyexr_nrp](https://github.com/OctoKeights/tinyexr_nrp) uses for
tinyexr's ZFP compression, compared there with tinyexr built against zfp.

`cargo test` needs no C compiler: the tests check the port against hashes recorded from zfp
(`tests/data`).

## Licence

As zfp is: BSD 3-Clause ([LICENSE](LICENSE), which keeps zfp's copyright notice and the
notice that comes with it).
