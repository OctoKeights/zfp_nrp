# zfp_nrp

**ZFP compression of floating-point blocks in pure, safe Rust.**

**Overview** · [Guide](docs/guide.md) · [Verification](docs/verification.md) · [Performance](docs/performance.md)

---

zfp_nrp is a native Rust port of the block coder of the [zfp](https://github.com/LLNL/zfp)
library v1.0.1, by Lawrence Livermore National Laboratory. It compresses 4 × 4 blocks of
`f32` values at a fixed rate, a fixed precision or a fixed error tolerance, and writes
exactly the bits zfp writes.

## Highlights

- **Bit-identical output.** Every block matches zfp's, bit for bit, in every mode.

- **Exact decoding.** Any stream, including a damaged one, decodes to the values zfp
  decodes it to.

- **Zero dependencies.** Standard library only: no C compiler, no build script.

- **No `unsafe` code.** Enforced by `#![forbid(unsafe_code)]`.

## Installation

```toml
[dependencies]
zfp_nrp = "0.2"
```

## Quick start

```rust
use zfp_nrp::{Block, Settings};

// Eight bits per value: every block takes exactly 128 bits.
let settings = Settings::fixed_rate(8.0);
let blocks: [Block; 2] = [
    core::array::from_fn(|k| k as f32 * 0.25),
    core::array::from_fn(|k| (k as f32).sqrt()),
];

let bytes = settings.compress(&blocks);
assert_eq!(bytes.len(), 32);

let decoded = settings.decompress(&bytes, blocks.len());
for (block, decoded) in blocks.iter().zip(&decoded) {
    assert!(block.iter().zip(decoded).all(|(a, b)| (a - b).abs() < 0.05));
}
```

For blocks interleaved with other data, `Settings::encode_block` and
`Settings::decode_block` work one block at a time on a `BitStream`. See the
[Guide](docs/guide.md).

## Scope

zfp_nrp ports the part of zfp that codes 4 × 4 blocks of `f32`, which is what
[tinyexr_nrp](https://github.com/OctoKeights/tinyexr_nrp) uses for tinyexr's ZFP compression
of `.exr` files. Doubles, integers, one, three and four dimensions, whole fields with
partial blocks and headers, and the reversible mode are not ported.

## Licence

BSD 3-Clause, as zfp is; see [`LICENSE`](LICENSE), which keeps zfp's copyright notice.

Bug reports and contributions are welcome, above all any block that zfp_nrp codes or
decodes differently from zfp.
