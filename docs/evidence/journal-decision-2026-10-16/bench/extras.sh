#!/bin/bash
# Second set: stage breakdowns, tC4's zip intake path, and the history corpus, interleaved.
S=/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/journal-decision
R=$S/bench-target/release/examples/foldbench
H=$S/bench-target/release/examples/h2h
C=$S/corpora
OUT=$S/results/extras
mkdir -p $OUT
FIRST=${1:-1}; LAST=${2:-5}
TIMEFORMAT='process: real %R s, user %U s, sys %S s'
step() {
  local name=$1; shift
  local f=$OUT/r$i-$name.txt
  { echo "### $name round $i"; echo "start $(date -u +%H:%M:%S) $(cat /proc/loadavg)"; } > $f
  { time "$@" ; } >> $f 2>&1
  echo "end $(date -u +%H:%M:%S) $(cat /proc/loadavg)" >> $f
  echo "$(date -u +%H:%M:%S) done $name r$i" >> $OUT/progress.log
}
for i in $(seq $FIRST $LAST); do
  step tc4-scanprof node $S/bench/tc4-scanprof.mjs $C/tc4-bible
  step rb-loadprof-tc4shape $H loadprof $C/rb-tc4shape
  step node-hashparse-tc4shape node $S/bench/node-hashparse.mjs $C/rb-tc4shape
  step tc4-zipintake node $S/bench/tc4-zipintake.mjs $C/tc4-journal.zip
  step rb-tc4shape-t4 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-tc4shape 4
  step tc4-open-history node $S/bench/tc4-open.mjs $C/tc4-history
  step rb-bible-t1 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-bible 1
  step rb-loadprof-bible $H loadprof $C/rb-bible
done
echo "$(date -u +%H:%M:%S) all extras done" >> $OUT/progress.log
