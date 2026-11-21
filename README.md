<h1 align="center">Myriade</h1>

<p align="center">
  <b>A Minecraft: Java Edition engine in Rust, rebuilt around an entity component system.</b><br>
  Same game for the player. Far more of it at once.
</p>

<p align="center">
  <a href="#status"><b>Status</b></a> ·
  <a href="#why"><b>Why</b></a> ·
  <a href="#how-it-works"><b>How it works</b></a> ·
  <a href="#build"><b>Build</b></a> ·
  <a href="#credits-and-licence"><b>Credits</b></a>
</p>

---

Myriade is a fork of [Horizon](https://github.com/horizon-rust/horizon), the from-scratch Rust
rewrite of Minecraft: Java Edition 26.3. It keeps Horizon's world, rendering, protocol and mod
system, and replaces how the game simulates things: every entity becomes plain data handled by
small systems, built on [`bevy_ecs`](https://crates.io/crates/bevy_ecs).

The goal is simple to state: **move everything to ECS and make every action as cheap as it can be,
so a world can hold far more than it could before.** Hundreds of thousands of entities, not
thousands.

## Status

> [!IMPORTANT]
> Myriade is under heavy development and is **not playable yet**.
> **The first playable builds are coming soon.**

Done so far: the game's mobs and other non-player entities are rewritten as components and
systems, each one checked against the original behaviour and measured for what it costs per tick,
awake and asleep.

Next: plugging the ECS into the running game in place of the original entity code, which is what
the first playable builds will ship. Block entities come after.

## Why

Minecraft simulates each mob as an object that is ticked every game tick, whether or not anything
is happening to it. That cost grows with every entity in the world, and it is what limits farms,
servers and large builds.

Myriade starts from the opposite rule:

- **An entity nobody is near should cost almost nothing.** It sleeps, and is woken by an event: a
  player arriving, a block changing, a hit.
- **Nothing is found by scanning.** State is written when something happens, not rediscovered every tick.
- **Every byte per entity is accounted for.** Rare state is stored sparsely, so a cow does not pay for what only a dragon needs.
- **The player sees the same game.** The order in which things are computed is free; the outcome is not. Minecraft's own behaviour is the reference.

## How it works

| Piece | What it is |
|---|---|
| **Entity** | An identifier. No data, no code. |
| **Component** | Data attached to an entity: health, velocity, a target, a cooldown. |
| **System** | A function that handles every entity holding certain components, whatever kind of mob it is. |
| **Rule** | The pure calculation a system calls: values in, value out. |

A mob is therefore not a class. It is a **configuration**: a list of components and their starting
values. A component never knows which mob it belongs to, and an entity type tag is just a marker
component that systems can query.

Because rules are pure functions with no engine types in their signatures, they are meant to be
**reused by mods** for their own entities, and each one can be checked against the original game
input by input.

The same split is what the simulation is designed to scale on: entities can decide in parallel
from read-only data, and their effects on the shared world are then applied in a fixed order.

## Build

Myriade builds like Horizon: a Rust toolchain, or the provided [Nix](https://nixos.org) flake, and
**your own copy of Minecraft: Java Edition 26.3**. Myriade ships no Minecraft files. See the
[build guide](docs/native-client.md#run-from-a-checkout).

The documentation under [`docs/`](docs), along with `CONTRIBUTING.md`, `GOVERNANCE.md` and
`ROADMAP.md`, is inherited from Horizon and describes Horizon. Myriade's own documentation will
replace it as the project settles.

## Credits and licence

Myriade exists because of [Horizon](https://github.com/horizon-rust/horizon), founded by Ludovic
Marie and built by the Horizon Team and its contributors. Myriade is an independent fork: it is
not affiliated with or endorsed by the Horizon project.

Released under the [GNU LGPL, version 3 or later](LICENSE), like Horizon. Code Horizon merged
before its move from MIT stays available under the [MIT licence](LICENSES/MIT.txt). Dependencies
keep their own licences, see the [notices](THIRD_PARTY_NOTICES.md).

<sub>NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT. Minecraft is a trademark of Mojang Synergies AB; its code and assets belong to Mojang and Microsoft under the [Minecraft EULA](https://www.minecraft.net/en-us/eula).</sub>
