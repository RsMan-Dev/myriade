//! `Mob.baseTick` ambient sounds: the delay after a sound is `nextInt(1000) + interval + 2` ticks (the counter restarts at `-interval`), and a
//! tame wolf under 20 health whines or barks, never pants, while a wolf at 20 health never whines (`Wolf.getAmbientSound`).
use horizon_ecs_proto::cow::{spawn_species, ActiveUntil, Health, Herd, SpeciesTable};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::rules::ambient_sound::vanilla_delay;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;

#[test]
fn the_ambient_delay_is_the_draw_plus_the_interval_plus_two() {
    assert_eq!(vanilla_delay(80, 0), 82);
    assert_eq!(vanilla_delay(80, 999), 1081);
}

fn wolf_sounds(h: &mut Herd) -> Vec<String> {
    h.world
        .resource_mut::<Events>()
        .host
        .drain()
        .filter_map(|e| match e {
            HostEvent::Sound { name, .. } if name.starts_with("entity.wolf.") => Some(name.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_tame_wolf_under_twenty_health_whines_or_barks_and_never_pants() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let wolf = *table.iter().find(|s| s.name == "minecraft:wolf").unwrap();
    let e = spawn_species(&mut h.world, &wolf, 0.5, 0.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.entity_mut(e).insert((Owner(Some(Who::Player(1))), Health(19.0)));
    let mut heard = Vec::new();
    for _ in 0..2400 {
        h.tick();
        heard.extend(wolf_sounds(&mut h));
    }
    assert!(!heard.is_empty(), "a wolf makes its ambient sound over 2400 ticks");
    assert!(heard.iter().all(|s| s != "entity.wolf.pant"), "a tame wolf under 20 health never pants: {heard:?}");
    assert!(heard.iter().any(|s| s == "entity.wolf.whine"), "a tame wolf under 20 health whines");
}
