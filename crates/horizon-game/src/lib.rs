//! The dedicated server as the platform runs it, for the `myriade-engine` executable to start
//! (docs/layers.md, "How a Horizon crate plugs in"). The executable registers the platform's
//! hooks (`horizon_platform::hooks`) and then calls [`main`].
//!
//! An executable built on it:
//!
//! ```ignore
//! static GAME_IDENTITY: [u8; horizon_game::GAME_IDENTITY.len()] = horizon_game::GAME_IDENTITY;
//!
//! fn main() {
//!     horizon_game::run_main(|| {
//!         std::hint::black_box(&GAME_IDENTITY);
//!         my_crate::register();
//!         horizon_game::main();
//!     });
//! }
//! ```
//!
//! The platform's own parts register here, in [`main`], as they leave the engine (layer plan M6).
//! Dedicated server only: no window, rendering or launcher part is built here.
#![forbid(unsafe_code)]

#[allow(unsafe_code)]
mod native_libraries;
mod loading;

pub use loading::main_with;
pub use native_libraries::native_main;
pub use mod_points::__private::SlotListener;

/// Identifies an executable as the game, without an API version or build id.
pub const GAME_IDENTITY: [u8; horizon_mod_package::game_identity::MARK.len()] =
    horizon_mod_package::game_identity::MARK;

/// Runs the game with the hooks registered so far, and returns when it ends. Register hooks
/// (`horizon_platform::hooks::register`) before calling it.
pub fn main() {
    horizon_guest_stack::prepare_main();
    exit_if_compile_child();
    let startup = myriade_engine::entry::prepare();
    std::hint::black_box(&horizon_mod_package::holder::CAPABILITY);
    loading::main_with(Vec::new(), &[], &[], startup);
}

/// Handles the private compiler invocation before any game startup work.
pub fn exit_if_compile_child() {
    horizon_guest_stack::prepare_main();
    if let Some(code) = horizon_mod_wasm::compile_child(std::env::args_os().skip(1)) {
        std::process::exit(code);
    }
}

/// Establish the executable main thread stack before initialization.
pub use horizon_guest_stack::prepare_main as prepare_main_stack;
/// Run an executable's whole `main` on a main thread stack the guest fits on.
pub use horizon_guest_stack::run_main;
