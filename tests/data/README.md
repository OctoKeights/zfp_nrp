# Recorded results

`zfp_expected.txt` holds the hashes `compare/dump_zfp.c` prints when built against zfp
1.0.1, for synthetic settings and blocks. `tests/golden.rs` checks that zfp_nrp prints the
same.

To record them again, run the comparison once, which builds zfp's side, then run that
program directly:

    compare/compare.sh <checkout of zfp> 3000 1
    ref="${TMPDIR:-/tmp}/zfp_nrp-compare/dump-zfp"
    { "$ref" zfp 3 600; "$ref" zfp 8 600; } > tests/data/zfp_expected.txt

All test data is generated from seeds; no external files are used.
