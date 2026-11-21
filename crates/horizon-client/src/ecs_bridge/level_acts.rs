//! The acts of a creature that live in P, by network id: `Mob.ate`, `LivingEntity.swing`, and the end of the meal of the mouth item
//! (`HostEvent::HeldConsume`). An inert level takes nothing: false.
use super::level::EcsLevel;
use crate::items::ItemStack;
use crate::mobs::MobEvent;

impl EcsLevel {
    /// `Mob.ate`: the game event `eat` (told in `events`), then P's part (the wool grows back, a baby grows).
    pub fn ate(&mut self, id: u32, events: &mut Vec<MobEvent>) {
        let Some(e) = self.entity_of(id) else { return };
        let Some(at) = self.position(id) else { return };
        events.push(MobEvent::GameEvent { event: "minecraft:eat", at, source: id, state: horizon_world::registry::BlockStateId(0) });
        if let Some(w) = self.ecs_mut() {
            w.ate(e);
        }
    }

    /// `EcsWorld::swing`: true when the swing started; the clients hear it through `HostEvent::Swing`.
    pub fn swing(&mut self, id: u32, off_hand: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.swing(e, off_hand))
    }

    /// `EcsWorld::held_use_valid`: the creature lives, its mouth holds `stack` and (when given) it stands at `at`.
    pub fn held_use_valid(&self, id: u32, stack: &ItemStack, at: Option<[f64; 3]>) -> bool {
        let (Some(e), Some(w)) = (self.entity_of(id), self.ecs()) else { return false };
        w.held_use_valid(e, stack, at)
    }

    /// `EcsWorld::finish_held_use`: the meal is done; false when the mouth no longer holds `stack`.
    pub fn finish_held_use(&mut self, id: u32, stack: &ItemStack, remainder: Option<ItemStack>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.finish_held_use(e, stack, remainder))
    }
}
