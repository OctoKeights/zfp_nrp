# Guide

[Overview](../README.md) · **Guide** · [Verification](verification.md) · [Performance](performance.md)

---

- [How zfp codes a block](#how-zfp-codes-a-block)
- [Choosing a mode](#choosing-a-mode)
- [Coding blocks one at a time](#coding-blocks-one-at-a-time)
- [Correspondence with zfp](#correspondence-with-zfp)
- [Differences from zfp](#differences-from-zfp)

## How zfp codes a block

A block is 4 × 4 `f32` values, row by row (`Block`). zfp stores it in four steps:

1. **Common exponent.** The exponent of the largest magnitude is written once, and every
   value becomes an integer scaled by it.

2. **Decorrelation.** A reversible lifting transform along rows and then columns leaves most
   coefficients of smooth data small.

3. **Reordering.** The coefficients are ordered from low to high frequency and mapped so
   that small magnitudes of either sign have few significant bits.

4. **Bit planes.** The coefficients are written one bit plane at a time, from the most
   significant down, each plane run-length coded. Coding stops when the mode's limit is
   reached.

An all-zero block takes a single bit.

## Choosing a mode

| Mode | Constructor | Each block takes | Use it for |
|---|---|---|---|
| Fixed rate | `Settings::fixed_rate(bits_per_value)` | Exactly `16 × rate` bits (at least 9) | A known size, and random access to blocks |
| Fixed precision | `Settings::fixed_precision(planes)` | As many bits as `planes` bit planes take | A relative error bound |
| Fixed accuracy | `Settings::fixed_accuracy(tolerance)` | As many bits as reaching `tolerance` takes | An absolute error bound |
| Defaults | `Settings::new()` | Up to 16,658 bits | Near-lossless coding |

Every constructor gives a valid setting, so there is no invalid state to check for. The
limits each mode sets can be read back with `min_bits`, `max_bits`, `max_precision` and
`min_exponent`.

`Settings::max_stream_len(width, height)` gives the most bytes a field of that size can
take in the chosen mode, as `zfp_stream_maximum_size` does.

## Coding blocks one at a time

`Settings::compress` and `Settings::decompress` code a run of blocks. To mix blocks with
other data, or to code a field block by block, use a `BitStream` directly:

```rust
use zfp_nrp::{BitStream, Block, Settings};

let settings = Settings::fixed_precision(16);
let block: Block = core::array::from_fn(|k| k as f32);

let mut stream = BitStream::new(vec![0; settings.max_stream_len(4, 4) / 8]);
let bits = settings.encode_block(&mut stream, &block);
stream.flush();

let mut stream = BitStream::from_bytes(&stream.to_bytes(), 0);
let mut decoded = [0.0; 16];
assert_eq!(settings.decode_block(&mut stream, &mut decoded), bits);
```

A stream is written or read from the start; `rewind` returns to it, and `seek` and `skip`
move the read position. Reading past the end gives zeros, as a zfp stream with zeroed
memory after it would.

## Correspondence with zfp

| zfp | zfp_nrp |
|---|---|
| `zfp_stream_open` | `Settings::new` |
| `zfp_stream_set_rate` (floats, two dimensions, unaligned) | `Settings::fixed_rate` |
| `zfp_stream_set_precision` | `Settings::fixed_precision` |
| `zfp_stream_set_accuracy` | `Settings::fixed_accuracy` |
| `zfp_stream_maximum_size` | `Settings::max_stream_len` |
| `zfp_encode_block_float_2` | `Settings::encode_block` |
| `zfp_decode_block_float_2` | `Settings::decode_block` |
| `bitstream.inl` (64-bit words) | `BitStream` |
| `ZFP_MIN_BITS`, `ZFP_MAX_BITS`, `ZFP_MAX_PREC`, `ZFP_MIN_EXP` | `MIN_BITS`, `MAX_BITS`, `MAX_PRECISION`, `MIN_EXPONENT` |

zfp's `set_` functions return the rate, precision or tolerance they set. Here the same
values follow from the settings: the rate is `max_bits / 16`, the precision is
`max_precision`, and the tolerance is 2 to the power `min_exponent` (or zero for a
tolerance of zero or less).

## Differences from zfp

The code is written as idiomatic Rust rather than translated line by line; what it computes
is unchanged. The differences are in the interface:

- **Settings and stream are separate.** zfp's `zfp_stream` holds both; here the bit stream
  is passed to each call.

- **Only valid settings.** zfp's expert mode, which sets the four limits directly, is not
  offered, so its reversible mode cannot be asked for; every `Settings` codes as zfp does.

- **Memory past the stream.** zfp reads and writes whatever memory follows its buffer.
  Here, reading past the end gives zeros and writing past it adds words. A stream as long
  as `max_stream_len` allows has room for all its blocks either way.

- **Conversions out of range.** Where zfp converts a double or float to an integer that
  cannot hold it, or converts NaN, the result is what x86-64 gives, on every platform.
