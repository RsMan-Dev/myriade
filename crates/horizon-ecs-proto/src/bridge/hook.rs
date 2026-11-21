//! The fishing hook as the host casts and reels it: the hook is an entity of P, its state in `Hook`, and the host applies what
//! `retrieve` returns (the loot, the pull, the rod's damage). The hook's draws come from the level's random, as vanilla draws them.
use crate::bridge::host::EcsWorld;
use crate::cow::with_level_random;
use crate::fishing_hook;
use crate::hook_extract::{retrieve, Retrieve};
use crate::hook_wait::HookIndex;
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;

impl EcsWorld {
    /// `FishingHook(owner, ..)` thrown from `at` with `velocity`: `speed` is the rod's Lure, `luck` its Luck of the Sea.
    pub fn cast_hook(&mut self, mw: &MobWorld<'_>, owner: u32, at: [f64; 3], velocity: [f64; 3], luck: u8, speed: u8) -> Entity {
        self.lent(mw, |w| {
            let seed = with_level_random(w, |r| r.next_long());
            let (at, velocity) = (Vec3::new(at[0], at[1], at[2]), Vec3::new(velocity[0], velocity[1], velocity[2]));
            fishing_hook::cast(w, owner, at, velocity, i32::from(speed), luck, seed)
        })
    }

    /// `FishingHook.retrieve` for the hook `hook` of `player`; None when the hook is gone or is not theirs.
    pub fn retrieve_hook(&mut self, mw: &MobWorld<'_>, hook: Entity, player: u32) -> Option<Retrieve> {
        if self.world().get::<fishing_hook::Hook>(hook)?.owner != player {
            return None;
        }
        Some(self.lent(mw, |w| retrieve(w, hook, player)))
    }

    /// `FishingHook.retrieve` for the owner's hook, found by the owner's index (None when `owner` is not a player or has no hook).
    pub fn reel_hook(&mut self, mw: &MobWorld<'_>, owner: Who) -> Option<Retrieve> {
        let Who::Player(id) = owner else { return None };
        let hook = self.world().get_resource::<HookIndex>()?.owners.get(&id).copied()?;
        self.retrieve_hook(mw, hook, id)
    }

    /// The experience of one object taken from the water: 1 to 6, drawn once per object from the level's random.
    pub fn hook_xp(&mut self) -> u32 {
        with_level_random(self.world_mut(), |r| r.next_int_bound(6).unwrap_or(0) as u32 + 1)
    }


    /// The weather changed (rain or none): the hooks in the water read their fish's speed again.
    pub fn weather_changed(&mut self, mw: &MobWorld<'_>) {
        self.lent(mw, fishing_hook::weather_changed)
    }

    /// A block changed at `c`: the hooks under it read their fish's speed again (their sky).
    pub fn hook_block_changed(&mut self, mw: &MobWorld<'_>, c: [i32; 3]) {
        self.lent(mw, |w| fishing_hook::block_changed(w, c))
    }

    /// The target the hook holds (a creature, or a player by id), None when it holds none.
    pub fn hook_attached(&self, hook: Entity) -> Option<Who> {
        match self.world().get::<fishing_hook::Hook>(hook)?.phase {
            fishing_hook::Phase::Hooked(fishing_hook::Target::Entity(c)) => Some(Who::Entity(c)),
            fishing_hook::Phase::Hooked(fishing_hook::Target::Player(id)) => Some(Who::Player(id)),
            _ => None,
        }
    }

    /// The hook of `owner` (its last cast), None when it has none.
    pub fn hook_of(&self, owner: u32) -> Option<Entity> {
        let w = self.world();
        let e = *w.get_resource::<HookIndex>()?.owners.get(&owner)?;
        w.get::<fishing_hook::Hook>(e).is_some_and(|h| h.owner == owner).then_some(e)
    }

    /// Removes the hook `hook` and frees its owner's index entry; false when it is not a hook.
    pub fn remove_hook(&mut self, hook: Entity) -> bool {
        let w = self.world_mut();
        let Some(owner) = w.get::<fishing_hook::Hook>(hook).map(|h| h.owner) else { return false };
        if let Some(mut x) = w.get_resource_mut::<HookIndex>() {
            if x.owners.get(&owner) == Some(&hook) {
                x.owners.remove(&owner);
            }
        }
        w.despawn(hook);
        true
    }
}
