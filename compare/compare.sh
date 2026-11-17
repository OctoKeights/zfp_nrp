#!/bin/sh
# Compares zfp_nrp with the zfp library (https://github.com/LLNL/zfp) v1.0.1.
#
#   compare/compare.sh <checkout of zfp> [count] [seeds] [dumps]
#
# compare/dump_zfp.c is built against zfp's library with its default configuration, and
# examples/dump.rs against zfp_nrp. Both run the same synthetic data for each seed, and their
# output is compared byte for byte. The zfp dump runs a tenth of [count] (200,000 by default)
# streams, each with its own settings and up to 40 blocks.
set -u
if [ $# -lt 1 ]; then
    echo "usage: $0 <checkout of zfp> [count] [seeds] [dumps]" >&2
    exit 2
fi
here=$(cd "$(dirname "$0")/.." && pwd)
source=$1
blocks=${2:-200000}
seeds=${3:-3}
dumps=${4:-zfp}
tmp=${TMPDIR:-/tmp}/zfp_nrp-compare
mkdir -p "$tmp" || exit 2
if ! [ -f "$source/include/zfp.h" ] && [ -f "$source/src/zfp.c" ]; then
    echo "$source does not look like checkout of zfp" >&2
    exit 2
fi

(cd "$here" && cargo build --release -q --ignore-rust-version --example dump) || exit 2
rust=$(cd "$here" && cargo metadata --format-version 1 --no-deps 2>/dev/null | sed 's/.*"target_directory":"\([^"]*\)".*/\1/')/release/examples/dump

mkdir -p "$tmp/obj" || exit 2
(cd "$tmp/obj" && rm -f ./*.o && ${CC:-cc} -O2 -fwrapv -ffp-contract=off -c -I "$source/include" "$source"/src/*.c) || exit 2
${CC:-cc} -O2 -fwrapv -ffp-contract=off -I "$source/include" "$here/compare/dump_zfp.c" "$tmp"/obj/*.o -lm -o "$tmp/dump-zfp" || exit 2

bad=0
for what in $dumps; do
    extra=
    case $what in
    zfp)
        reference=$tmp/dump-zfp
        count=$((blocks / 10))
        ;;
    *)
        echo "unknown dump $what" >&2
        exit 2
        ;;
    esac
    seed=1
    while [ "$seed" -le "$seeds" ]; do
        # shellcheck disable=SC2086
        "$reference" "$what" "$seed" "$count" $extra > "$tmp/reference.txt" 2> /dev/null
        # shellcheck disable=SC2086
        "$rust" "$what" "$seed" "$count" $extra > "$tmp/rust.txt"
        if [ ! -s "$tmp/reference.txt" ]; then
            echo "$what, seed $seed: NOT COMPARED, the original's side printed nothing"
            bad=$((bad + 1))
        elif cmp -s "$tmp/reference.txt" "$tmp/rust.txt"; then
            echo "$what, seed $seed: the same ($(tail -1 "$tmp/rust.txt"))"
        else
            echo "$what, seed $seed: DIFFERENT"
            bad=$((bad + 1))
        fi
        seed=$((seed + 1))
    done
done
[ "$bad" -eq 0 ]
