//! The rest memo of a `Mob` cow (`cow_rest.rs`), off unless the bench or the test turns it on; a clone keeps the switch.
#[derive(Default)]
pub struct MobMemo(pub super::cow_rest::RestState);
impl Clone for MobMemo {
    fn clone(&self) -> Self {
        Self(super::cow_rest::RestState { enabled: self.0.enabled, ..Default::default() })
    }
}
impl std::fmt::Debug for MobMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MobMemo")
    }
}
