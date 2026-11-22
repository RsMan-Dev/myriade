//! Dispensers, shears and maces on the ECS level: reads of P's components, writes by the bridge.
use crate::game::Game;
use horizon_client::{
    items::ItemStack,
    mobs::species::{self, Species},
};
use bevy_ecs::entity::Entity;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    bridge::{
        host::{HurtSource, Select},
        ids::id_of,
    },
    cow::{Bounds, GrowAt, Health, Pos, TypeName},
    goals::{self, Slot},
    items::{item_id, item_name},
    leash::{self, Holder},
    tame::Owner,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// The rules a dispenser or a mace reads of an ECS entity, from its components.
#[derive(Clone, Copy)]
pub(in crate::game) struct EcsMob {
    pub kind: &'static str,
    pub species: Option<Species>,
    pub at: Vec3,
    pub height: f64,
    pub alive: bool,
    pub baby: bool,
    pub tame: bool,
}

/// The species of each vanilla id, built once from the table of the species.
static SPECIES: OnceLock<HashMap<String, Species>> = OnceLock::new();

/// The species a vanilla id names (`minecraft:cow`); None for a kind the table lacks.
pub(in crate::game) fn species_of(kind: &str) -> Option<Species> {
    SPECIES
        .get_or_init(|| {
            species::ALL.iter().map(|d| (species::vanilla_id(d.species).to_owned(), d.species)).collect()
        })
        .get(kind)
        .copied()
}

impl Game<'_> {
    /// The ECS entities whose box is near `block`, as (position, network id, box), from the spatial index: the caller keeps the
    /// overlaps. The radius covers the largest box that can reach a block from its centre.
    pub(in crate::game) fn ecs_in_box(&self, block: Aabb) -> Vec<(Vec3, u32, Aabb)> {
        let c = block.center();
        let (max, min) = (block.max_position(), block.min_position());
        let half = f64::from((max.x - min.x).max(max.y - min.y).max(max.z - min.z)) * 0.5;
        let radius = half + f64::from(self.ecs.max_half_width());
        let q = Select { kind: None, tag: None, near: Some(([c.x, c.y, c.z], radius)), limit: usize::MAX };
        let Some(w) = self.ecs.world() else { return Vec::new() };
        self.ecs
            .select(&q)
            .into_iter()
            .filter_map(|e| Some((w.get::<Pos>(e)?.0, id_of(w, e)?, w.get::<Bounds>(e)?.0)))
            .collect()
    }
    /// The rules of the entity `id`; None when it is not in the level or sleeps.
    pub(in crate::game) fn ecs_mob(&self, id: u32) -> Option<EcsMob> {
        let w = self.ecs.world()?;
        let e = self.ecs.entity_of(id)?;
        let kind = w.get::<TypeName>(e)?.0;
        Some(EcsMob {
            kind,
            species: species_of(kind),
            at: w.get::<Pos>(e)?.0,
            height: w.get::<Bounds>(e).map_or(0.0, |b| f64::from(b.0.max_position().y - b.0.min_position().y)),
            alive: w.get::<Health>(e).is_some_and(|h| h.0 > 0.0),
            baby: w.get::<GrowAt>(e).is_some(),
            tame: w.get::<Owner>(e).is_some_and(|o| o.0.is_some()),
        })
    }
    /// The item in a slot of the entity `id`, by its name; None when the slot is empty.
    pub(in crate::game) fn ecs_item(&self, id: u32, slot: Slot) -> Option<Arc<str>> {
        let w = self.ecs.world()?;
        let e = self.ecs.entity_of(id)?;
        item_name(goals::equipped(w, e, slot)).map(Arc::from)
    }
    /// `setItemSlot` of one `stack` on the entity `id`, then `setPersistenceRequired`: the item id goes in `Equipment`.
    pub(in crate::game) fn ecs_set_slot(&mut self, id: u32, slot: Slot, stack: &ItemStack) {
        let Some(e) = self.ecs.entity_of(id) else { return };
        self.ecs.equip(e, slot, item_id(&stack.item));
        self.ecs.set_persistent(e);
    }
    /// `setItemSlot(slot, EMPTY)` on the entity `id`: the item it held comes back as one stack.
    pub(in crate::game) fn ecs_take(&mut self, id: u32, slot: Slot) -> Option<ItemStack> {
        let item = self.ecs_item(id, slot)?;
        let e = self.ecs.entity_of(id)?;
        self.ecs.equip(e, slot, 0);
        Some(ItemStack::new(&item, 1))
    }
    /// `mob.playAmbientSound()` on the entity `id`: the bridge queues the sound at its place, the host plays it.
    pub(in crate::game) fn ecs_ambient(&mut self, id: u32) {
        self.ecs.play_ambient(id);
    }
    /// `setPersistenceRequired` on the entity `id`.
    pub(in crate::game) fn ecs_persist(&mut self, id: u32) {
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.set_persistent(e);
        }
    }
    /// Whether the entity `id` stands on the ground, from its view (`EntityView.on_ground`); None when it is not in the level.
    pub(in crate::game) fn ecs_on_ground(&self, id: u32) -> Option<bool> {
        let w = self.ecs.world()?;
        horizon_ecs_proto::bridge::view::view(w, self.ecs.entity_of(id)?).map(|v| v.on_ground)
    }
    /// The stack in the main hand of the entity `id` (`LivingEntity.getMainHandItem`).
    pub(in crate::game) fn ecs_main_hand(&self, id: u32) -> Option<ItemStack> {
        let w = self.ecs.world()?;
        goals::main_hand(w, self.ecs.entity_of(id)?).cloned()
    }
    /// `Leashable.dropLeash` on the ECS level: detaches the leash of `own` and of the entities within `area` that `holder`
    /// holds. Returns where each detached entity drops its lead; nothing when none was leashed.
    pub(in crate::game) fn ecs_unleash(&mut self, own: Option<u32>, holder: Option<Holder>, area: Aabb) -> Vec<Vec3> {
        let own = own.and_then(|id| self.ecs.entity_of(id));
        let Some(w) = self.ecs.world_mut() else { return Vec::new() };
        let held = holder.map_or_else(Vec::new, |h| leash::held_by(w, h));
        let within: Vec<Entity> = held
            .into_iter()
            .filter(|&x| w.get::<Pos>(x).is_some_and(|p| area.intersects(Aabb::new(p.0, p.0))))
            .collect();
        let mut out = Vec::new();
        for x in own.into_iter().chain(within) {
            let at = w.get::<Pos>(x).map(|p| p.0);
            if leash::detach(w, x).is_some() {
                out.extend(at);
            }
        }
        out
    }
    /// `LivingEntity.hurtServer` on the entity `id`: the ECS applies it, and its events come back on the next tick.
    pub(in crate::game) fn ecs_hurt(&mut self, id: u32, amount: f32, source: HurtSource) {
        let Some(e) = self.ecs.entity_of(id) else { return };
        if let HurtSource::Player(player) = source {
            self.ecs_note_killer(e, player);
        }
        self.with_ecs(|ecs, mw| ecs.hurt(mw, e, amount, source));
    }
}

