//! Small per-creature reads and writes of the host: the breath, the uuid words, the random seed, the pitch.
use crate::bridge::host::EcsWorld;
use crate::bridge::uuids::{EntityUuid, to_words};
use crate::cow::{Air, AirLow, FluidParams};
use crate::flat::Rng;
use crate::nbt_fields::common::Pitch;
use bevy_ecs::entity::Entity;

impl EcsWorld {
    /// `Entity.getAirSupply`; None when the creature breathes nothing (no `Air`).
    pub fn air(&self, e: Entity) -> Option<i32> {
        self.world().get::<Air>(e).map(|a| a.0)
    }

    /// `Entity.setAirSupply`: the breathing module takes over through `AirLow` while the air is below the creature's maximum.
    pub fn set_air(&mut self, e: Entity, air: i32) -> bool {
        let w = self.world_mut();
        let max = w.get::<FluidParams>(e).map_or(FluidParams::DEFAULT.max_air, |p| p.max_air);
        let Some(mut a) = w.get_mut::<Air>(e) else { return false };
        a.0 = air;
        if air < max {
            w.entity_mut(e).insert(AirLow);
        }
        true
    }

    /// The four words of the uuid the host gave `e`; None while it has none.
    pub fn uuid_words(&self, e: Entity) -> Option<[i32; 4]> {
        self.world().get::<EntityUuid>(e).map(|u| to_words(u.0))
    }

    /// Seeds the creature's own random stream (a fixture's reproducible draw).
    pub fn set_random_seed(&mut self, e: Entity, seed: u64) -> bool {
        match self.world_mut().get_mut::<Rng>(e) {
            Some(mut r) => {
                r.0 = seed;
                true
            }
            None => false,
        }
    }

    /// `Entity.tickCount`: the ticks `e` has lived awake, wrapped to an `int` as vanilla's is.
    pub fn age(&self, e: Entity) -> Option<i32> {
        let w = self.world();
        w.get_entity(e).ok()?;
        Some(w.get_resource::<crate::cow::Clock>()?.of(e) as i32)
    }

    /// The villager's inventory, slot by slot (`Villager.inventory`): the item and its count; empty when it carries nothing.
    pub fn inventory_stacks(&self, e: Entity) -> Vec<(String, u8)> {
        self.world().get::<crate::behaviours::food_inventory::Stock>(e).map_or_else(Vec::new, |s| s.0.clone())
    }

    /// Takes up to `count` of `item` out of the villager's inventory; the number taken.
    pub fn inventory_take(&mut self, e: Entity, item: &str, count: u8) -> u8 {
        crate::behaviours::food_inventory::take(self.world_mut(), e, item, count)
    }

    /// One draw of the drops' own generator: the seed of the random a thrown item's motion is drawn from (`Entity.drop`).
    pub fn drop_seed(&mut self) -> u32 {
        self.world_mut().get_resource_or_insert_with(crate::frame_item::DropRng::default).0.next()
    }

    /// The pitch the creature carries, in degrees; None while it carries none.
    pub fn pitch(&self, e: Entity) -> Option<f32> {
        self.world().get::<Pitch>(e).map(|p| p.0)
    }
}
