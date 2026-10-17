#!/bin/bash
# Head-to-head after the VM restart (warm page cache): interleaved tC4, rebuild, tC4, rebuild...
# Load recorded before and after every run; a sampler logs the top CPU users every 20 s.
S=/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/journal-decision
T=/home/user/unfoldingword/translationcore4
R=$S/bench-target/release/examples/foldbench
H=$S/bench-target/release/examples/h2h
C=$S/corpora
OUT=$S/results/rounds2
mkdir -p $OUT
FIRST=${1:-1}; LAST=${2:-5}
TIMEFORMAT='process: real %R s, user %U s, sys %S s'
( while true; do echo "== $(date -u +%H:%M:%S) $(cat /proc/loadavg)"; top -b -n1 -o %CPU | sed -n '8,12p'; sleep 20; done ) >> $OUT/contention.log 2>&1 &
SAMPLER=$!
trap 'kill $SAMPLER' EXIT
step() {
  local name=$1; shift
  local f=$OUT/r$i-$name.txt
  { echo "### $name round $i"; echo "start $(date -u +%H:%M:%S) $(cat /proc/loadavg)"; } > $f
  { time "$@" ; } >> $f 2>&1
  echo "end $(date -u +%H:%M:%S) $(cat /proc/loadavg)" >> $f
  echo "$(date -u +%H:%M:%S) done $name r$i" >> $OUT/progress.log
}
for i in $(seq $FIRST $LAST); do
  step tc4-foldonly env -C $T BENCH_FOLD_ONLY=1 node conformance/bench-fold.mjs --bible
  step rb-tc4shape-t1 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-tc4shape 1
  step tc4-open node $S/bench/tc4-open.mjs $C/tc4-bible
  step rb-tc4shape-t4 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-tc4shape 4
  step tc4-open-history node $S/bench/tc4-open.mjs $C/tc4-history
  step rb-bible-t1 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-bible 1
  step rb-bible-t4 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-bible 4
done
echo "$(date -u +%H:%M:%S) all rounds done" >> $OUT/progress.log
