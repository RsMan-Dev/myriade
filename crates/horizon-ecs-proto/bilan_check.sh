#!/usr/bin/env bash
# Regression gate of the ECS prototype. BUILD MODE MATTERS: build the benches with CARGO_INCREMENTAL=0 (an incremental build adds 5 to 8 %
# to every line, the Mob's included); `BILAN_LOG=file` compares a saved bilan.sh output instead of running it.
# runs bilan.sh (HORIZON_TEST_RESOURCES and D set, the benches built with
# `cargo build -p horizon-ecs-proto --profile fast --bins`) and compares every ECS figure with `bilan.ref`.
#   ./bilan_check.sh                  fails (exit 1) if a line is more than THRESHOLD % (default 1) above its reference
#   ./bilan_check.sh --update "why"   writes the figures as the new reference and records the reason in bilan.ref.log
# A line that buys fidelity to the Mob is excused in `bilan.waivers` ("label|column ## percent ## reason"): the waiver is
# written and committed WITH the change, and the reference is updated with a reason. Only `ecs`, `ecs-eager`, `batch`,
# `ordered` columns are compared (the `Mob` column is a control, not a budget).
set -u
cd "$(dirname "$0")"
THRESHOLD=${THRESHOLD:-1}
now=$(mktemp)
{ if [ -n "${BILAN_LOG:-}" ]; then cat "$BILAN_LOG"; else ./bilan.sh 2>/dev/null; fi; } | awk '{
  label=$0; sub(/:.*/,"",label); if (label ~ /^(zombie chase|crowd)/) { l=$0; sub(/\).*/,")",l); label=l }
  n=split($0,a," ");
  for(i=1;i<=n;i++) if (a[i]=="ecs"||a[i]=="ecs-eager"||a[i]=="batch"||a[i]=="ordered") print label "|" a[i], a[i+1]
}' > "$now"
if [ "${1:-}" = "--update" ]; then
  [ -n "${2:-}" ] || { echo "a reason is required: --update \"why\""; exit 2; }
  cp "$now" bilan.ref; printf '%s %s\n' "$(git rev-parse --short HEAD 2>/dev/null)" "$2" >> bilan.ref.log; echo "reference updated: $2"; rm -f "$now"; exit 0
fi
[ -f bilan.ref ] || { echo "no bilan.ref: run --update first"; exit 2; }
fail=0
while IFS= read -r line; do
  key=${line% *}; new=${line##* }
  ref=$(grep -F -- "$key " bilan.ref | head -n1 | awk '{print $NF}')
  [ -z "$ref" ] && { echo "NEW   $key $new (no reference)"; continue; }
  waive=$(awk -F' ## ' -v k="$key" '$1==k{print $2}' bilan.waivers 2>/dev/null | head -n1)
  limit=${waive:-$THRESHOLD}
  verdict=$(awk -v n="$new" -v r="$ref" -v t="$limit" 'BEGIN{ d=(n-r)*100/(r>0?r:1); printf "%s %.2f", (d>t && n-r>5)?"FAIL":"ok", d }')
  echo "${verdict%% *} $key ref $ref now $new (${verdict##* } %, limit $limit %)"
  case $verdict in FAIL*) fail=1;; esac
done < "$now"
if [ -z "${BILAN_LOG:-}" ]; then
  sc=$(mktemp); ./species_cost.sh > "$sc" 2>&1 || fail=1
  grep -v '^ok [a-z_]* [0-9]*$' "$sc" | sed 's/^/species cost: /'; rm -f "$sc"
fi
rm -f "$now"; exit $fail
