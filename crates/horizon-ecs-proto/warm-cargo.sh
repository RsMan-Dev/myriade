#!/usr/bin/env bash
# warm-cargo.sh <cargo arguments> : cargo on a warm target only, under a time limit.
# A cold build compiles the whole workspace for nothing; it has to be justified, never a default.
#   CARGO_TARGET_DIR    required; the warm target shared by the agents taking turns
#   WARM_PROFILE_DIR    profile directory checked for warmth (default ecsdev)
#   BUILD_TIMEOUT       seconds before the build is killed (default 300)
#   COLD_BUILD_REASON   the written reason that allows a cold build (limit: COLD_BUILD_TIMEOUT, default 3600)
set -u
: "${CARGO_TARGET_DIR:?warm-cargo: CARGO_TARGET_DIR must name the warm target}"
dir="$CARGO_TARGET_DIR/${WARM_PROFILE_DIR:-ecsdev}/deps"
limit=${BUILD_TIMEOUT:-300}
rule="A cold build must be absolutely justified: use the warm target and its worktree in turn, or set COLD_BUILD_REASON='<why no warm target can serve>' and say so in the report."

if ! ls "$dir"/libmyriade_level-*.rlib >/dev/null 2>&1; then
  if [ -z "${COLD_BUILD_REASON:-}" ]; then
    echo "warm-cargo: REFUSED, '$dir' is cold. $rule" >&2
    exit 97
  fi
  echo "warm-cargo: cold build allowed, reason: $COLD_BUILD_REASON" >&2
  limit=${COLD_BUILD_TIMEOUT:-3600}
fi

timeout --signal=TERM --kill-after=10 "$limit" cargo "$@"
rc=$?
if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
  echo "warm-cargo: KILLED after ${limit}s. A build this long is a cold or near-cold build (new target, another worktree's" >&2
  echo "paths or timestamps, a profile never built here). Do not retry with a longer limit before finding which. $rule" >&2
  exit 98
fi
exit "$rc"
