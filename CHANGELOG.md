# Changelog

## 0.2.0

- **Breaking: an idiomatic API.** `ZfpStream` is now `Settings`, built by one constructor
  per mode (`fixed_rate`, `fixed_precision`, `fixed_accuracy`, `new`), so every value is a
  valid setting. Blocks are coded with `Settings::encode_block` and
  `Settings::decode_block`, which replace `encode_block_float_2` and
  `decode_block_float_2`, and `maximum_size` is now `max_stream_len`. The constants lose
  their `ZFP_` prefix (`MAX_PRECISION`, `MIN_EXPONENT`), `Block` names a 4 × 4 block, and
  `BitStream`'s methods have descriptive names (`read_position`, `seek`, `byte_len`).

- **Compressing many blocks at once.** `Settings::compress` and `Settings::decompress` code
  a slice of blocks to bytes and back.

- **Rewritten as idiomatic Rust.** The coder no longer mirrors zfp's C line by line. Its
  output is unchanged, checked against zfp on 11.5 million blocks. Encoding is 2% to 8%
  faster and decoding unchanged (see [Performance](docs/performance.md)).

- **Matching zfp on empty fields.** `max_stream_len` gives zfp's answer for a field with no
  width (0) or no height (counted as one-dimensional).

- **Documentation.** Every public item is documented, the README is a short overview, and
  the detail is in [Guide](docs/guide.md), [Verification](docs/verification.md) and
  [Performance](docs/performance.md).

## 0.1.0

- First release: the zfp block coder, moved out of
  [rustbcx](https://github.com/OctoKeights/rustbcx), where it was private to the tinyexr
  port.
