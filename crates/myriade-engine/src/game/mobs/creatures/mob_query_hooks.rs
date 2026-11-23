// Included in creatures/mod.rs to preserve its existing scope.
impl myriade_engine::mobs::MobHooks for Game<'_> {
    fn enderman_anger_suppressed(&self, _enderman: u32, _player: &PlayerView) -> bool {
        false
    }
    fn can_grief(&self, _mob: u32, rule: bool) -> bool {
        rule
    }
    fn grief_hooked(&self) -> bool {
        false
    }
    fn living_visibility(&self, _player: &PlayerView, _looking: u32, visibility: f64) -> f64 {
        visibility
    }
}
