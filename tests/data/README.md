# Recorded results

`zfp_expected.txt` is what the zfp library makes of made-up settings and blocks, as
`compare/dump_zfp.c` prints it; `tests/golden.rs` checks that the port prints the same. To make
it again, run the comparison once (it builds the original's side), then that by itself:

    compare/compare.sh <checkout of zfp> 3000 1
    ref="${TMPDIR:-/tmp}/zfp_nrp-compare/dump-zfp"
    { "$ref" zfp 3 600; "$ref" zfp 8 600; } > tests/data/zfp_expected.txt

No picture or file from anywhere is in this repository: every file and stream the tests and
the comparison use is made up from a seed.
