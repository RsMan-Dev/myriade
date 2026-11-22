#!/usr/bin/env bash
# Plays the vanilla behaviour runs in the real 26.3 server; writes $SP/vanilla-behaviour.tsv. Env: SP (scratch dir), PROBE_SPECIES (optional, comma list).
set -eu
SP=${SP:-/tmp/claude-1000/-home-rsman-projects-horizon-server-tests/5e382023-90b1-4b61-9ef0-0af6d768e9f1/scratchpad}
JDK=/nix/store/59hn3lxp64cgh3498l98bja99jcgn1q6-openjdk-25.0.5.0.3+1
RT=/home/rsman/projects/horizon-server-tests/runtime/vanilla-26.3
SRC=$(cd "$(dirname "$0")" && pwd)
RUN=$SP/vanilla-probe-run
mkdir -p "$RUN/classes"
CP=$RT/server-26.3.jar:$(find "$RT/client/libraries" -name '*.jar' | grep -v lwjgl | tr '\n' ':')
unset LD_LIBRARY_PATH
"$JDK/bin/javac" -d "$RUN/classes" -cp "$CP" "$SRC"/*.java
cd "$RUN"
echo "eula=true" > eula.txt
printf '%s\n' level-type=minecraft:flat generate-structures=false online-mode=false server-ip=127.0.0.1 server-port=25581 difficulty=normal \
  spawn-monsters=false spawn-animals=false spawn-npcs=false max-tick-time=-1 view-distance=3 simulation-distance=3 pause-when-empty-seconds=-1 \
  enable-status=false enable-query=false enable-rcon=false spawn-protection=0 sync-chunk-writes=false > server.properties
rm -rf world
timeout 900 "$JDK/bin/java" -Xmx4g --add-opens java.base/java.lang=ALL-UNNAMED -Dprobe.out="$SP/vanilla-behaviour.tsv" \
  ${PROBE_SPECIES:+-Dprobe.species=$PROBE_SPECIES} -cp "classes:$CP" Probe nogui < /dev/null > server.log 2>&1
rm -rf world
