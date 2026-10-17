#!/bin/bash
# One-shot runs with the corpora not yet in the page cache (the VM restarted at 15:51 UTC).
# n=1 per side: indicative only. Order alternates tC4, rebuild.
S=/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/journal-decision
R=$S/bench-target/release/examples/foldbench
C=$S/corpora
OUT=$S/results/cold
mkdir -p $OUT
TIMEFORMAT='process: real %R s, user %U s, sys %S s'
step() {
  local name=$1; shift
  local f=$OUT/$name.txt
  { echo "### $name cold"; echo "start $(date -u +%H:%M:%S) $(cat /proc/loadavg)"; grep -E '^(Cached|Buffers):' /proc/meminfo | tr '\n' ' '; echo; } > $f
  { time "$@" ; } >> $f 2>&1
  echo "end $(date -u +%H:%M:%S) $(cat /proc/loadavg)" >> $f
  echo "$(date -u +%H:%M:%S) done $name" >> $OUT/progress.log
}
step tc4-open node $S/bench/tc4-open.mjs $C/tc4-bible
step rb-tc4shape-t4 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-tc4shape 4
step tc4-zipintake node $S/bench/tc4-zipintake.mjs $C/tc4-journal.zip
step rb-bible-t4 env FOLDBENCH_FOLD_ONLY=1 FOLDBENCH_RUNS=1 $R run $C/rb-bible 4
step tc4-open-history node $S/bench/tc4-open.mjs $C/tc4-history
echo "$(date -u +%H:%M:%S) all cold done" >> $OUT/progress.log
