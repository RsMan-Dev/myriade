# Install a local unsigned developer base

Commit tracked changes, build the game and companion programs, generate the wasm standard library
beside the game, and prepare compatible exporters from your own Minecraft installation before
installing. The packager copies `horizon-wasm-std.wasm` from the game's folder for every developer
base, whether the mods you build are native or wasm; the command names it if it is missing:

```sh
cargo run --locked -p cargo-horizon-mod -- wasm-std CHECKOUT/target/debug/horizon-wasm-std.wasm
```

The install needs these files. The checkout's packager (`cargo run -p xtask -- package`, and
`package-server` for a server folder) checks them all before it writes anything, and names every
missing one in one message with the command that makes it. Its list is
`crates/horizon-mod-package/src/sdk/inputs.json`; a test keeps this table equal to it. In the
commands, `PROFILE` is the cargo flag of the profile the game program was built with (`--release`
for `target/release`, nothing for `target/debug`), `PROGRAM_FOLDER` its folder, and on Windows the
programs end in `.exe`:

<!-- required inputs: crates/horizon-mod-package/src/sdk/inputs.json, checked by a test -->
| File | Where | Needed by | How to make it |
|---|---|---|---|
| `horizon-prepare` | beside the game program | `package`, `package-server` | cargo build PROFILE -p horizon-prepare |
| `horizon-server` | beside the game program | `package`, `package-server` | cargo build PROFILE -p horizon-server |
| `horizon-wasm-std.wasm` | beside the game program | `package`, `package-server` | cargo run --locked -p cargo-horizon-mod -- wasm-std PROGRAM_FOLDER/horizon-wasm-std.wasm |
| `exporters/manifest.json` | in the checkout | `package`, `package-server` | horizon-prepare compile-exporters --root CHECKOUT, then copy its server, client and manifest.json into CHECKOUT/exporters/ |
<!-- end of required inputs -->

A native mod build needs the prebuilt [SDK](../../reference/sdk.md) of the base's exact commit. For a commit without a
published SDK, build it from the same checkout and name its folder; one folder holds one side:

```sh
python3 tools/prebuilt_game.py build SDK_FOLDER            # add --server for a dedicated server
HORIZON_DEVELOPER_SDK=SDK_FOLDER horizon-server native-build MODS --version COMMIT_TAG --trust-native-code
```

The variable is read for developer bases only, and the folder is used where it is. It must be
that commit's build for this computer, side and the server's Rust, as a downloaded SDK must;
anything else is refused. Without it, the server uses the commit's SDK release when one is
published, and otherwise says the build has no mod tools. The native mod builder never compiles the checkout itself. For native mixins, the pinned
source must also be available for copying the target functions.
