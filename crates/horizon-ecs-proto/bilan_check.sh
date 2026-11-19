#!/usr/bin/env bash
# Regression gate of the ECS prototype (deterministic: exact instruction counts, so any difference is real). BUILD MODE MATTERS: build
# the benches with CARGO_INCREMENTAL=0. `BILAN_LOG=file` compares a saved bilan.sh output instead of running it.
# Two rules, per line (only `ecs`, `ecs-eager`, `batch`, `ordered` columns; the `mob` column is a control):
#  (a) the STEP of a merge (against `bilan.last`, the figures of the previous merge): accepted without cause if under 1 % or under
#      10 instructions; above, a named cause is needed BEFORE the merge: a waiver in `bilan.waivers` ("line ## percent ## reason").
#  (b) the CUMULATIVE drift against `bilan.ref` (the reference 2dbc7394eb): at 3 % a search for the cause is opened (printed SEARCH, the
#      lots go on); at 5 % nothing enters on that line (FAIL).
# ./bilan_check.sh                     checks (exit 1 on a FAIL)
# ./bilan_check.sh --record "commit"   after a merge: appends the steps to `bilan.drift` (commit, line, before, after) and the figures
#                                      become `bilan.last`
set -u
cd "$(dirname "$0")"
now=$(mktemp)
{ if [ -n "${BILAN_LOG:-}" ]; then cat "$BILAN_LOG"; else ./bilan.sh 2>/dev/null; fi; } | awk '{
  label=$0; sub(/:.*/,"",label); if (label ~ /^(zombie chase|crowd)/) { l=$0; sub(/\).*/,")",l); label=l }
  n=split($0,a," ");
  for(i=1;i<=n;i++) if (a[i]=="ecs"||a[i]=="ecs-eager"||a[i]=="batch"||a[i]=="ordered") print label "|" a[i], a[i+1]
}' > "$now"
if [ "${1:-}" = "--record" ]; then
  [ -n "${2:-}" ] || { echo "a commit label is required: --record \"commit\""; exit 2; }
  [ -f bilan.last ] || cp bilan.ref bilan.last
  while IFS= read -r line; do
    key=${line% *}; new=${line##* }
    old=$(grep -F -- "$key " bilan.last | head -n1 | awk '{print $NF}')
    [ -n "$old" ] && [ "$old" != "$new" ] && printf '%s|%s|%s|%s\n' "$2" "$key" "$old" "$new" >> bilan.drift
  done < "$now"
  cp "$now" bilan.last; echo "recorded $2"; rm -f "$now"; exit 0
fi
[ -f bilan.ref ] || { echo "no bilan.ref"; exit 2; }
[ -f bilan.last ] || cp bilan.ref bilan.last
fail=0
while IFS= read -r line; do
  key=${line% *}; new=${line##* }
  ref=$(grep -F -- "$key " bilan.ref | head -n1 | awk '{print $NF}')
  last=$(grep -F -- "$key " bilan.last | head -n1 | awk '{print $NF}')
  [ -z "$ref" ] && { echo "NEW   $key $new (no reference)"; continue; }
  [ -z "$last" ] && last=$ref
  waive=$(awk -F' ## ' -v k="$key" '$1==k{print $2}' bilan.waivers 2>/dev/null | head -n1)
  verdict=$(awk -v n="$new" -v r="$ref" -v l="$last" -v w="${waive:-1}" 'BEGIN{
    c=(n-r)*100/(r>0?r:1); s=(n-l)*100/(l>0?l:1); v="ok";
    if (s>w && n-l>=10) v="FAIL-step"; if (c>=3 && v=="ok") v="SEARCH"; if (c>=5 && n-r>=10) v="FAIL-cumul";
    printf "%s step %.2f cumul %.2f", v, s, c }')
  echo "${verdict%% *} $key ref $ref last $last now $new (${verdict#* })"
  case $verdict in FAIL*) fail=1;; esac
done < "$now"
if [ -z "${BILAN_LOG:-}" ]; then
  sc=$(mktemp); ./species_cost.sh > "$sc" 2>&1 || fail=1
  grep -v '^ok [a-z_]* [0-9]*$' "$sc" | sed 's/^/species cost: /'; rm -f "$sc"
fi
rm -f "$now"; exit $fail
