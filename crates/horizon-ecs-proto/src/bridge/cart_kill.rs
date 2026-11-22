//! The death of a cart or a boat by command (`/kill`) through the bridge (`cart_kill.rs`). Vanilla removes it as `KILLED`: its riders get
//! off (`Entity.setRemoved`), the slots of a chest or hopper cart and of a chest boat fall (`AbstractMinecartContainer.remove`,
//! `AbstractChestBoat.remove`, `Containers.dropContents`), a leashed boat drops its lead (`AbstractBoat.remove`). The drops are item entities
//! of the ECS; nothing comes back to the host.
use crate::bridge::host::EcsWorld;
use crate::boat_state::{BoatMarker, Chest};
use crate::cart_state::{Cart, Contents};
use crate::cow::{with_level_random, Pos};
use crate::item::spawn_at_location;
use crate::leash;
use crate::loot::LevelRandom;
use crate::minecart::{Fuse, Kind};
use crate::seat;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

impl EcsWorld {
    /// `Entity.kill` of the cart or boat `e`: false when `e` is no vehicle. The host reads the players of `e` (`players_of`) before the call.
    pub fn kill_vehicle(&mut self, mw: &MobWorld<'_>, e: Entity) -> bool {
        self.lent(mw, |w| kill(w, e))
    }
    /// The fuse of the TNT cart `e`: -1 when it is not primed; None for another kind of cart or for no cart.
    pub fn cart_fuse(&self, e: Entity) -> Option<i32> {
        fuse_of(self.world(), e)
    }
}

fn fuse_of(w: &World, e: Entity) -> Option<i32> {
    if w.get::<Kind>(e).copied() != Some(Kind::Tnt) {
        return None;
    }
    w.get::<Fuse>(e).map(|f| f.0)
}

/// `remove(KILLED)`: the riders get off, the stacks of the chest fall in slot order, then the lead of a leashed boat, and the entity is gone.
fn kill(w: &mut World, e: Entity) -> bool {
    if w.get::<Cart>(e).is_none() && w.get::<BoatMarker>(e).is_none() {
        return false;
    }
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    seat::eject_all(w, e);
    let slots = w.get::<Contents>(e).map(|k| k.slots.clone()).or_else(|| w.get::<Chest>(e).map(|c| c.slots.clone())).unwrap_or_default();
    let mut drops: Vec<ItemStack> = slots.into_iter().flatten().collect();
    if leash::detach(w, e).is_some() {
        drops.push(ItemStack::new("minecraft:lead", 1));
    }
    with_level_random(w, |_| ());
    let mut rng = w.remove_resource::<LevelRandom>().expect("made by with_level_random");
    for stack in drops {
        spawn_at_location(w, stack, at, &mut rng.0);
    }
    w.insert_resource(rng);
    w.entity_mut(e).despawn();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::cart_state;
    use crate::cart_variants::ignite_cart;
    use crate::cow::Herd;
    use crate::minecart::FUSE;

    fn world() -> Herd {
        let mut h = Herd::new(0, 1.0, true, false, 1);
        crate::item::insert_resources(&mut h.world);
        h
    }

    fn item_count(w: &mut World) -> usize {
        w.query::<&Item>().iter(w).count()
    }

    #[test]
    fn a_killed_chest_cart_leaves_its_stacks_as_items_and_is_removed() {
        let mut h = world();
        let e = cart_state::spawn(&mut h.world, Kind::Chest, [0.5, 1.0, 0.5], 0.0);
        let mut k = h.world.get_mut::<Contents>(e).expect("a chest cart has contents");
        k.slots[0] = Some(ItemStack::new("minecraft:stone", 3));
        k.slots[5] = Some(ItemStack::new("minecraft:coal", 2));
        drop(k);
        assert!(kill(&mut h.world, e));
        assert!(h.world.get::<Cart>(e).is_none());
        assert_eq!(item_count(&mut h.world), 2);
    }

    #[test]
    fn the_fuse_of_a_tnt_cart_reads_minus_one_then_its_primed_value() {
        let mut h = world();
        let e = cart_state::spawn(&mut h.world, Kind::Tnt, [0.5, 1.0, 0.5], 0.0);
        assert_eq!(fuse_of(&h.world, e), Some(-1));
        ignite_cart(&mut h.world, e, None);
        assert_eq!(fuse_of(&h.world, e), Some(FUSE));
    }
}
