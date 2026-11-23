//! The fishing hook as the server's level casts and reels it: a hook is bound to a network id at its cast, and reeled by that id.
use super::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::bridge::ids::id_of;
use horizon_ecs_proto::hook_extract::Retrieve;
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// Casts the hook of `owner` and returns its network id, bound by `alloc`; 0 when inert.
    pub fn cast_hook(&mut self, mw: &MobWorld<'_>, owner: u32, at: [f64; 3], velocity: [f64; 3], luck: u8, speed: u8, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(w) = self.ecs_mut() else { return 0 };
        let e = w.cast_hook(mw, owner, at, velocity, luck, speed);
        w.id_for(e, alloc)
    }

    /// Reels the owner's hook (None when it has none, or `owner` is not a player).
    pub fn reel_hook(&mut self, mw: &MobWorld<'_>, owner: Who) -> Option<Retrieve> {
        self.ecs_mut()?.reel_hook(mw, owner)
    }

    /// The experience of one object taken from the water (1 to 6).
    pub fn hook_xp(&mut self) -> u32 {
        self.ecs_mut().map_or(0, |w| w.hook_xp())
    }

    /// The weather changed: the hooks in the water read their fish again.
    pub fn weather_changed(&mut self, mw: &MobWorld<'_>) {
        if let Some(w) = self.ecs_mut() {
            w.weather_changed(mw);
        }
    }

    /// A block changed at `cell`: the hooks under it leave the wait.
    pub fn hook_block_changed(&mut self, mw: &MobWorld<'_>, cell: [i32; 3]) {
        if let Some(w) = self.ecs_mut() {
            w.hook_block_changed(mw, cell);
        }
    }

    /// Reels the hook `id` for `player`; None when inert, when the hook is gone, or when it is not theirs.
    pub fn retrieve_hook(&mut self, mw: &MobWorld<'_>, id: u32, player: u32) -> Option<Retrieve> {
        let w = self.ecs_mut()?;
        let e = w.entity_of(id)?;
        w.retrieve_hook(mw, e, player)
    }

    /// The target the hook `hook_id` holds (a creature, or a player by id), None when it holds none.
    pub fn hook_attached(&self, hook_id: u32) -> Option<Who> {
        let w = self.ecs()?;
        w.hook_attached(w.entity_of(hook_id)?)
    }

    /// `FishingHook.openWater` of the hook `hook_id`; None when it is not a hook.
    pub fn hook_open_water(&self, hook_id: u32) -> Option<bool> {
        let w = self.ecs()?;
        w.hook_open_water(w.entity_of(hook_id)?)
    }

    /// The target the hook `hook_id` holds, as a network id; None when it holds none, a player, or an entity with no id yet.
    pub fn hook_attached_id(&self, hook_id: u32) -> Option<u32> {
        let w = self.ecs()?;
        match w.hook_attached(w.entity_of(hook_id)?)? {
            Who::Entity(e) => id_of(w.world(), e),
            Who::Player(_) => None,
        }
    }

    /// The network id of the hook of `owner`, None when it has none (or `owner` is not a player).
    pub fn hook_of(&self, owner: Who) -> Option<u32> {
        let Who::Player(id) = owner else { return None };
        let w = self.ecs()?;
        id_of(w.world(), w.hook_of(id)?)
    }

    /// Removes the hook `hook_id` and frees its owner; false when there is no such hook.
    pub fn remove_hook(&mut self, hook_id: u32) -> bool {
        let Some(w) = self.ecs_mut() else { return false };
        let Some(e) = w.entity_of(hook_id) else { return false };
        w.remove_hook(e)
    }
}
