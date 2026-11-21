//! The actual player wrappers, damage continuations, effects and explosions reach one listener.
use super::entity::damage_level_of;
use super::native_entity_refs::tests::{bind_local_player, with_game};
use horizon_client::{
    command::args::GameMode,
    effects::EffectAction,
    mobs::{Damage, DamageKind, Mob, Who},
    motion::PlayerMotion,
    projectiles::{Hit, Projectile, ProjectileKind},
};
use horizon_core::geometry::Vec3;
use mod_api::{EntityInvulnerabilityCheckEvent, Hooks, LevelSide};
use std::sync::{Arc, Mutex};

struct Probe(Arc<Mutex<Vec<bool>>>, Arc<Mutex<Option<bool>>>);
#[mod_api::__subscriptions]
impl Hooks for Probe {
    fn entity_invulnerability_check(
        &self,
        _: LevelSide<'_>,
        event: &mut EntityInvulnerabilityCheckEvent<'_>,
    ) {
        self.0.lock().unwrap().push(event.original_invulnerability);
        if let Some(answer) = *self.1.lock().unwrap() {
            event.invulnerable = answer;
        }
    }
}

fn fresh_player(game: &mut super::Game<'_>, mode: GameMode) {
    game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
    bind_local_player(game, 7, 7);
    // A replacement player starts with survival abilities even when Game was
    // already creative; force the mode transition to apply them to the new body.
    game.set_game_mode(GameMode::Survival);
    game.set_game_mode(mode);
}
