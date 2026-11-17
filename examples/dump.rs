//! The Rust half of the comparison with zfp: runs the port over made-up data and
//! prints a running hash of what comes out, as `compare/dump_zfp.c` does with the original.
//! `compare/compare.sh` compares the two.
//!
//!   dump zfp <seed> <count>   made-up settings of a stream: a rate, a precision, a tolerance
//!                             or zfp's defaults, with the most bytes of a made-up field;
//!                             blocks of eight kinds coded with them, the stream flushed, and
//!                             decoded; now and then with some of its bits flipped; and made-up
//!                             words decoded

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
    match what.as_str() {
        "zfp" => print!("{}", made_up::zfp(seed, count)),
        _ => {
            eprintln!("unknown dump {what}");
            std::process::exit(2);
        }
    }
}
