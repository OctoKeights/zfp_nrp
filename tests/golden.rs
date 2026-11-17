//! zfp_nrp against hashes recorded from zfp, so that any change in output shows without a C
//! compiler.
//!
//! `tests/data/zfp_expected.txt` is what `compare/dump_zfp.c`, built against zfp 1.0.1,
//! prints for `zfp 3 600` and `zfp 8 600`: hashes of synthetic settings in every mode, of the
//! blocks coded with them, and of what each stream decodes to, intact, damaged and random.
//! `compare/compare.sh` runs the full comparison.

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
