//! The soaking count of a zombie is read from `DrownedConversionTime` and saved back as the ticks left.
use horizon_core::nbt::Tag;
use horizon_ecs_proto::{behaviours::convert_in_water::Soaking, behaviours::convert_timer::Converting, cow::*, nbt};

#[test]
fn a_conversion_in_progress_is_loaded_and_saved() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:zombie").expect("the zombie");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    let mut c = nbt::save(&h.world, e).expect("an NBT");
    c.insert("DrownedConversionTime".into(), Tag::Int(200));
    nbt::load(&mut h.world, e, &c);
    let s = h.world.get::<Soaking>(e).copied().expect("the count resumed");
    assert!(s.converting);
    assert!(h.world.get::<Converting>(e).is_some(), "the shake is synced");
    let saved = nbt::save(&h.world, e).expect("an NBT");
    assert_eq!(saved.get("DrownedConversionTime"), Some(&Tag::Int(200)));
}
