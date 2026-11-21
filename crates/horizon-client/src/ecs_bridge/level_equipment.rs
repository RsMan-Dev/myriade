//! The equipment stacks of the entities by network id, through the bridge (`horizon-ecs-proto` `bridge/host_equipment.rs`): the writes, the
//! wear, the changes and the sound. The read is `equipment_stack` (`level_read_living.rs`). An inert level answers as an empty world.
use super::level::EcsLevel;
use horizon_client::items::ItemStack;

impl EcsLevel {
    pub fn set_equipment_stack(&mut self, id: u32, slot: usize, stack: ItemStack) -> bool {
        let Some(ecs) = self.ecs_mut() else {
            return false;
        };
        match ecs.entity_of(id) {
            Some(e) => ecs.set_equipment_stack(e, slot, stack),
            None => false,
        }
    }
    pub fn damage_equipment(&mut self, id: u32, slot: usize, amount: i32) -> bool {
        let Some(ecs) = self.ecs_mut() else {
            return false;
        };
        match ecs.entity_of(id) {
            Some(e) => ecs.damage_equipment(e, slot, amount),
            None => false,
        }
    }
    pub fn equip_sound(&self, id: u32, slot: usize, over: Option<String>) -> Option<String> {
        let ecs = self.ecs()?;
        ecs.equip_sound(ecs.entity_of(id)?, slot, over)
    }
}
