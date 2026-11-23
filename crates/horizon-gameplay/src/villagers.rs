//! What villagers do with each other and with the player: gossip and
//! reputation, sharing food and wares, picking up dropped items, making
//! babies, calling iron golems and the prices they ask.
use crate::{item_areas::ItemAreas, mob_level::MobLevel, villager_level::VillagerLevel};
use myriade_engine::{
    gossip::{self, GossipType},
    items::{ItemStack, Items},
    mobs::{Species, spawning, villager, villager_brain},
};
use horizon_core::{geometry::Vec3, position::BlockPos};

fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}


pub fn item_max_stack(items: &Items, item: &str) -> u32 {
    items.get(item).map_or(64, |i| i.max_stack)
}


/// `WorkAtComposter.useWorkstation`: at a composter the farmer makes
/// bread, then composts seeds.
pub fn villager_work_at_composter<L: VillagerLevel + ?Sized>(l: &mut L, id: u32, pos: [i32; 3]) {
    let Some(i) = l.villager_index(id) else {
        return;
    };
    if l.composter_level(pos).is_none() {
        return;
    }
    l.villager_make_bread(i);
    l.compost_items(i, pos);
}


/// `BehaviorUtils.throwItem`: toss a stack from a villager's hand toward
/// a point.
pub fn villager_throw<L: VillagerLevel + ?Sized>(
    l: &mut L,
    from: Vec3,
    eye: f64,
    to: Vec3,
    stack: ItemStack,
) {
    // The hand drop and the velocity are the float 0.3F, widened.
    let k = f64::from(0.3_f32);
    let at = [from.x, eye - k, from.z];
    let d = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z).normalize();
    l.spawn_item(stack, at, [d.x * k, d.y * k, d.z * k], 10);
}










/// `Villager.getPlayerReputation` of the local player.
pub fn villager_reputation<L: VillagerLevel + ?Sized>(l: &L, id: u32) -> i32 {
    let Some(uuid) = l.host_uuid() else {
        return 0;
    };
    l.villager_reputation_of(id, uuid)
}


