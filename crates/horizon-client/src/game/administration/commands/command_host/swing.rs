use super::entity::{entity_ref, target_of};
use super::super::Target;
use crate::game::Game;
use horizon_gameplay::commands::{
    entity::EntityRef,
    host::R,
    swing::SwingHost,
};

impl SwingHost for Game<'_> {
    fn implied_entity(&self) -> R<EntityRef> {
        Ok(entity_ref(self.implied(false)?))
    }
    fn swing_entity(&mut self, entity: EntityRef) -> bool {
        match target_of(entity) {
            Target::Player => self.swing_arm(),
            Target::Mob(id) => {
                // `LivingEntity.swing(MAIN_HAND)`: the clients hear it through `HostEvent::Swing`.
                self.ecs.swing(id, false);
            }
            // An armor stand is a LivingEntity: it swings (invisibly).
            Target::Stand(_) => {}
            _ => return false,
        }
        true
    }
}
