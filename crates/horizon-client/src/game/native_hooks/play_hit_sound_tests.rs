#![cfg(test)]
use crate::game::{native_entity_refs::tests::with_game, *};
use horizon_client::{chunk::Chunk, motion::PlayerMotion};
use horizon_core::geometry::Vec3;
use mod_api::{
    BlockPart, BlockPos, ClientHost, Direction, Flow, Hooks, ModEntry, PlayHitSoundEvent, Registrar,
};
use std::sync::{Arc, Mutex};

type Hits = Arc<Mutex<Vec<(BlockPos, Direction)>>>;
struct Quiet(Hits);
#[mod_api::horizon_part]
impl BlockPart for Quiet {
    fn play_hit_sound(&self, _: &mut dyn ClientHost, event: &mut PlayHitSoundEvent) -> Flow {
        self.0.lock().unwrap().push((event.pos, event.face));
        Flow::Cancel
    }
}
struct Register(Hits);
#[mod_api::__subscriptions]
impl Hooks for Register {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let block = r.block_key("minecraft:glass");
        r.block_part(block, Quiet(self.0.clone()));
    }
}
