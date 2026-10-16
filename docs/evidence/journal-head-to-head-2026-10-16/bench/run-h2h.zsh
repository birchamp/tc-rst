#!/bin/zsh
# Head-to-head journal benchmark runner: tC4 vs the Rust rebuild.
# Phase A = matched scale (~278k events/changes), interleaved. Phase B = rebuild native 617k.
set -u
SP=/private/tmp/claude-502/-Users-agenthermes-Development-tc-rs/e7c4b2e1-cc9d-488b-8ef7-175c069f251c/scratchpad
TC4=/Users/agenthermes/Development/translationCore4
RS=/Users/agenthermes/Development/tc-rs
FB=$RS/target/release/examples/foldbench
OUT=$SP/runs; BT=$SP/benchtmp
mkdir -p "$OUT" "$BT"; touch "$BT/.metadata_never_index"

stamp(){ date -u +%H:%M:%SZ }
l1(){ sysctl -n vm.loadavg | tr -d '{}' | awk '{print $1}' }
# CPU_Speed_Limit < 100 means the chassis is thermally throttling: the run is not comparable.
spd(){ pmset -g therm 2>/dev/null | awk -F'= *' '/CPU_Speed_Limit/{print $2}' | tr -d ' ' }
hdr(){ print -r -- "side=$1 phase=$2 run=$3 start=$(stamp) load1=$(l1) speed_limit=$(spd)" }
ftr(){ print -r -- "end=$(stamp) load1=$(l1) speed_limit=$(spd)" }

PHASE=${1:-A}
RUNS=${2:-5}

if [[ $PHASE == A ]]; then
  for i in $(seq 1 $RUNS); do
    f=$OUT/a-tc4-$i.log
    hdr tc4 A $i > $f
    ( cd $TC4 && BENCH_TMPDIR=$BT node conformance/bench-fold.mjs --bible ) >> $f 2>&1
    ftr >> $f
    print "done tc4 run $i at $(stamp)"

    f=$OUT/a-rs-$i.log
    hdr rebuild A $i > $f
    ( cd $RS && FOLDBENCH_RUNS=1 $FB run $SP/corpus-278k 4 ) >> $f 2>&1
    ftr >> $f
    print "done rebuild run $i at $(stamp)"
  done
fi

if [[ $PHASE == B ]]; then
  for i in $(seq 1 $RUNS); do
    f=$OUT/b-rs-bible-$i.log
    hdr rebuild B $i > $f
    ( cd $RS && FOLDBENCH_RUNS=1 $FB run $SP/corpus-bible 4 ) >> $f 2>&1
    ftr >> $f
    print "done rebuild-bible run $i at $(stamp)"
  done
fi
