//! What a cactus does to the entities that are not living, and the
//! `entity_damage` game event of their hits: `CactusBlock.entityInside` hurts
//! every entity inside it, so a boat (`AbstractBoat.tick`'s two
//! `applyEffectsFromBlocks`), a minecart (its behaviour's) and an experience
//! orb (`ExperienceOrb.tick`'s) wear down in one, and `VehicleEntity.hurtServer`
//! and `ItemEntity.hurtServer` post `entity_damage` for every hit they take.
use super::commands::Target;
use super::*;

/// The blocks a box less 1e-5 is in whose name is `name`: what
/// `Entity.checkInsideBlocks` visits, each block once.
fn cells_named(game: &Game, bounds: Aabb, name: &str) -> Vec<[i32; 3]> {
    let e = f64::from(1.0e-5_f32);
    let b = bounds.deflate(Vec3::new(e, e, e));
    let (lo, hi) = (b.min_position(), b.max_position());
    let facts = game.res.facts();
    let mut cells = Vec::new();
    for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
        for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
            for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                if &*facts.get(game.world.block(x, y, z)).name == name {
                    cells.push([x, y, z]);
                }
            }
        }
    }
    cells
}

impl Game<'_> {
    /// `VehicleEntity.hurtServer`'s `gameEvent(ENTITY_DAMAGE, source.getEntity())`
    /// at the vehicle, after the damage is added and before it breaks.
    pub(in crate::game) fn vehicle_damage_event(&mut self, at: [f64; 3], by: Option<Target>) {
        self.sculk_prepare();
        self.entity_event_by("minecraft:entity_damage", at, by);
    }
    /// `ItemEntity.hurtServer`'s `gameEvent(ENTITY_DAMAGE, source.getEntity())`
    /// at the item, before a hit that empties its health destroys it.
    pub(in crate::game) fn item_damage_event(&mut self, at: Vec3, by: Option<Target>) {
        self.sculk_prepare();
        self.entity_event_by("minecraft:entity_damage", [at.x, at.y, at.z], by);
    }
    /// [`Self::hurt_boat`] for a source that names an entity.
    pub(super) fn hurt_boat_by(
        &mut self,
        id: u32,
        damage: f32,
        creative: bool,
        by: Option<Target>,
    ) -> bool {
        let who = by.and_then(|t| match t {
            Target::Player => u32::try_from(self.own_entity_id()).ok().map(horizon_ecs_proto::targeting::Who::Player),
            Target::Mob(mob) => self.ecs.entity_of(mob).map(horizon_ecs_proto::targeting::Who::Entity),
            _ => None,
        });
        self.hurt_boat_result(id, damage, creative, who)
            .unwrap_or(false)
    }
    /// `ExperienceOrb.tick`'s `applyEffectsFromBlocks` after its move: each
    /// cactus its box is in takes a point off its 5 health
    /// (`ExperienceOrb.hurtServer`, no event). True when that destroyed it.
    pub(super) fn orb_cactus_hits(&mut self, i: usize) -> bool {
        let bounds = self.orbs[i].bounds();
        let hits = cells_named(self, bounds, "minecraft:cactus").len();
        (0..hits).any(|_| self.orbs[i].hurt(1.0))
    }
}

#[cfg(test)]
#[path = "tests/nonliving_hurt_tests.rs"]
mod tests;
