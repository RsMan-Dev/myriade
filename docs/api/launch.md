# Launch API

What a panel or a script needs to start the Horizon game as a dedicated server: the options it takes, how it ends,
where its crash report goes, and `--launch-api`, which prints the options. [horizon-server](../hosting.md) starts this
program for you; read this page when you start it yourself.

## Overview

| | |
| --- | --- |
| Version | 0.2.0, printed by `horizon-client --launch-api` and `horizon-prepare --launch-api` |
| Level | experimental ([stability.md](stability.md)) |
| Covers | starting `horizon-client --dedicated`: its options, the exit codes and the crash report's path |

## Ask first

`horizon-client --launch-api` prints one JSON object on stdout and exits 0, before anything else runs:

- `launch_api`: this API's version; `horizon`: the build's version; `minecraft`: the Minecraft version it runs.
- `options`: each option as `{"name", "value"}` (`value` is the placeholder, `null` for a flag).
- `exit_codes`: the codes listed below.
- `crash_report_line`: the text of the stderr line that names a saved crash report.
- `content`: what this build loads ([content.md](content.md)).

Readers ignore fields they don't know. Pass only the options in `options`. The game takes more (`--help` lists
them), but only `options` is promised: any option it doesn't know is refused with exit code 2.

## Start

```sh
horizon-client --resources PREPARED --game-dir INSTANCE --home INSTANCE --dedicated --nogui
```

## Options

| Option | What it does |
| --- | --- |
| `--resources DIR` | The prepared resources (`horizon-prepare`'s output). Default: `resources` beside the executable, else `.local/horizon-client/26.3`. |
| `--game-dir DIR` | The game folder: `saves/`, `server.properties`, `resourcepacks/`, `horizon-mods/`, `screenshots/`. Created when missing. Default: the working directory. `--gameDir` is the same. |
| `--home DIR` | Logs (`logs/latest.log`) and crash reports (`crash-reports/`). Default: `HORIZON_HOME`, else `horizon` in the platform data folder. |
| `--launch-api` | Prints the reply above and exits 0. |

Also accepted, and listed by `--help`:

- `--dedicated`: vanilla's server arguments (`--nogui`, `--port`, `--world`, `--universe`, `--initSettings`) follow it,
  and are passed on as they are. It must come last.
- `--world NAME`: the world folder under `saves/`.
- `--control-socket PATH`: accept control-socket commands on a local socket.
- `--pregen`: adds Horizon's `/pregen` command.
- `--help` (`-h`): prints the options and exits 0.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | The game ended normally, or `--help` or `--launch-api` printed. |
| 1 | The game stopped on an error. A crash report was saved. |
| 2 | The command line was refused: an unknown option, a missing or bad value. No crash report. |
| 101 | The game panicked. A crash report was saved. |

## Crash reports

On exit 1 or 101, the last line on stderr is `Horizon: crash report saved to ` followed by the report's path. The
report is a file `crash-<yyyy-MM-dd_HH.mm.ss>-<side>.txt` in `crash-reports/` under the `--home` folder, where
`<side>` is `client` or `server`. On exit 2, stderr says which option was refused (`Horizon: --width: unknown option`).

## Reference

[launch-reference.md](launch-reference.md) is generated from `crates/horizon-client/src/game/runtime/launch_api.rs`:
every option, the `--launch-api` reply and the exit codes.

## Preparing

`horizon-prepare --launch-api` prints the preparer's own reply on stdout and exits 0. Its version is the same as the game's.

## Stability

Experimental: the options in the reference, the exit codes and the crash report line may still change. From preview
on, they change only as [stability.md](stability.md) says, and the reference's test keeps the contract and the code
together.
