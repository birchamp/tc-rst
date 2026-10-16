#!/bin/bash
S=/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/journal-decision
T=/home/user/unfoldingword/translationcore4
R=$S/bench-target/release/examples/foldbench
C=$S/corpora
OUT=$S/results/extras2
mkdir -p $OUT $S/bench/tmp
TIMEFORMAT='process: real %R s, user %U s, sys %S s'
step() {
  local name=$1; shift
  local f=$OUT/r$i-$name.txt
  { echo "### $name round $i"; echo "start $(date -u +%H:%M:%S) $(cat /proc/loadavg)"; } > $f
  { time "$@" ; } >> $f 2>&1
  echo "end $(date -u +%H:%M:%S) $(cat /proc/loadavg)" >> $f
  echo "$(date -u +%H:%M:%S) done $name r$i" >> $OUT/progress.log
}
for i in 1 2 3; do
  step tc4-zipintake-sync env SYNC=1 node $S/bench/tc4-zipintake-variant.mjs $C/tc4-journal.zip
  step rb-tc4shape-t1 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-tc4shape 1
done
i=1
# tC4's own benchmark, unmodified and complete (fold, C1-C3 controls, open() curve), once.
step tc4-bench-full env -C $T BENCH_TMPDIR=$S/bench/tmp node conformance/bench-fold.mjs --bible
echo "$(date -u +%H:%M:%S) all extras2 done" >> $OUT/progress.log
