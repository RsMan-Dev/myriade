//! `Entity.extinguishFire`, distinct from the silent `clearFire` write.
use super::{Game, commands::Target, cosmetic_random::cosmetic_random, powder_snow::extinguish_pitch};
use mod_api::{EntityRef, SoundSource};
use horizon_core::nbt::Tag;

impl Game<'_> {
    pub(super) fn entity_extinguish_fire(&mut self, entity: EntityRef) -> bool {
        match self.native_entity_target(entity) {
            Some(target @ (Target::Player | Target::Guest(_))) => self
                .for_player(target, |game| {
                    let Some(player) = game.player.as_ref() else {
                        return false;
                    };
                    let ticks = player.fire_ticks;
                    let at = [player.position.x, player.position.y, player.position.z];
                    if ticks > 0 {
                        let pitch = extinguish_pitch(&mut cosmetic_random());
                        game.audio.play_on(
                            SoundSource::Players.index(),
                            "minecraft:entity.generic.extinguish_fire",
                            Some(at),
                            0.7,
                            pitch,
                        );
                    }
                    if let Some(player) = game.player.as_mut() {
                        player.set_remaining_fire_ticks(ticks.min(0));
                    }
                    true
                })
                .unwrap_or(false),
            // The ECS mob's fire is its saved `Fire` ticks; clearing them is the vanilla clearFire.
            Some(Target::Mob(id)) if self.ecs_has(id) => self.ecs_put(id, "Fire", Tag::Short(0)),
            Some(Target::Mob(id)) => {
                let Some(mob) = self.remote_entities.iter_mut().find(|mob| mob.id == id) else {
                    return false;
                };
                // Own random, kind-owned fire immunity, sound outbox, then clearFire.
                mob.extinguish_fire();
                mob.needs_sync = true;
                true
            }
            Some(Target::Stand(id)) => {
                let Some(index) = self.armor_stands.iter().position(|stand| stand.id == id) else {
                    return false;
                };
                let stand = &self.armor_stands[index];
                let ticks = stand.fire_ticks;
                let at = [stand.position.x, stand.position.y, stand.position.z];
                if ticks > 0 {
                    let pitch = extinguish_pitch(&mut cosmetic_random());
                    self.audio.play_on(
                        SoundSource::Neutral.index(),
                        "minecraft:entity.generic.extinguish_fire",
                        Some(at),
                        0.7,
                        pitch,
                    );
                }
                self.armor_stands[index].fire_ticks = ticks.min(0);
                true
            }
            Some(Target::Item(id)) => {
                let Some(item) = self.ecs.item(id) else {
                    return false;
                };
                let immune = self.stack_can_be_hurt_by(&item.stack, "minecraft:in_fire") == Some(false);
                if item.fire_ticks > 0 && !immune {
                    let at = [item.position.x, item.position.y, item.position.z];
                    let pitch = extinguish_pitch(&mut cosmetic_random());
                    self.audio.play_on(
                        SoundSource::Ambient.index(),
                        "minecraft:entity.generic.extinguish_fire",
                        Some(at),
                        0.7,
                        pitch,
                    );
                }
                self.ecs_put(id, "Fire", Tag::Short(item.fire_ticks.min(0) as i16))
            }
            _ => false,
        }
    }
}
