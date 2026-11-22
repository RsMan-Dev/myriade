//! The armadillo's scute wait is saved under the jar's `scute_time` (the ticks left) and a load gives it back as the due tick.
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::{cow::*, gift_drop::GiftDue, nbt};

#[test]
fn the_scute_wait_is_saved_and_loaded() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:armadillo").expect("the armadillo");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    let saved = nbt::save(&h.world, e).expect("an NBT");
    let Some(Tag::Int(left)) = saved.get(&JavaString::from("scute_time")) else { panic!("scute_time is saved") };
    assert!((6000..12000).contains(left), "the first wait is 6000 + nextInt(6000), got {left}");

    let f = spawn_species(&mut h.world, &sp, 4.5, 4.5, 62);
    let mut c = saved.clone();
    c.insert("scute_time".into(), Tag::Int(777));
    nbt::load(&mut h.world, f, &c);
    let now = now_of(&h.world, f);
    assert_eq!(h.world.get::<GiftDue>(f).map(|d| d.0), Some(now + 777));
}
