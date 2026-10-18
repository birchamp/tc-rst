#!/bin/bash
# tC4's own benchmark, unmodified and complete (fold, C1-C3, the open() curve to 278,041
# segments), once, its temporary corpus under the scratch directory.
S=/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/journal-decision
T=/home/user/unfoldingword/translationcore4
OUT=$S/results/tc4full
mkdir -p $OUT $S/bench/tmp
TIMEFORMAT='process: real %R s, user %U s, sys %S s'
f=$OUT/tc4-bench-full.txt
# The shared disk is nearly full: the bench writes ~1.1 GiB of 4 KiB-block segment files.
# Free the history corpus first (regenerable with bench/tc4-save-history.mjs) and skip the
# run if there is still not enough room.
avail_kb() { df --output=avail -k / | tail -1; }
if [ "$(avail_kb)" -lt 4000000 ]; then rm -rf $S/corpora/tc4-history; echo "$(date -u +%H:%M:%S) removed corpora/tc4-history for space" >> $OUT/load.log; fi
if [ "$(avail_kb)" -lt 2500000 ]; then echo "skipped: only $(avail_kb) KiB free on /" > $f; exit 0; fi
{ echo "### tc4-bench-full"; echo "start $(date -u +%H:%M:%S) $(cat /proc/loadavg)"; } > $f
( while true; do echo "== $(date -u +%H:%M:%S) $(cat /proc/loadavg)"; sleep 30; done ) >> $OUT/load.log 2>&1 &
SAMPLER=$!
{ time env -C $T BENCH_TMPDIR=$S/bench/tmp node conformance/bench-fold.mjs --bible ; } >> $f 2>&1
echo "exit $?" >> $f
kill $SAMPLER
echo "end $(date -u +%H:%M:%S) $(cat /proc/loadavg)" >> $f
