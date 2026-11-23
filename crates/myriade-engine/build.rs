//! Generates the native mod API, and gives the Windows executable its stack
//! (`packaging/windows/exe.rs`, shared with the dedicated server's build script).
#[path = "../../packaging/windows/exe.rs"]
mod windows_exe;

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = std::path::Path::new(&dir).join("../..");
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("crates/myriade-engine/src/game").display()
    );
    std::fs::write(
        out_dir.join("game_modules.rs"),
        horizon_game_discovery::declarations(
            &root.join("crates/myriade-engine/src/game"),
            &["native_hooks", "host", "creatures", "ecs"],
        )
        .expect("discovering game modules"),
    )
    .expect("writing game modules");
    let tests =
        root.join("crates/myriade-engine/src/crafting/ingredient/builtin_kinds_tests/families");
    println!("cargo:rerun-if-changed={}", tests.display());
    std::fs::write(
        out_dir.join("builtin_ingredient_test_modules.rs"),
        horizon_game_discovery::folder_modules(&tests).expect("discovering ingredient tests"),
    )
    .expect("writing ingredient test modules");
    // The launcher adds trait packages to the workspace's members.
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Cargo.toml").display()
    );
    for dir in ["native-api", "native-crates"] {
        if root.join(dir).is_dir() {
            println!("cargo:rerun-if-changed={}", root.join(dir).display());
        }
    }
    // The SDK builds this library, not the platform executable. Carry the same executable
    // policy without asking Cargo to link a binary this package no longer owns.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo must provide the target OS");
    let target_env =
        std::env::var("CARGO_CFG_TARGET_ENV").expect("Cargo must provide the target environment");
    println!(
        "cargo:rerun-if-changed={}",
        root.join("packaging/executable_link.rs").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("crates/horizon-guest-stack/src/guest_stack_constants.rs")
            .display()
    );
    for arg in windows_exe::executable_link::arguments(&target_os, &target_env) {
        println!("cargo:horizon-executable-link-arg={arg}");
    }
    if std::env::var("CARGO_PKG_NAME").as_deref() != Ok("myriade-engine") {
        windows_exe::resources("myriade-engine");
    }
}