#[cfg(test)]
mod tests {
    use super::species_of;
    use crate::game::ecs::test_support::ecs_game_with;
    use horizon_client::items::ItemStack;
    use horizon_client::mobs::species::Species;
    use horizon_ecs_proto::goals::Slot;

    #[test]
    fn a_vanilla_id_names_its_species_and_nothing_else_does() {
        assert!(matches!(species_of("minecraft:cow"), Some(Species::Cow)));
        assert!(matches!(species_of("minecraft:sheep"), Some(Species::Sheep)));
        assert!(species_of("minecraft:not_a_mob").is_none());
    }

    #[test]
    fn a_dispensed_piece_is_held_by_the_entity_it_was_given_to() {
        let (mut game, id) = ecs_game_with("minecraft:zombie", [0.5, 64.0, 0.5]);
        assert!(game.ecs_item(id, Slot::Main).is_none());
        game.ecs_set_slot(id, Slot::Main, &ItemStack::new("minecraft:iron_sword", 1));
        assert!(game.ecs_item(id, Slot::Main).is_some());
    }

    #[test]
    fn a_sheep_is_shorn_once() {
        let (mut game, id) = ecs_game_with("minecraft:sheep", [0.5, 64.0, 0.5]);
        let e = game.ecs.entity_of(id).unwrap();
        assert!(game.ecs.world_mut().is_some_and(|w| horizon_ecs_proto::behaviours::shear::shear_entity(w, e, None)));
        assert!(!game.ecs.world_mut().is_some_and(|w| horizon_ecs_proto::behaviours::shear::shear_entity(w, e, None)));
    }

    #[test]
    fn a_mace_hit_reads_the_ground_of_the_target_from_its_view() {
        let (game, id) = ecs_game_with("minecraft:cow", [0.5, 64.0, 0.5]);
        assert!(game.ecs_on_ground(id).is_some());
        assert!(game.ecs_mob(id).is_some_and(|m| m.alive));
    }
}
