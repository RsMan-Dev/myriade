# Content API

## Overview

| | |
| --- | --- |
| Level | experimental ([stability.md](stability.md)) |
| Covers | the folders and files of each kind under `--game-dir`, turning content on and off, when a change applies, what to check before installing, what a build loads |

## What a build loads

Offer a version only when this build loads it:

| Kind | Loads when | From `content` |
| --- | --- | --- |
| Content mod (`.hmod`) | its manifest `api` is listed | `mod.api` |
| Native mod (`.hmod`) | the game is rebuilt with it for this exact build | `native_mod.exact_build`, `native_mod.rebuild` |
| Resource pack | always; outside `format` it is marked too old or too new, as vanilla marks it | `resourcepack.format` |
| Data pack | the same | `datapack.format` |
| Modpack (`.hpack`) | its `format` is this one | `modpack.format` |
| The index | its `schema` is at most this one | `index.schema` |

## Where content goes

Paths are under the game folder (`--game-dir`). Change these files while the game is stopped: it reads them when it
starts and writes them when the player changes a setting.

| Kind | File | Folder | On and off |
| --- | --- | --- | --- |
| Content mod | `<id>.hmod`, or a folder with `horizon-mod.toml` | `horizon-mods/` | `horizon-mods.txt`: a `mod.<id>:true` line turns it on. A mod without one is off |
| Native mod | `<id>.hmod` with `[native]` in `horizon-mod.toml` | `horizon-mods/` | runs only in a game built with it: `mod.<id>:true` chooses it for the next build, `mod.<id>:false` turns a built one off |
| Resource pack | a `.zip` or a folder with `pack.mcmeta` | `resourcepacks/` | — |
| Data pack | a `.zip` or a folder with `pack.mcmeta` and `data/` | `saves/<world>/datapacks/` | `saves/<world>/datapacks.json`: `{"enabled": [...], "disabled": [...]}`, each `file/<name>` (or `vanilla`). A pack in neither list is on |

- **A modpack** isn't copied into the game folder. `overrides/` may hold only what [mod-index.md](mod-index.md#hpack)
  lists, within its size limits.

## Before installing a file

The checks `horizon_mod_package::index::check_file` makes, which [mod-index.md](mod-index.md#before-a-file-is-installed)
specifies.

1. **Where it comes from.** An `https://` address of the index's shape: a registry release
   (`https://github.com/horizon-rust/horizon-mods/releases/download/...`) or a release asset of the project's own
   GitHub repository. Anything else is refused.
2. **Size and SHA-256.** The size is at most 64 MiB and equals the entry's, and the SHA-256 is the entry's.
   Check both before the file reaches the game folder.
3. **Signature.** When the project has a minisign `key`, the version's `signature` verifies with it, and a
   version signed with another key is refused. With no key, say the project is unsigned.
4. **Revoked.** A file in `revoked[]` (by `sha256`, or every version of an `id` without one) isn't installed. An
   installed copy is turned off (`mod.<id>:false`, or out of `resourcePacks`), and the player is told the reason.
5. **What the file says.** Its manifest matches the entry (id, version, API), as mod-index.md lists for each kind.

## Stability

Experimental: the folders, the keys and `content` may still change before preview. From preview on, they change only
as [stability.md](stability.md) says.



