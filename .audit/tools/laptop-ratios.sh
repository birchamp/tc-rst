#!/bin/bash
# Runs the reference laptop's own foldbench (storage 58b6874, docs/evidence/
# storage-fold-windows-g0-2026-10-17.md) on this machine, shaped like the laptop (4 performance
# cores with 2 threads each: taskset 0-7, RAYON_NUM_THREADS=8, 4 reading threads), with the same
# invocations the Windows runner (spikes/foldbench-windows/run-foldbench.ps1) used for each row,
# so laptop-ratios.py can divide the laptop's medians by this machine's.
#
#   laptop-ratios.sh FOLDBENCH CORPUS_DIR OUT_DIR [ROUNDS]
set -u
FB=$1; DIR=$2; OUT=$3; ROUNDS=${4:-5}
mkdir -p "$OUT"
export RAYON_NUM_THREADS=8
run() { taskset -c 0-7 "$FB" "$@"; }
warm() { find "$DIR" -type f -print0 | xargs -0 cat > /dev/null; }
for r in $(seq 1 "$ROUNDS"); do
  echo "round $r load $(cut -d' ' -f1-3 /proc/loadavg)" | tee -a "$OUT/rounds.txt"
  rm -f "$DIR/index.bin"; warm
  FOLDBENCH_FIRST_ONLY=1 FOLDBENCH_RUNS=1 run run "$DIR" 4 > "$OUT/c-$r.txt" 2> "$OUT/c-$r.err"
  warm
  FOLDBENCH_CACHED_ONLY=1 FOLDBENCH_RUNS=1 run run "$DIR" 4 > "$OUT/a-$r.txt" 2> "$OUT/a-$r.err"
  warm
  FOLDBENCH_PSALMS_ONLY=1 FOLDBENCH_RUNS=1 run run "$DIR" 4 > "$OUT/p-$r.txt" 2> "$OUT/p-$r.err"
  warm
  /usr/bin/time -v taskset -c 0-7 "$FB" app "$DIR" 4 > "$OUT/app-$r.txt" 2> "$OUT/app-$r.err"
  sleep 2
done
warm
run run "$DIR" 4 > "$OUT/every.txt" 2> "$OUT/every.err"
echo "done load $(cut -d' ' -f1-3 /proc/loadavg)" | tee -a "$OUT/rounds.txt"
