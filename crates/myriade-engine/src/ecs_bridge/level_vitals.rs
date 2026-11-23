//! The breath, the uuid words, the random seed and the pitch of a creature by id (`P bridge/host_vitals.rs`).
use super::level::EcsLevel;

impl EcsLevel {
    /// The air supply; 0 when the id is no breathing creature of the level.
    pub fn air(&self, id: u32) -> i32 {
        let Some(e) = self.entity_of(id) else { return 0 };
        self.ecs().and_then(|w| w.air(e)).unwrap_or(0)
    }
    pub fn set_air(&mut self, id: u32, air: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_air(e, air))
    }
    pub fn uuid_words(&self, id: u32) -> Option<[i32; 4]> {
        self.ecs()?.uuid_words(self.entity_of(id)?)
    }
    pub fn set_random_seed(&mut self, id: u32, seed: u64) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_random_seed(e, seed))
    }
    pub fn pitch(&self, id: u32) -> Option<f32> {
        self.ecs()?.pitch(self.entity_of(id)?)
    }
    /// `Entity.tickCount` of `id` (`EcsWorld::age`).
    pub fn age(&self, id: u32) -> Option<i32> {
        self.ecs()?.age(self.entity_of(id)?)
    }
    /// `LivingEntity.isBaby` of the mob `id` (`EcsWorld::is_baby`).
    pub fn is_baby(&self, id: u32) -> Option<bool> {
        self.ecs()?.is_baby(self.entity_of(id)?)
    }
    /// The tame flag and the owner's uuid words of `id`; None when it cannot be tamed (`EcsWorld::tamable_values`).
    pub fn tamable_values(&self, id: u32) -> Option<(bool, Option<[i32; 4]>)> {
        self.ecs()?.tamable_values(self.entity_of(id)?)
    }
    /// The entity id of the player that owns `id`; None when wild or owned by a creature (`EcsWorld::owner_player`).
    pub fn owner(&self, id: u32) -> Option<u32> {
        self.ecs()?.owner_player(self.entity_of(id)?)
    }
    /// The villager's inventory as stacks (`EcsWorld::inventory_stacks`).
    pub fn inventory_stacks(&self, id: u32) -> Vec<myriade_engine::items::ItemStack> {
        let (Some(ecs), Some(e)) = (self.ecs(), self.entity_of(id)) else { return Vec::new() };
        ecs.inventory_stacks(e).iter().map(|(item, n)| myriade_engine::items::ItemStack::new(item, u32::from(*n))).collect()
    }
    /// Takes up to `count` of `item` from the villager's inventory; the number taken (`EcsWorld::inventory_take`).
    pub fn inventory_take(&mut self, id: u32, item: &str, count: u8) -> u8 {
        let Some(e) = self.entity_of(id) else { return 0 };
        self.ecs_mut().map_or(0, |w| w.inventory_take(e, item, count))
    }
    /// `Entity.drop`'s motion and spawn point for `id`: the item leaves the eyes (0.3 below), scattered when `randomly`, else along the view.
    pub fn drop_motion(&mut self, id: u32, randomly: bool) -> Option<([f64; 3], [f64; 3])> {
        let [x, y, z] = self.position(id)?;
        let eye = self.eye_offset(id).unwrap_or(0.0);
        let (yaw, pitch) = (self.yaw(id).unwrap_or(0.0), self.pitch(id).unwrap_or(0.0));
        let seed = self.ecs_mut()?.drop_seed();
        let mut random = horizon_core::random::LegacyRandom::new(i64::from(seed));
        let v = myriade_engine::entities::drop_motion(&mut random, yaw, pitch, randomly);
        Some((v, [x, y + eye - f64::from(0.3f32), z]))
    }
}
