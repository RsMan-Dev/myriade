//! The host's calls for acts of a creature that live in P: `Mob.ate`, `LivingEntity.swing`, and the meal of the item in the mouth
//! (`behaviours::eat_held`) that the host completes after running the item's effects.
use crate::behaviours::eat_held;
use crate::bridge::{host::EcsWorld, view};
use crate::cow::Pos;
use bevy_ecs::entity::Entity;
use myriade_engine_lib::items::ItemStack;

impl EcsWorld {
    /// `Mob.ate` (the wool grows back, a baby grows by a minute). The game event `eat` is the caller's (`HostOut` has none for it here).
    pub fn ate(&mut self, e: Entity) {
        let w = self.world_mut();
        crate::behaviours::eat_block::ate(w, e);
        view::mark(w, e);
    }

    /// `LivingEntity.swing(hand)`: true when the swing started (`SwingState.startIfAble`); the host hears `HostEvent::Swing`.
    pub fn swing(&mut self, e: Entity, off_hand: bool) -> bool {
        crate::swing::swing(self.world_mut(), e, off_hand)
    }

    /// The creature lives, its mouth still holds `stack`, and (when given) it still stands at `at`: the meal asked by
    /// `HostEvent::HeldConsume` may go on.
    pub fn held_use_valid(&self, e: Entity, stack: &ItemStack, at: Option<[f64; 3]>) -> bool {
        let w = self.world();
        eat_held::still_holds(w, e, stack) && at.is_none_or(|a| w.get::<Pos>(e).is_some_and(|p| [p.0.x, p.0.y, p.0.z] == a))
    }

    /// The end of the meal: one of the stack is eaten or the use `remainder` replaces it, and the clock of the last meal restarts.
    /// False when the mouth no longer holds `stack`.
    pub fn finish_held_use(&mut self, e: Entity, stack: &ItemStack, remainder: Option<ItemStack>) -> bool {
        let w = self.world_mut();
        let done = eat_held::finish(w, e, stack, remainder);
        if done {
            view::mark(w, e);
        }
        done
    }
    /// `Mob.playAmbientSound`: the sound is queued at the creature's place; false without one.
    pub fn play_ambient(&mut self, e: Entity) -> bool {
        crate::behaviours::ambient_sound::play(self.world_mut(), e)
    }
}
