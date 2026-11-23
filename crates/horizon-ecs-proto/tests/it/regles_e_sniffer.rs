//! `Sniffer.setState`: entering a state plays its sound (`entity.sniffer.*`, jar `SoundEvents.SNIFFER_*`), and a `Silent` sniffer plays
//! none. The state is forced by writing the sniffer's own `State` component; the sound goes through the same voice gate `setState` uses.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::elder_curse::Silent;
use horizon_ecs_proto::brain::behaviours::sniff::{State, SCENTING};
use horizon_ecs_proto::cow::{spawn_species, ActiveUntil, Herd, SpeciesTable};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;

/// The sniffer sounds of the tick's events, drained from the host.
fn sniffer_sounds(h: &mut Herd) -> Vec<std::borrow::Cow<'static, str>> {
    h.world
        .resource_mut::<Events>()
        .host
        .drain()
        .filter_map(|e| match e {
            HostEvent::Sound { name, .. } if name.starts_with("entity.sniffer.") => Some(name),
            _ => None,
        })
        .collect()
}

/// A sniffer forced into `SCENTING` that enters that state, with or without `Silent`; returns the sounds it played.
fn enter_scenting(silent: bool) -> Vec<std::borrow::Cow<'static, str>> {
    let mut h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sniffer = *table.iter().find(|s| s.name == "minecraft:sniffer").unwrap();
    let e = spawn_species(&mut h.world, &sniffer, 0.5, 0.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    if silent {
        h.world.entity_mut(e).insert(Silent);
    }
    h.world.get_mut::<State>(e).unwrap().state = SCENTING;
    horizon_ecs_proto::tear_down::voice(&mut h.world, e, "entity.sniffer.scenting");
    h.tick();
    sniffer_sounds(&mut h)
}

#[test]
fn entering_a_state_plays_its_sound_unless_silent() {
    if !have_data() || wrong_species(&["minecraft:sniffer"]) {
        return;
    }
    assert_eq!(enter_scenting(false), vec!["entity.sniffer.scenting"], "a sniffer entering scenting plays its sound");
    assert!(enter_scenting(true).is_empty(), "a silent sniffer plays no state sound");
}
