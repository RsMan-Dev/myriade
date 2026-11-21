//! Direct move-control wishes, independent of navigation and terrain availability.
use super::{Game, commands::Target};
use mod_api::{MobRef, WantedPositionError};

impl Game<'_> {
    pub(super) fn mob_set_wanted_position(
        &mut self,
        mob: MobRef,
        at: [f64; 3],
        speed: f64,
    ) -> Result<bool, WantedPositionError> {
        if at.iter().any(|v| !v.is_finite()) {
            return Err(WantedPositionError::NonFinitePosition);
        }
        if !speed.is_finite() {
            return Err(WantedPositionError::NonFiniteSpeed);
        }
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return Ok(false);
        };
        if self.ecs.is_active() {
            // The move control is the ECS mob's `mov` state: a live mob takes the wish through the bridge.
            return Ok(self.ecs_mod_live(id).is_some() && self.ecs.set_wanted(id, at, speed));
        }
        let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id && !m.removed) else {
            return Ok(false);
        };
        mob.ai.mov.set_wanted(at[0], at[1], at[2], speed);
        Ok(true)
    }

    pub(super) fn mob_wanted_position(&self, mob: MobRef) -> Option<[f64; 3]> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.wanted(id));
        }
        Some(
            self.remote_entities
                .iter()
                .find(|m| m.id == id && !m.removed)?
                .ai
                .mov
                .wanted,
        )
    }

    pub(super) fn mob_speed_modifier(&self, mob: MobRef) -> Option<f64> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.move_speed_modifier(id));
        }
        Some(
            self.remote_entities
                .iter()
                .find(|m| m.id == id && !m.removed)?
                .ai
                .mov
                .speed_modifier,
        )
    }
}
