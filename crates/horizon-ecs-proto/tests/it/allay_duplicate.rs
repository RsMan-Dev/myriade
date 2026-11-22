//! `Allay.mobInteract`: an amethyst shard given to a dancing allay makes a second one; both then wait 6000 ticks.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::event_listen::Dancing;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{interact, Entered, Online, Player};
use horizon_ecs_proto::items;

#[test]
fn amethyst_makes_a_dancing_allay_duplicate_once_per_cooldown() {
    if !have_data() || wrong_species(&["minecraft:allay"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    let shard = items::item_id("minecraft:amethyst_shard");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: shard, off: 0, ..Default::default() });
    let count = |h: &mut Herd| h.world.query::<&SpeciesId>().iter(&h.world).count();
    let before = count(&mut h);
    assert!(!interact(&mut h.world, e, &Interaction { player: 1, item: shard }) || count(&mut h) == before, "not dancing: no copy");
    h.world.entity_mut(e).insert(Dancing(u64::MAX));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: shard, off: 0, ..Default::default() });
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: shard }));
    assert_eq!(count(&mut h), before + 1);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: shard, off: 0, ..Default::default() });
    interact(&mut h.world, e, &Interaction { player: 1, item: shard });
    assert_eq!(count(&mut h), before + 1, "on cooldown");
}
