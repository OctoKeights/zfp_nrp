# Performance

[Overview](../README.md) · [Guide](guide.md) · [Verification](verification.md) · **Performance**

---

`examples/bench.rs` codes 200,000 blocks of smooth synthetic data (12.8 MB of `f32`
values) in each mode, and reports the best of nine runs:

```sh
cargo run --release --example bench
```

## Throughput

Measured on one core of an Intel Xeon at 2.1 GHz, built for generic x86-64:

| Mode | Encode | Decode |
|---|---|---|
| Fixed rate, 8 bits per value | 169 MB/s | 215 MB/s |
| Fixed precision, 20 bit planes | 159 MB/s | 195 MB/s |
| Fixed accuracy, 10⁻³ | 152 MB/s | 168 MB/s |
| Defaults (near-lossless) | 126 MB/s | 113 MB/s |

## Compared with 0.1

Version 0.2 rewrote the coder as idiomatic Rust. Measured alternately with 0.1 on the same
machine:

| Mode | Encode | Decode |
|---|---|---|
| Fixed rate | 5% faster | Unchanged |
| Fixed precision | 2% faster | Unchanged |
| Fixed accuracy | 4% faster | Unchanged |
| Defaults | 8% faster | Unchanged |

"Unchanged" means within ±1%, the run-to-run variation of the measurement.
