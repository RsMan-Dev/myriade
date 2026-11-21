#!/usr/bin/env bash
# The passes of the integration suite (`tests/it`), by environment variables only (no path of a machine).
#   pass.sh -t PATTERN [ID ...]   TARGETED (the normal mode after a fix, seconds): the tests whose name contains PATTERN (`-x`: exactly), for these
#                                 identifiers (no ID: the identifier of the process default, cow), in parallel. Prints the reds and the time.
#   pass.sh --common              the common group alone, once (tests that do not read the identifier).
#   pass.sh                       FULL (the end of a merge): the common group ONCE, then the other tests for each identifier (IDS or the whole
#                                 table of species/mod.rs), then the summary green / red / ignored and the completeness check.
#   pass.sh --classify            rebuilds tests/common.list: one process per test with HORIZON_SPECIES_PROBE; the tests that never read
#                                 HORIZON_SPECIES are the common group (to redo when tests are added: a test absent from the list is "dependent", so
#                                 it stays correct and only costs time; a test that STOPS being common turns red in the common run, see below).
# Environment: HORIZON_TEST_RESOURCES (required). BIN = the test binary `it`; else it is built by ./warm-cargo.sh (CARGO_TARGET_DIR required, a warm target only; --profile $PROFILE, default ecsdev,
#   extra arguments in CARGO_ARGS). IDS = "minecraft:cow zombie ..." (with or without the prefix). JOBS = processes at once (8). TT = test threads
#   of a process (4; the common run uses TT_COMMON, 8). OUT = the output directory (a temporary one). HORIZON_QUICK defaults to 1 (FULL=1: whole counts).
# Why the common group is safe: it runs with HORIZON_SPECIES_FORBID=1, where any read of the identifier panics: a test that reads it is red, not
#   run unseen. Completeness: common.list + dependents = the whole list of the binary (`--list`), and the number of results of every run is checked.
set -u
cd "$(dirname "$0")"
: "${HORIZON_TEST_RESOURCES:?}"
export HORIZON_TEST_RESOURCES NO_XP="${NO_XP:-1}"
if [ -n "${FULL:-}" ]; then unset HORIZON_QUICK; else export HORIZON_QUICK="${HORIZON_QUICK:-1}"; fi
JOBS=${JOBS:-8}; TT=${TT:-4}; TT_COMMON=${TT_COMMON:-8}
OUT=${OUT:-$(mktemp -d)}; mkdir -p "$OUT"
LIST=tests/common.list
now() { date +%s.%N; }
since() { awk -v a="$1" -v b="$(now)" 'BEGIN{printf "%.1f", b-a}'; }

