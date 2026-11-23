//! `makeMad`: a horse rears and says its angry sound once (nothing while it already rears), a skeleton horse rears silently.
use horizon_ecs_proto::behaviours::random_stand::Standing;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::make_mad::make_mad;

fn sounds(h: &mut Herd) -> Vec<std::borrow::Cow<'static, str>> {
    h.world.resource_mut::<Events>().host.drain().filter_map(|e| if let HostEvent::Sound { name, .. } = e { Some(name) } else { None }).collect()
}

#[test]
fn a_horse_rears_and_neighs_angrily_once() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = species_table().0;
    let horse = table.iter().find(|s| s.name == "minecraft:horse").expect("horse");
    let skeleton = table.iter().find(|s| s.name == "minecraft:skeleton_horse").expect("skeleton horse");
    let e = spawn_species(&mut h.world, horse, 0.5, 0.5, 5);
    let _ = sounds(&mut h);
    make_mad(&mut h.world, e);
    assert!(h.world.get::<Standing>(e).is_some(), "it rears");
    let said = sounds(&mut h);
    assert!(said.iter().any(|n| n.ends_with("horse.angry")), "{said:?}");
    make_mad(&mut h.world, e);
    assert!(sounds(&mut h).is_empty(), "already rearing: nothing more");
    let s = spawn_species(&mut h.world, skeleton, 4.5, 0.5, 6);
    let _ = sounds(&mut h);
    make_mad(&mut h.world, s);
    assert!(sounds(&mut h).is_empty(), "the skeleton horse has no angry sound");
}
