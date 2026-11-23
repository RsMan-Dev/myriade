//! Horizon: dedicated server for Minecraft Java Edition 26.3 (`myriade-engine --dedicated`).
#![forbid(unsafe_code)]

fn main() {
    horizon_guest_stack::run_main(|| {
        horizon_game::main();
    });
}
