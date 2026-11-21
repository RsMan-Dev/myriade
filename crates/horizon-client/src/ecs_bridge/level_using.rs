//! The item use of the entities by network id, through the bridge (`horizon-ecs-proto` `bridge/host_using.rs`): the start, the ticks left,
//! the release, the totem, the draws, the fire and the roar. An inert level answers as an empty world: none, false, nothing.
use super::level::EcsLevel;
use horizon_client::items::ItemStack;

impl EcsLevel {
    pub fn next_float(&mut self, id: u32) -> Option<f32> {
        let ecs = self.ecs_mut()?;
        let e = ecs.entity_of(id)?;
        ecs.next_float(e)
    }
    pub fn next_int_bound(&mut self, id: u32, bound: i32) -> Option<i32> {
        let ecs = self.ecs_mut()?;
        let e = ecs.entity_of(id)?;
        ecs.next_int_bound(e, bound)
    }
    pub fn off_hand_stack(&self, id: u32) -> Option<ItemStack> {
        let ecs = self.ecs()?;
        ecs.off_hand_stack(ecs.entity_of(id)?)
    }
    pub fn is_using_item(&self, id: u32) -> bool {
        self.ecs().and_then(|ecs| ecs.entity_of(id).map(|e| ecs.is_using_item(e))).unwrap_or(false)
    }
    pub fn start_using(&mut self, id: u32, off_hand: bool) -> bool {
        let Some(ecs) = self.ecs_mut() else {
            return false;
        };
        match ecs.entity_of(id) {
            Some(e) => ecs.start_using(e, off_hand),
            None => false,
        }
    }
    pub fn using(&self, id: u32) -> Option<(bool, i32)> {
        let ecs = self.ecs()?;
        ecs.using(ecs.entity_of(id)?)
    }
    pub fn set_using(&mut self, id: u32, counter: i32) {
        if let Some(ecs) = self.ecs_mut() {
            if let Some(e) = ecs.entity_of(id) {
                ecs.set_using(e, counter);
            }
        }
    }
    pub fn stop_using(&mut self, id: u32) {
        if let Some(ecs) = self.ecs_mut() {
            if let Some(e) = ecs.entity_of(id) {
                ecs.stop_using(e);
            }
        }
    }
    pub fn release_using_item(&mut self, id: u32) {
        if let Some(ecs) = self.ecs_mut() {
            if let Some(e) = ecs.entity_of(id) {
                ecs.release_using_item(e);
            }
        }
    }
    pub fn item_use_remaining(&self, id: u32) -> Option<i32> {
        let ecs = self.ecs()?;
        ecs.item_use_remaining(ecs.entity_of(id)?)
    }
    pub fn set_item_use_remaining(&mut self, id: u32, ticks: i32) {
        self.set_using(id, ticks);
    }
    pub fn using_off_hand(&self, id: u32) -> Option<bool> {
        let ecs = self.ecs()?;
        ecs.using_off_hand(ecs.entity_of(id)?)
    }
    pub fn ignite(&mut self, id: u32, ticks: i32) {
        if let Some(ecs) = self.ecs_mut() {
            if let Some(e) = ecs.entity_of(id) {
                ecs.ignite(e, ticks);
            }
        }
    }
    pub fn fire_immune(&self, id: u32) -> bool {
        self.ecs().and_then(|ecs| ecs.entity_of(id).map(|e| ecs.fire_immune(e))).unwrap_or(false)
    }
    pub fn is_roaring(&self, id: u32) -> bool {
        self.ecs().and_then(|ecs| ecs.entity_of(id).map(|e| ecs.is_roaring(e))).unwrap_or(false)
    }
    pub fn try_totem(&mut self, id: u32) -> bool {
        let Some(ecs) = self.ecs_mut() else {
            return false;
        };
        match ecs.entity_of(id) {
            Some(e) => ecs.try_totem(e),
            None => false,
        }
    }
}
