//! The Rust side of the comparison with zfp: runs `zfp_nrp` over synthetic data and prints a
//! running hash of its output, as `compare/dump_zfp.c` does with zfp. `compare/compare.sh`
//! compares the two.
//!
//!   dump zfp <seed> <count>   synthetic settings in every mode, with the maximum stream size
//!                             of a random field; blocks of eight kinds coded with them; each
//!                             stream decoded intact, with bits flipped, and random words
//!                             decoded

#[path = "../tests/made_up/mod.rs"]
mod made_up;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(what), Some(seed), Some(count)) = (
        args.get(1),
        args.get(2).and_then(|s| s.parse::<u64>().ok()),
        args.get(3).and_then(|s| s.parse::<u32>().ok()),
    ) else {
        eprintln!("usage: dump <what> <seed> <count>   (see the top of dump.rs)");
        std::process::exit(2);
    };
    let _ = seed;
    if what.as_str() == "zfp" {
        print!("{}", made_up::zfp(seed, count));
    } else {
        eprintln!("unknown dump {what}");
        std::process::exit(2);
    }
}
