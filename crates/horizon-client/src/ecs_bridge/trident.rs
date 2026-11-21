//! The thrown tridents on `EcsLevel`: a throw by the host, the pickup a player asks (by network id) and the events since the last call.
//! Through `ecs()` / `ecs_mut()`: an inert level throws nothing and gives nothing.
use super::level::EcsLevel;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_ecs_proto::{targeting::Who, trident::{BlowBonus, Pickup}};

impl EcsLevel {
    /// Throws a trident from `at`, owned by `owner`, with its pickup mode; returns the trident's network id (0 when inert).
    pub fn throw_trident(
        &mut self,
        mw: &MobWorld<'_>,
        at: [f64; 3],
        velocity: [f64; 3],
        owner: Option<Who>,
        loyalty: u8,
        channeling: bool,
        pickup: Pickup,
        stack: ItemStack,
        alloc: &mut dyn FnMut() -> u32,
    ) -> u32 {
        let Some(ecs) = self.ecs_mut() else { return 0 };
        let e = ecs.throw_trident(mw, at, velocity, owner, loyalty, channeling, pickup, stack);
        self.id_for(e, alloc)
    }

    /// The player `player` takes the trident `id`: the stack it gives, or None when the pickup rule refuses.
    pub fn trident_pickup(&mut self, id: u32, player: u32) -> Option<ItemStack> {
        let ecs = self.ecs_mut()?;
        let e = ecs.entity_of(id)?;
        ecs.trident_pickup(e, player)
    }

    /// The stack the thrown trident `id` gives back; None when inert or `id` is no trident.
    pub fn trident_stack(&self, id: u32) -> Option<ItemStack> {
        self.ecs()?.trident_stack(id)
    }

    /// Sets the table of a creature's melee weapon (`EcsWorld::set_melee_gear`; no-op when inert).
    pub fn set_melee_gear(&mut self, f: horizon_ecs_proto::melee_gear::MeleeGearFn) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.set_melee_gear(f);
        }
    }

    /// Sets the enchantment bonus of a blow for the host's table (no-op when inert).
    pub fn set_trident_hit_bonus(&mut self, f: BlowBonus) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.set_trident_hit_bonus(f);
        }
    }

}
