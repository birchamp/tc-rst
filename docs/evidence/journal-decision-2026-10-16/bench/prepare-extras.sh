#!/bin/bash
S=/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/journal-decision
C=$S/corpora
TIMEFORMAT='process: real %R s, user %U s, sys %S s'
mkdir -p $C/tc4-history
{ time node $S/bench/tc4-save-history.mjs --bible $C/tc4-history ; } > $S/results/tc4-save-history.log 2>&1
echo EXIT $? >> $S/results/tc4-save-history.log
cd $C/tc4-bible/journal && { time zip -r -q -X $C/tc4-journal.zip bench-a ; } > $S/results/tc4-zip.log 2>&1
echo EXIT $? >> $S/results/tc4-zip.log; ls -l $C/tc4-journal.zip >> $S/results/tc4-zip.log
