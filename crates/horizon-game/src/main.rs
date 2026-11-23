//! Horizon: dedicated server for Minecraft Java Edition 26.3 (`horizon-client --dedicated`).
#![forbid(unsafe_code)]

/// Identifies this executable as the game. `main` reads it so the linker keeps it; it carries
/// neither an API version nor a per-build id.
static GAME_IDENTITY: [u8; horizon_mod_package::game_identity::MARK.len()] =
    horizon_mod_package::game_identity::MARK;

fn main() {
    horizon_guest_stack::run_main(|| {
        std::hint::black_box(&GAME_IDENTITY);
        horizon_game::main();
    });
}
