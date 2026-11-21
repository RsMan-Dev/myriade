//! Tells the library whether it is compiled optimized (`opt-level` 2 or more): the benches refuse to run otherwise (`bench_support::require_optimized`).
fn main() {
    let level = std::env::var("OPT_LEVEL").unwrap_or_default();
    if !matches!(level.as_str(), "2" | "3" | "s" | "z") {
        println!("cargo:rustc-cfg=unoptimized");
    }
    println!("cargo:rustc-check-cfg=cfg(unoptimized)");
    println!("cargo:rerun-if-changed=build.rs");
}
