//! Règle du coup sans colère, reprise de `goat_ram_tests.rs` (valeurs inchangées) : un coup de bélier ne nomme aucun attaquant
//! (`DamageTypeTags.NO_ANGER`, `LivingEntity.resolveMobResponsibleForDamage`), un coup de mob nomme le sien dans l'événement de dégât.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::host_out::HostEvent;

/// The attacker the host hears in the `Hurt` of a hit that `add` queues on a cow, one tick on; None without such an event.
fn heard(add: impl FnOnce(&mut DamageQueue, Entity, Entity)) -> Option<Option<Entity>> {
    let mut h = Herd::with_slab_awake(60, false, false);
    let victim = ecs_cow(&mut h, 0.5, 1.0, 0.5, 1);
    let attacker = ecs_cow(&mut h, 3.5, 1.0, 0.5, 2);
    add(&mut h.world.resource_mut::<DamageQueue>(), victim, attacker);
    h.tick();
    h.drain_host_out().into_iter().find_map(|ev| if let HostEvent::Hurt { attacker, .. } = ev { Some(attacker) } else { None })
}

#[test]
fn a_ram_names_no_attacker_and_a_mob_attack_names_its_own() {
    if !have_data() {
        return;
    }
    let ram = heard(|q, victim, _| q.add_ram(victim, 2.0, (3.5, 0.5), 0.4));
    assert_eq!(ram, Some(None), "a ram's hurt names nobody");
    let blow = heard(|q, victim, attacker| q.add_by_entity(victim, 2.0, (3.5, 0.5), attacker));
    assert!(matches!(blow, Some(Some(_))), "a mob_attack names its attacker: {blow:?}");
}
