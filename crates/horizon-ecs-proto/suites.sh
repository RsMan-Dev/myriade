#!/usr/bin/env bash
# The suites of EVERY species in one go, in parallel (one process per test binary and per species, as many at once as there are cores).
#   ./suites.sh                    the reference species (cow, zombie, skeleton) and those whose file species/<name>.rs changed since the last commit
#   ./suites.sh --all              every species of the registry table (species/mod.rs): for the merges (always with HORIZON_QUICK: a quarter of the repeated trials; FULL=1 for the whole count)
#   ./suites.sh name ...           these species
# HORIZON_TEST_RESOURCES set; the tests are built first (`cargo test --no-run`, incremental, in the target dir TGT), then every test binary is
# run for every species with HORIZON_SPECIES; the report names the species and the tests that failed. Exit 1 if any failed.
set -u
cd "$(dirname "$0")"
: "${HORIZON_TEST_RESOURCES:?}"
JOBS=${JOBS:-$(nproc)}
out=$(mktemp -d)
species=()
all() { grep -o '("minecraft:[a-z_]*", &\[' src/species/mod.rs | sed 's/("minecraft://;s/", &\[//' | sort -u | grep -v '^nope$'; }
case "${1:-}" in
  --all) mapfile -t species < <(all) ;;
  "" | --changed)
    # The default: the three reference species (the framework), and the species whose own file changed since the last commit; a change of the
    # table, a template or the framework (anything outside species/<name>.rs) is not a change of a species: `--all` is for the merges.
    species=(cow zombie skeleton)
    for f in $(git status --porcelain -- src/species | awk '{print $NF}'); do n=$(basename "$f" .rs); [ -n "$(all | grep -x "$n")" ] && species+=("$n"); done
    mapfile -t species < <(printf '%s\n' "${species[@]}" | sort -u) ;;
  *) species=("$@") ;;
esac
# 0. every name must be a registered species (the process panics on an unknown HORIZON_SPECIES, but say it before building), and for --all the
# species launched must be the species of BUILTIN: a mismatch means species run unnamed or not at all.
for s in "${species[@]}"; do all | grep -qx "$s" || { echo "unknown species '$s' (not in the table of species/mod.rs; give it without the minecraft: prefix)"; exit 2; }; done
if [ "${1:-}" = --all ]; then
  nb=$(awk '/^pub static BUILTIN/{f=1} f{n+=gsub(/&[a-z_0-9]+::DEF/,"&")} f&&/\];/{print n; exit}' src/species/mod.rs)
  [ "$nb" = "${#species[@]}" ] || { echo "--all launches ${#species[@]} species but BUILTIN has $nb"; exit 2; }
  echo "species launched: ${#species[@]} = BUILTIN"
fi
# 1. build once, list the test executables
${BUILD:-cargo} test -p horizon-ecs-proto --profile fast --no-run --message-format=json 2> "$out/build.err" | python3 -c '
import sys,json
for l in sys.stdin:
    try: m=json.loads(l)
    except Exception: continue
    if m.get("reason")=="compiler-artifact" and m.get("executable") and m.get("profile",{}).get("test"):
        print(m["executable"])' > "$out/exes"
[ -s "$out/exes" ] || { echo "build failed:"; grep -E "^error" -A6 "$out/build.err" | head -n 30; exit 1; }
# 2. every (binary, species) pair, in parallel
while read -r exe; do for s in "${species[@]}"; do printf '%s\t%s\n' "$exe" "$s"; done; done < "$out/exes" > "$out/jobs"
run() {
  exe=$1; s=$2; out=$3
  f="$out/$(basename "$exe")-$s.log"
  q=1; [ -n "${FULL:-}" ] && q=""
  if [ -n "$q" ]; then export HORIZON_QUICK=1; else unset HORIZON_QUICK; fi
  tt=4; [ "$s" = cow ] && tt=16   # the cow runs the laws of its own configuration (once for everybody): its heavy tests side by side
  t0=$(date +%s%N)
  HORIZON_SPECIES=minecraft:$s "$exe" --test-threads=$tt > "$f" 2>&1 || echo "$s $(basename "$exe")" >> "$out/failed"
  echo "$(( ($(date +%s%N) - t0) / 1000000 )) $(basename "$exe") $s" >> "$out/times"
}
export -f run; export out HORIZON_TEST_RESOURCES
tr '\t' '\n' < "$out/jobs" | xargs -d '\n' -n 2 -P "$JOBS" bash -c 'run "$0" "$1" "$out"'
# 3. the report
if [ -s "$out/failed" ]; then
  echo "FAILED (species, test binary):"
  sort -u "$out/failed" | head -n 60
  for l in $(sort -u "$out/failed" | tr ' ' ':' | head -n 12); do s=${l%%:*}; b=${l##*:}; grep -h "FAILED$" "$out/$b-$s.log" 2>/dev/null | sed "s/^/  $s: /" | head -n 4; done
  echo "logs in $out"
  exit 1
fi
echo "slowest runs (ms, binary, species):"; sort -rn "$out/times" | head -n 6 | sed "s/^/  /"
echo "all ${#species[@]} species green ($(wc -l < "$out/exes") test binaries each)"; rm -rf "$out"
