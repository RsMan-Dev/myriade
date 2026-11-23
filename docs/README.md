# Horizon documentation

The project site is [playhorizon.dev](https://playhorizon.dev). These pages are for developers. What is being done now and next, the work board and how to help: [the hub](https://playhorizon.dev/hub) ([ROADMAP.md](../ROADMAP.md), [roadmap/README.md](../roadmap/README.md) for keeping it current).

## Build and run

- [AGENTS.md](../AGENTS.md) and [Architecture](architecture.md): where each part of the code lives, how `Game` is organised, which harness checks which area.
- [How Horizon is built](how-horizon-is-built.md): who writes Horizon, how changes are checked, and what is safe to run.
- [Release flow](release-flow.md): pull request checks, merging and the automatic Nightly releases that reach players.
- [Operations](operations.md): running Horizon by hand when no agent runs and the maintainer's machines are off: merging, releases, offline runners, the mods registry, Kira, the official server, security.
- [Build times](build-times.md): `tools/build_times.py` measures cold and incremental builds per crate and the critical path, writes a JSON report and a table, and compares two reports; the first numbers, Windows and Linux.
- [Host a server](hosting.md): install `horizon-server`, prepare the game files, first run, `server.toml`, the commands, the web panel, backups, services and Docker; what works today and what doesn't yet.
- [A server's levels](server-levels.md): how the dedicated server ticks, saves and travels between dimensions.
- [Velocity forwarding](velocity-forwarding.md): the player-information exchange behind a Velocity proxy and how it was checked.
- [The anticheat](anticheat.md): the dedicated server's own checks, on by default (`combat` and `ground` act, `timer` and `prediction` only log) and set in `server.toml`'s `[anticheat]`: violation levels, setback, alerts, kicks, the log, and how to write a check.
- [The chat filter](chat-filter.md): the dedicated server refuses hate slurs in chat, message commands, signs, anvil names and book titles: the list and its normalising, mutes, temporary bans, `[chat_filter]`.
- [Nix and NixOS](nix.md): the flake's packages: server, engine and prepare, and the `services.horizon-server` NixOS module.
- [Hosting a server: design](server.md): `horizon-server`, the design and the plan: install, `server.toml`, the CLI, the web panel, backups, services, Docker, and the bot benchmark against vanilla.
- [The next server: design](server-next.md): Horizon's dedicated server against vanilla and Pumpkin (measured), the targets, parallel ticking that keeps vanilla's order, the network front, the Horizon fast path, the plugin API and the plan.
- [Survival multiplayer](smp.md): the official server's `smp` native mod (economy, market, trades, games of chance, spawn, homes, teleport requests, warps, land claims) and the server hook points it needs; not part of Horizon's own server.
- [For server owners](owners.md): every `server.properties` key next to its `server.toml` field.
- [Memory](memory.md): what holds the game's memory (the control socket's `memory` command, the `alloc-census` heap census and its measured cost, resident memory in benchmark reports, heap profilers on probes).
- [Server against Pumpkin](performance/server-vs-pumpkin.md): Horizon's server and Pumpkin (Rust) under the same bots at 0, 20, 60 and 100 players: tick time, CPU, memory, chunks, join; where Horizon loses, and how to run it again.
- [Server against vanilla](performance/server-vs-vanilla.md): Horizon's server and vanilla 26.3 under the same bots at 1, 10, 25 and 50 players: tick time, CPU, memory, chunks and startup.
- [The modding book](modding/src/introduction.md): content mods (data) and native mods (Rust), how to make, install and publish them. [Native mods](modding/src/how-to/native-mods.md). [Hook counterparts](https://github.com/horizon-rust/horizon/tree/main/crates/horizon-mod-vocab/src/hooks): the NeoForge and Fabric catalog conventions.
- [Upgrading a 26.2 data pack](datapack-upgrade.md): `horizon-port-pack upgrade-datapack` rewrites a 26.2 worldgen pack in 26.3's formats, and how the rewrite is checked against vanilla.
- [Modpacks](modpacks.md): porting whole packs (ATM10 and smaller): import, inventory, carry-over, an agent per mod with a harness against the Java original, coverage, and how "10x" is measured.
- [Versioning](versioning.md): how releases are numbered from 0.2 (`v0.2.x`), and how installs from before 0.2 get there.
- [Benchmark report format](benchmark-report.md): the versioned JSON every benchmark writes (game, `tools/perf_bench.py`) and the local log.
- [Hot-path performance trends](hot-path-trends.md): the dispatch-only job that counts instructions and allocations of the tick, commands, chunk saving and mod dispatch on fixed workloads, and how to compare two runs.
- [Security](security.md): how Horizon protects code, CI and automatic releases against untrusted input while work is performed by AI agents.
- [Known issues](known-issues.md): player-facing issues listed in each game release and removed as pull requests fix them.
- [Horizon APIs](api/README.md): every surface other programs build on (mods, servers, data formats), each with its guide, generated reference, stability and changelog, and the [stability and versioning policy](api/stability.md).

## Launcher and platforms


## Parity

- [Rust rewrite status](rust-parity.md): where the state of the port lives (harnesses, port ledger, known issues) and the rules every port keeps.
- [Parity workflow](parity-workflow.md): `tools/parity.py`, the gate, other machines, merging.
- [Paper event parity](paper-parity.md): local trace comparisons, scenarios and tolerances, and informational nightly/manual CI.
- [CI runners](ci-runners.md): which jobs run on GitHub's runners and which on ours (the CI VPS, playhorizon, the Mac mini), the `SELF_HOSTED_CI` switch, and where a fork's pull request may run.
- Area notes: [block behaviours and ticking](parity/blocks.md), [section meshing](parity/meshing.md), [redstone machines tick by tick](parity/redstone-diff.md), [the same seed in vanilla and Horizon](parity/world-compare.md).
- [Port ledger](port-ledger.md): `tools/port_ledger.py`, every vanilla method next to the Rust functions that port it, both ways, with the gaps where a port isn't wired like vanilla.
- [Version ports](version-ports.md): how a new Minecraft release is detected, diffed against the pinned one and ported, and what stays manual.
- [Reading vanilla](parity-workflow.md#reading-vanilla): the local decompiled sources and bytecode queries the harnesses build on.

- [Translations](translations.md): language coverage and human review of machine-written strings.

## Design notes

- [Live mod reloading](design/mod-hotload.md): loading, enabling and updating mods from inside the game without a restart.
- [Wasm group faults](design/wasm-group-faults.md): what a wasm dependency group does after a trap or an interruption, and how interruption counts guest work.
- [Mod registry v2](design/registry-v2.md): project kinds, metadata, installation locations and the registry rollout.
- [Scoped whole-stack rollback](design/stack-snapshot.md): transaction-scoped, same-place savepoints for internals mods; the real transaction lifetime must be confirmed before implementation, and provider-owned places are initially refused.

## Architecture: world generation

- [World generation](world-generation.md): the pipeline from noise to features. Each stage's vanilla comparison is its harness (`tools/test_<name>_parity.py`); `python3 tools/port_ledger.py where Class.method` finds the Rust port of a vanilla method.
- [World height](world-height.md): each level's shape from its dimension type, data packs' taller levels.
- [Pregeneration](pregeneration.md): `/pregen`, generating, lighting and saving an area ahead of the players.

## Architecture: world data and simulation

- [World data](world-data.md): chunk storage, palettes, region files.

## Java comparison profiling

- [JFR analysis for Java comparisons](profiling.md).
