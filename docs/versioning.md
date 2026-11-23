# Versioning

How Horizon's releases are numbered from 0.2 on, how that maps onto the release workflows, and how every install
made before 0.2 gets there. The workflows themselves are in [release-flow.md](release-flow.md).

From 0.2 the game follows [semantic versioning](https://semver.org): **Horizon 0.2.x** is the game line.

**Since 2026-10-28 the game ships only Nightlies** ([release-flow.md](release-flow.md#nightly-only)): no new `vX.Y.Z`
game tag is made, and a game build is named `nightly-<date>-<commit>` (its release title, "Nightly <date> ·
<commit>"), which servers rank after every `vX.Y.Z`.
the release line. What follows describes the `vX.Y.Z` game tags as they were.

## Tags

| | Game |
| --- | --- |
| From 0.2 | `v0.2.0`, `v0.2.1`, … |
| Before 0.2 | `v2026.10.19`, `v2026.10.19.15`, … |
| Release title | `Horizon 0.2.3: <what changed>` |
| The program says | `horizon-server 0.2.3`, `v0.2.3` in crash reports |

- **The line** (`MAJOR.MINOR`) is in `.github/release-line.txt`, one line such as `0.2`. Ludovic decides when it
  moves; moving it is a one-line pull request (`0.3`), and the new line's first release (`v0.3.0`) goes out as soon as it merges. Without the file, the release train keeps the tags from before
  0.2. A `feature` label no longer bumps anything: it only lists the change under **New**.
- **Patch releases** are every release the train makes: the patch after the newest release tagged on the line.
- **One channel** (since 2026-10-23): every game release is published under its final tag as the release and
  GitHub's latest, once its smoke test passed and no `release-blocker` is open
  ([One channel](release-flow.md#one-channel)). The game pre-releases from before (v0.2.1 on, the old Alpha
  channel) keep their tags and stay pre-releases.
- **Pre-release tags** (`v0.3.0-rc.1`, `v0.3.0-beta.2`) are semver's and are reserved for a hand-made preview of a
  new line; the train never makes them. Every reader orders them before their release and after the release
  before (`v0.2.9` < `v0.3.0-beta.1` < `v0.3.0-rc.1` < `v0.3.0`).


### The order every reader uses

Every 0.2 tag is newer than every tag from before 0.2. Within each:

- before 0.2: number by number, missing numbers as zero (`v2026.10.19` = `v2026.10.19.0`);
- from 0.2: semver: major, minor, patch as numbers, then a release after its pre-releases, whose identifiers
  compare one by one (numbers before words, numbers as numbers, a shorter list first).

A tag is a 0.2 tag when it is `v` followed by exactly three numbers (and an optional `-PRE`) with a
major below 1000; a date's first number is a year, so `v2026.10.21` never is. The rule is written once per
language, with the same tests: `crates/horizon-server/src/update.rs` (`newer`),
`tools/release_train.py` (`tag_key`) and
the website's `src/versions.js` (horizon-rust/horizon-site).

## The workflows

- **`release.yml`**: `tools/release_train.py decide --kind game` picks the tag (`next_game_tag`); the build gets the
  version without its `v` in `HORIZON_VERSION`, so `horizon_core::release::VERSION` (and `--version`, the user
  agents and crash reports) say `0.2.3`. No version-bump commits: `Cargo.toml` versions are what builds
  from source say. The build checks that `horizon-server --version` and `horizon-prepare --version` say it.

- **Notes**: a line's first release (`X.Y.0`) opens with a paragraph saying so; `v0.2.0`'s say
  that releases are numbered from now on, and tell server owners to run `horizon-server update` (or, if their server
  says it is up to date, `horizon-server update --version v0.2.0`).
- **A line's first release** goes out as soon as the release train runs after the line changes, without waiting for
  a pull request, but never while `RELEASES_PAUSED` is set.

## Existing installs: the way to 0.2

Installs made before 0.2 run old code that compares tags number by number, so to them `v0.2.0` is older than
`v2026.10.19.15`. What each one does, and what moves it:

| Install | What its old code does with 0.2 | How it gets there |
| --- | --- | --- |

| `horizon-server` from before the bridge | Reads GitHub's *latest* release and installs it only if its tag compares higher: `v0.2.0` never does. `--version <tag>` skips the comparison. A server never updates itself: its supervisor only logs a notice every 6 hours, and the running process keeps its old code until the service restarts. | **The game bridge**: the last dated release, built with the new code, made stable (*latest*) before `v0.2.0` is published. `v0.2.0` then spends at least 6 hours as a beta, so *latest* stays the bridge for that long (the longer it stays a beta, the more servers are caught). An owner who runs `horizon-server update` in that time gets the bridge, and the command runs the new code at once, so the next `horizon-server update` takes `v0.2.0` once it is stable. The log notice only changes after the service restarts. An owner on old code after that time is told the server is up to date, and runs `horizon-server update --version v0.2.0` once. 0.2.0's notes say so. |
| `install.sh`, `install.ps1` | Install *latest*. | Nothing to do. |
| Crash reports of releases before 0.2 | Report the folder name (`v2026.10.19.15`). | Unchanged; 0.2 builds report `v0.2.x` from the built-in version. |

No change on the feed side alone could do it: the dated releases can't be hidden (old servers also compare with the
versions they have installed, and *latest* is a single release), and GitHub's release JSON can't be rewritten. So the
bridges are required, and must never be withdrawn or deleted: the last dated release stays the step every old server
takes, for as long as any is around.

Outside this repository, whatever reads tags must accept `vX.Y.Z` too: Kira's release posts,
the fleet's `release-kick` and the orchestrator's heartbeat prompts.

## Irreversible

**None of these can be undone.**

- Once 0.2 is published, dated tags can never be used again: every reader ranks them below `v0.2.0`.

- `.github/release-line.txt` must never be removed.
- The last dated game release must never be withdrawn or deleted.
