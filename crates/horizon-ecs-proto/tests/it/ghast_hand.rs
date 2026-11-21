//! `Mob.attemptToShearEquipment` on the happy ghast: shears take the harness off an adult nobody rides; the food (`Animal.mobInteract`) ages a baby up.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::harness_worn::Harness;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::Entered;
use horizon_ecs_proto::items;

fn ghast(h: &mut Herd) -> Option<Entity> {
    if !have_data() || wrong_species(&["minecraft:happy_ghast"]) {
        return None;
    }
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    Some(e)
}

#[test]
fn shears_take_the_harness_off_a_happy_ghast() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = ghast(&mut h) else { return };
    let shears = items::item_id("minecraft:shears");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: shears, off: 0, ..Default::default() });
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: shears }), "no harness, nothing to shear");
    h.world.entity_mut(e).insert(Harness { item: "minecraft:white_harness" });
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: shears }));
    assert!(h.world.get::<Harness>(e).is_none());
}

#[test]
fn snowball_ages_a_baby_happy_ghast_up() {
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = ghast(&mut h) else { return };
    let ball = items::item_id("minecraft:snowball");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: ball, off: 0, ..Default::default() });
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(GrowAt(now + 24_000));
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: ball }));
    assert_eq!(h.world.get::<GrowAt>(e).unwrap().0, now + 24_000 - 2_400);
}
