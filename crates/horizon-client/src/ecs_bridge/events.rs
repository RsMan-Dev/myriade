//! The ECS host events as the server's `MobEvent`s, which `handle_mob_events_reusable` applies with the effect the mobs' own events have.
//! `None`: no `MobEvent` of the same meaning, or an entity without a network id or a position; the caller keeps the event.
use crate::mobs::MobEvent;
use bevy_ecs::world::World;
use horizon_ecs_proto::bridge::{ids::id_of, view};
use horizon_ecs_proto::host_out::HostEvent;

pub fn to_mob_event(world: &World, ev: HostEvent) -> Option<MobEvent> {
    Some(match ev {
        HostEvent::SetBlock { pos, state } => MobEvent::SetBlock { pos, state },
        HostEvent::Sound { name, at, volume, pitch } => MobEvent::Sound(name, at, volume, pitch),
        HostEvent::Particles { kind, at, spread, speed, count } => MobEvent::LevelParticles { kind, at, spread, speed, count },
        HostEvent::EntityEvent { entity, code } => MobEvent::EntityEvent { mob: id_of(world, entity)?, event: code as i8, at: view::position(world, entity)? },
        HostEvent::Darkness { at, radius } => MobEvent::Darkness { at, radius },
        HostEvent::Xp { at, amount } => MobEvent::Experience { at, amount },
        HostEvent::LevelEvent { event, at, data } => MobEvent::LevelEvent { id: event, pos: at, data },
        HostEvent::ChestOpen { chest, open, entity } => MobEvent::ChestOpen { chest, open, mob: id_of(world, entity)? },
        HostEvent::ChestSlot { chest, slot, stack } => MobEvent::ChestSlot { chest, slot, stack },
        HostEvent::ParrotToShoulder { parrot, .. } => MobEvent::ParrotToShoulder { mob: id_of(world, parrot)? },
        _ => return None,
    })
}

/// Hands the host events that have a `MobEvent` to `run` (the server's `handle_mob_events_reusable`, called by the game with
/// its own access), and returns the others, unchanged, for the caller to keep.
pub(crate) fn apply(
    world: &World,
    evs: Vec<HostEvent>,
    mut run: impl FnMut(&mut Vec<MobEvent>) -> horizon_client::Result<()>,
) -> horizon_client::Result<Vec<HostEvent>> {
    let mut mob = Vec::new();
    let mut kept = Vec::new();
    for ev in evs {
        match to_mob_event(world, ev.clone()) {
            Some(m) => mob.push(m),
            None => kept.push(ev),
        }
    }
    run(&mut mob)?;
    Ok(kept)
}
