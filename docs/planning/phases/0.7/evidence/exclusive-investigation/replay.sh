#!/bin/sh
# Run in the investigation checkout. Supply a new directory name for builds.
set -eu
driver=$(python3 -c 'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ required"; print(sys.executable)')
repo=$(git rev-parse --show-toplevel)
toolsdir="$repo/docs/planning/phases/0.7/evidence/exclusive-investigation"
mkdir "$1"
cd "$1"
cp "$toolsdir/run.py" "$toolsdir/summarize.py" .
for side in baseline candidate; do
    case "$side" in
        baseline) revision=2d638e761a26211e5c98f1792731103dd9856994 ;;
        candidate) revision=b57017f07efc76b59062cc87f66bd0df4022f659 ;;
    esac
    mkdir "$side"
    git -C "$repo" archive "$revision" > source.tar
    tar xf source.tar -C "$side"
    (cd "$side" && cargo build --release --locked --example benchmark_phase06)
    cp "$side/target/release/examples/benchmark_phase06" "$side-stock"
    (cd "$side" && patch -p1 < "$toolsdir/$side.patch")
    (cd "$side" && cargo build --release --locked --example benchmark_phase06)
    cp "$side/target/release/examples/benchmark_phase06" "$side-probe"
done
unset G4_SKIP_FILTER
"$driver" run.py
"$driver" summarize.py
