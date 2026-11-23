//! Executables' stack and link arguments, shared by the build scripts of the game's executables
//! (`crates/horizon-client/build.rs`, `crates/horizon-game/build.rs`, which include it with
//! `#[path]`). The dedicated server has no icon or manifest resource.

#[path = "../executable_link.rs"]
pub mod executable_link;

/// The stack size of the executable `bin` of the package being built.
pub fn resources(bin: &str) {
    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target_os =
        std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo must provide the build target OS");
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("../../packaging/executable_link.rs")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("../horizon-guest-stack/src/guest_stack_constants.rs")
            .display()
    );
    for arg in executable_link::arguments(&target_os, &target_env) {
        println!("cargo:rustc-link-arg-bin={bin}={arg}");
    }
}
