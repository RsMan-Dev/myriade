//! Copper golem shears (`CopperGolem.mobInteract`, `shear`, `readyForShearing`): the poppy on the antenna (`Saddle` slot) comes off and falls;
//! a golem with no flower, or with another item there, is not sheared. Expected values come from the jar.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::items;

fn golem(h: &mut Herd) -> Option<Entity> {
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:copper_golem")?;
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    Some(e)
}

#[test]
fn shears_take_the_poppy_off_the_antenna_and_only_a_flower() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let Some(e) = golem(&mut h) else { return };
    let shears = items::item_id("minecraft:shears");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: shears, off: 0, ..Default::default() });
    let click = |h: &mut Herd| interact(&mut h.world, e, &Interaction { player: 1, item: shears });
    assert!(!click(&mut h), "no flower: nothing to shear");
    horizon_ecs_proto::equipment::set_slot(&mut h.world, e, Slot::Saddle, items::item_id("minecraft:stone"));
    assert!(!click(&mut h), "a stone on the antenna is not shearable");
    horizon_ecs_proto::equipment::set_slot(&mut h.world, e, Slot::Saddle, items::item_id("minecraft:poppy"));
    assert!(click(&mut h));
    assert_eq!(equipped(&h.world, e, Slot::Saddle), 0, "the antenna is empty");
}
