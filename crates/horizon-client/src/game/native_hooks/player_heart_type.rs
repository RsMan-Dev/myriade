use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// Which hearts the local player's health bar draws (`PlayerHeartTypeEvent`): `kind` is
    /// vanilla's pick, and a mod may set another.
    pub(in crate::game) fn native_player_heart_type(
        &mut self,
        kind: &mut mod_api::client::hud_layout::HeartKind,
    ) {
        self.fire_player_heart_type(kind);
    }
}
