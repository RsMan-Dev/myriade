use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
pub const KINDS: [&str; 6] = ["load", "generate", "decorate", "finish", "light", "other"];
static ENABLED: AtomicBool = AtomicBool::new(false);
static NANOS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
static COUNT: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
pub fn enable(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}
pub(super) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
pub(super) fn add(kind: usize, nanos: u64) {
    NANOS[kind].fetch_add(nanos, Ordering::Relaxed);
    COUNT[kind].fetch_add(1, Ordering::Relaxed);
}
/// (kind, nanoseconds, count) since the last call; resets them.
pub fn take() -> Vec<(&'static str, u64, u64)> {
    (0..KINDS.len())
        .map(|i| {
            (
                KINDS[i],
                NANOS[i].swap(0, Ordering::Relaxed),
                COUNT[i].swap(0, Ordering::Relaxed),
            )
        })
        .collect()
}
