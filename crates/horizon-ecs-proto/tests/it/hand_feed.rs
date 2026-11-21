//! `Panda.mobInteract` with bamboo: a baby ages up by a tenth of what it has left; an adult already in love (the breed refuses it) sits, begins to eat
//! and holds one bamboo; the end of the eating then comes by the wheel (sound steps, the animal keeps sitting for a bamboo). Expected values from the jar.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::breed::Love;
use horizon_ecs_proto::behaviours::panda_state::{PandaState, FLAG_SIT};
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items;

fn panda(h: &mut Herd) -> Option<Entity> {
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:panda")?;
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    Some(e)
}

#[test]
fn bamboo_ages_a_baby_up_by_a_tenth_of_its_time_left() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = panda(&mut h) else { return };
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(GrowAt(now + 24_000));
    let bamboo = items::item_id("minecraft:bamboo");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: bamboo, off: 0, ..Default::default() });
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: bamboo }));
    // (24000 / 20) * 0.1 = 120 seconds = 2400 ticks.
    assert_eq!(h.world.get::<GrowAt>(e).unwrap().0, now + 24_000 - 2_400);
}

#[test]
fn bamboo_makes_an_adult_that_cannot_fall_in_love_sit_and_eat() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = panda(&mut h) else { return };
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(Love(now + 600));
    let bamboo = items::item_id("minecraft:bamboo");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: bamboo, off: 0, ..Default::default() });
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: bamboo }));
    assert!(h.world.get::<PandaState>(e).unwrap().flags & FLAG_SIT != 0, "it sits down");
    assert_eq!(equipped(&h.world, e, Slot::Main), bamboo, "it holds one bamboo");
    assert!(h.world.get::<PandaState>(e).unwrap().eat > 0, "it is eating");
}
