//! The minecarts on rails by network id (lot 57): the host's track reads, the push of a cart and its velocity, through the bridge's
//! `EcsWorld` (bridge/cart_move.rs). An inert level answers as an empty world.
use super::level::EcsLevel;
use crate::mobs::MobWorld;
use horizon_ecs_proto::cart_move::TrackRead;
use horizon_ecs_proto::minecart::Rail;
use horizon_gameplay::minecarts::Shape;

const RAILS: [&str; 4] = ["minecraft:rail", "minecraft:powered_rail", "minecraft:detector_rail", "minecraft:activator_rail"];

/// The host's reads of the track (`BaseRailBlock.isRail`, the shape of the rail, `isRedstoneConductor`, the activator rail, the container signal).
pub fn track_read() -> TrackRead {
    fn rail(mw: &MobWorld<'static>, p: [i32; 3]) -> Option<Rail> {
        let state = mw.world.block(p[0], p[1], p[2]);
        let name = &*mw.facts.get(state).name;
        if !RAILS.contains(&name) {
            return None;
        }
        let props = &mw.registry.state(state)?.properties;
        let (a, b) = Shape::parse(props.get("shape").map_or("", String::as_str)).exits();
        let powered = (name == "minecraft:powered_rail").then(|| props.get("powered").is_some_and(|v| v == "true"));
        Some(Rail { exits: [a, b], powered })
    }
    fn conductor(mw: &MobWorld<'static>, p: [i32; 3]) -> bool {
        let f = mw.facts.get(mw.world.block(p[0], p[1], p[2]));
        f.solid_render && f.full_collision
    }
    fn activator(mw: &MobWorld<'static>, p: [i32; 3]) -> bool {
        &*mw.facts.get(mw.world.block(p[0], p[1], p[2])).name == "minecraft:activator_rail"
    }
    fn signal(mw: &MobWorld<'static>, slots: &[Option<myriade_engine::items::ItemStack>]) -> i32 {
        let fractions: Vec<f32> = slots.iter().flatten().map(|s| s.count as f32 / horizon_ecs_proto::rules::item::max_stack(mw, s) as f32).collect();
        horizon_ecs_proto::minecart::container_signal(&fractions, slots.len())
    }
    TrackRead { rail, conductor, activator, signal }
}

impl EcsLevel {
    /// Lends the host's track reads to the world (once, when the level opens).
    pub fn set_track_read(&mut self, read: TrackRead) {
        if let Some(b) = self.ecs_mut() {
            b.set_track_read(read);
        }
    }
    /// Adds `impulse` to the velocity of the cart `id` and wakes it (`EcsWorld::cart_push`).
    pub fn cart_push(&mut self, id: u32, impulse: [f64; 3]) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(b) = self.ecs_mut() {
            b.cart_push(e, impulse);
        }
    }
    /// A rail at `pos` changed or got its power: the resting carts near it wake (`EcsWorld::rail_changed`).
    pub fn rail_changed(&mut self, pos: [i32; 3]) {
        if let Some(b) = self.ecs_mut() {
            b.rail_changed(pos);
        }
    }
    /// The velocity of the cart `id`, None when it is not a cart.
    pub fn cart_velocity(&self, id: u32) -> Option<[f64; 3]> {
        self.entity_of(id).zip(self.ecs()).and_then(|(e, b)| b.cart_velocity(e))
    }
}
