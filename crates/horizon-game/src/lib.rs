//! The dedicated server as the platform runs it, for the `myriade-engine` executable to start
//! (docs/layers.md, "How a Horizon crate plugs in"). The executable registers the platform's
//! hooks (`horizon_platform::hooks`) and then calls [`main`].
//!
//! An executable built on it:
//!
//! ```ignore
//! fn main() {
//!     horizon_game::run_main(|| {
//!         my_crate::register();
//!         horizon_game::main();
//!     });
//! }
//! ```
//!
//! The platform's own parts register here, in [`main`], as they leave the engine (layer plan M6).
//! Dedicated server only: no window, rendering or launcher part is built here.
#![forbid(unsafe_code)]

/// Runs the game with the hooks registered so far, and returns when it ends. Register hooks
/// (`horizon_platform::hooks::register`) before calling it.
pub fn main() {
    horizon_guest_stack::prepare_main();
    let startup = myriade_engine::entry::prepare();
    myriade_engine::entry::start(startup);
}

/// Establish the executable main thread stack before initialization.
pub use horizon_guest_stack::prepare_main as prepare_main_stack;
/// Run an executable's whole `main` on a main thread stack the guest fits on.
pub use horizon_guest_stack::run_main;
