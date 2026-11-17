//! The port of zfp against results recorded from the original, so that a change shows without
//! a C compiler at hand.
//!
//! `tests/data/zfp_expected.txt` is what `compare/dump_zfp.c`, built against zfp 1.0.1 (see
//! `compare/compare.sh`), prints for `zfp 3 600` and `zfp 8 600`: a hash of made-up settings
//! in each mode, of the blocks coded with them and of what each stream, whole, damaged and made
//! up, decodes to. `compare/compare.sh` is the full comparison.

mod made_up;

#[test]
fn codes_and_decodes_as_zfp_does() {
    let printed = [made_up::zfp(3, 600), made_up::zfp(8, 600)].concat();
    let expected = include_str!("data/zfp_expected.txt");
    for (line, (ours, theirs)) in printed.lines().zip(expected.lines()).enumerate() {
        assert_eq!(ours, theirs, "line {}", line + 1);
    }
    assert_eq!(printed.lines().count(), expected.lines().count());
}
