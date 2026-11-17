# Verification

[Overview](../README.md) · [Guide](guide.md) · **Verification** · [Performance](performance.md)

---

zfp_nrp is checked against zfp itself: both are run over the same synthetic data and their
output compared byte for byte.

## The comparison

`compare/compare.sh` builds zfp's library from a checkout, builds `compare/dump_zfp.c`
against it and `examples/dump.rs` against zfp_nrp, runs both, and compares what they print:

```sh
compare/compare.sh <checkout of zfp 1.0.1> [count] [seeds]
```

Both programs generate the same data from each seed and print a running hash of everything
they produce:

- **Settings** in every mode, including zero, negative, huge, infinite and NaN rates and
  tolerances, with the limits each sets and the maximum stream size of a random field.

- **Blocks** of eight kinds: zeros, smooth values, random bit patterns (including infinities
  and NaN), subnormals, a single non-zero value, the largest exponents, and small integers.
  Each block's bit count and the stream's bytes are hashed.

- **Decoding** of every stream as written, with some of its bits flipped, and of random
  words.

## Results

| zfp built by | Streams | Blocks | Result |
|---|---|---|---|
| gcc | 600,000 | 11.5 million | Identical |
| clang | 400,000 | 7.7 million | Identical |

zfp_nrp is also the coder behind tinyexr's ZFP compression in
[tinyexr_nrp](https://github.com/OctoKeights/tinyexr_nrp), which is compared there with
tinyexr built against zfp.

## Tests

`cargo test` needs no C compiler. `tests/golden.rs` checks zfp_nrp against hashes recorded
from zfp (`tests/data/zfp_expected.txt`, 2,400 streams), and the examples in the README and
the Guide are run as tests. `tests/data/README.md` explains how to record the hashes again.