if [ -z "${BIN:-}" ]; then
  # shellcheck disable=SC2086
  BIN=$(./warm-cargo.sh test -p horizon-ecs-proto --profile "${PROFILE:-ecsdev}" ${CARGO_ARGS:-} --no-run --message-format=json 2> "$OUT/build.err" | python3 -c '
import sys,json
for l in sys.stdin:
    try: m=json.loads(l)
    except Exception: continue
    if m.get("reason")=="compiler-artifact" and m.get("executable") and m.get("target",{}).get("name")=="it": print(m["executable"])' | tail -n 1)
  [ -n "$BIN" ] || { echo "build failed or refused (warm-cargo.sh codes 97 cold target, 98 over 300 s):"; grep -E '^error' -A6 "$OUT/build.err" | head -n 30; exit 1; }
fi
BIN=$(readlink -f "$BIN")
ids() {
  if [ -n "${IDS:-}" ]; then printf '%s\n' $IDS; else grep -o '("minecraft:[a-z_]*", &\[' src/species/mod.rs | sed 's/("minecraft://;s/", &\[//' | sort -u | grep -v '^nope$'; fi | sed 's/^minecraft://'
}
all_tests() { "$BIN" --list 2>/dev/null | sed -n 's/: test$//p' | sort; }
ignored_tests() { "$BIN" --list --ignored 2>/dev/null | sed -n 's/: test$//p' | sort; }
runnable() { comm -23 <(all_tests) <(ignored_tests); }
# run EXACT_NAMES_FILE ID TT LOG : the listed tests (exact) under ID ("-" = no identifier, the common run)
run_names() {
  mapfile -t names < "$1"
  if [ "$2" = - ]; then HORIZON_SPECIES_FORBID=1 "$BIN" --exact "${names[@]}" --test-threads="$3" > "$4" 2>&1
  else HORIZON_SPECIES="minecraft:$2" "$BIN" --exact "${names[@]}" --test-threads="$3" > "$4" 2>&1; fi
}
results() { grep -c "^test .* \.\.\. $2\$" "$1"; }   # count of one verdict in a log
reds() { sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$1"; }

case "${1:-}" in
-t|-x)
  exact=""; [ "$1" = -x ] && exact="--exact"; pat=${2:?pattern}; shift 2
  if [ $# -gt 0 ]; then idl=("$@"); else idl=(cow); fi
  t0=$(now)
  for i in "${idl[@]}"; do i=${i#minecraft:}; echo "$i"; done | xargs -P "$JOBS" -I{} bash -c '
    HORIZON_SPECIES=minecraft:{} "$0" "$1" $2 --test-threads="$3" > "$4/t-{}.log" 2>&1; echo $? > "$4/t-{}.rc"' "$BIN" "$pat" "$exact" "$TT_COMMON" "$OUT"
  rc=0
  for i in "${idl[@]}"; do i=${i#minecraft:}
    p=$(results "$OUT/t-$i.log" ok); f=$(results "$OUT/t-$i.log" FAILED)
    echo "$i: $p passed, $f failed, $(grep -c 'ignored' "$OUT/t-$i.log" | head -1) ignored lines"
    reds "$OUT/t-$i.log" | sed "s/^/  RED $i /"
    [ "$(cat "$OUT/t-$i.rc")" = 0 ] || rc=1
    grep -q '^running 0 tests' "$OUT/t-$i.log" && { echo "  no test matches '$pat'"; rc=1; }
  done
  echo "targeted: $(since "$t0") s"; exit $rc ;;
--classify)
  : > "$OUT/probe.tsv"; runnable > "$OUT/todo"
  c() { t=$1; f=$(mktemp); HORIZON_SPECIES_PROBE=$f HORIZON_SPECIES=minecraft:cow "$BIN" --exact "$t" --test-threads=1 > /dev/null 2>&1; rc=$?
        printf '%s\t%s\t%s\n' "$t" "$([ -s "$f" ] && echo 1 || echo 0)" "$rc"; rm -f "$f"; }
  export -f c; export BIN
  xargs -a "$OUT/todo" -P "$JOBS" -I{} bash -c 'c {}' > "$OUT/probe.tsv"
  [ "$(wc -l < "$OUT/probe.tsv")" = "$(wc -l < "$OUT/todo")" ] || { echo "classification incomplete"; exit 1; }
  awk -F'\t' '$3!=0{print "red in the classification: "$1}' "$OUT/probe.tsv"
  awk -F'\t' '$2==0{print $1}' "$OUT/probe.tsv" | sort > "$LIST"
  echo "common: $(wc -l < "$LIST") of $(wc -l < "$OUT/todo") ($LIST)"; exit 0 ;;
esac

# ---- common group and completeness
t_all=$(now)
runnable > "$OUT/all"; ignored_tests > "$OUT/ignored"
[ -s "$LIST" ] || { echo "$LIST missing: pass.sh --classify"; exit 2; }
sort -u "$LIST" > "$OUT/common"
stale=$(comm -13 "$OUT/all" "$OUT/common"); [ -z "$stale" ] || { echo "common.list names tests that no longer exist (pass.sh --classify):"; echo "$stale"; exit 2; }
comm -23 "$OUT/all" "$OUT/common" > "$OUT/dep"
nc=$(wc -l < "$OUT/common"); nd=$(wc -l < "$OUT/dep"); na=$(wc -l < "$OUT/all"); ni=$(wc -l < "$OUT/ignored")
[ $((nc + nd)) -eq "$na" ] || { echo "completeness: common $nc + dependent $nd != $na"; exit 2; }
echo "tests: $na run (common $nc, dependent $nd), $ni ignored"
t0=$(now); run_names "$OUT/common" - "$TT_COMMON" "$OUT/common.log"; tc=$(since "$t0")
[ "$(( $(results "$OUT/common.log" ok) + $(results "$OUT/common.log" FAILED) ))" -eq "$nc" ] || echo "completeness: the common run gave a wrong number of results"
if [ "${1:-}" = --common ]; then
  echo "common: $(results "$OUT/common.log" ok) green, $(results "$OUT/common.log" FAILED) red, $tc s"; reds "$OUT/common.log" | sed 's/^/  RED /'; exit 0
fi
# ---- the dependent tests, for every identifier
ids > "$OUT/ids"
t0=$(now)
xargs -a "$OUT/ids" -P "$JOBS" -I{} bash -c '
  s=$(date +%s); HORIZON_SPECIES=minecraft:{} "$0" --exact "${@:3}" --test-threads="$1" > "$2/id-{}.log" 2>&1; echo "{} $(( $(date +%s) - s ))" >> "$2/id-times"' "$BIN" "$TT" "$OUT" $(tr '\n' ' ' < "$OUT/dep")
td=$(since "$t0")
# ---- summary
nid=$(wc -l < "$OUT/ids"); green=$(results "$OUT/common.log" ok); red=$(results "$OUT/common.log" FAILED); short=""
reds "$OUT/common.log" | sed 's/^/common /' > "$OUT/reds"
while read -r i; do
  p=$(results "$OUT/id-$i.log" ok); f=$(results "$OUT/id-$i.log" FAILED)
  [ $((p + f)) -eq "$nd" ] || short="$short $i($((p + f))/$nd)"
  green=$((green + p)); red=$((red + f)); reds "$OUT/id-$i.log" | sed "s/^/$i /" >> "$OUT/reds"
done < "$OUT/ids"
{ echo "identifiers: $nid ; common $nc once ($tc s) ; dependent $nd x $nid ($td s) ; total $(since "$t_all") s"
  echo "executions: $((nc + nd * nid)) ; distinct tests $((nc + nd)) + $ni ignored ; green $green ; red $red ; ignored $ni"
  [ -z "$short" ] && echo "completeness: ok" || echo "completeness: INCOMPLETE runs:$short"
  echo "reds (identifier or common, test):"; sort -u "$OUT/reds" | sed 's/^/  /'; } | tee "$OUT/summary.txt"
[ "$red" -eq 0 ] && [ -z "$short" ]
