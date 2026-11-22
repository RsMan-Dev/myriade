//! The End fight's tick (`tick_fight` D:566, `fight_has_players` D:1329, `find_or_create_dragon`): the players are scanned every
//! `PLAYER_SCAN` ticks, the spikes are placed once, a dragon is found or made, and a saved dragon unseen for 1200 ticks is replaced.
//! `drain_out` moves the fight's events to the host once per step.
use crate::behaviours::elder_curse::Silent;
use crate::bridge::uuids::{entity_by_uuid, to_words, EntityUuid};
use crate::dragon::crystals;
use crate::dragon::fight::{self, EndFight, FightEvents, PLAYER_SCAN};
use crate::dragon::flight::DragonRng;
use crate::dragon::respawn;
use crate::host_out_fight::{FightEvent, FightOut};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_world::worldgen::end_spike::spikes_for_seed;

/// A saved dragon not seen for this many ticks is replaced (`dragon_unseen`).
const UNSEEN_LIMIT: u32 = 1200;
/// The players of the fight are those within this reach of the origin.
const REACH: f64 = 192.0;

/// The fight's tick, once its arena is loaded. Nothing runs before `fight_load`'s state is there.
pub fn tick(w: &mut World, arena_ready: bool) {
    if !arena_ready {
        return;
    }
    w.init_resource::<EndFight>();
    w.init_resource::<FightEvents>();
    w.init_resource::<FightOut>();
    scan_players(w);
    place_spikes(w);
    find_or_create(w);
    bar(w);
}

fn push_out(w: &mut World, ev: FightEvent) {
    if let Some(mut out) = w.get_resource_mut::<FightOut>() {
        out.0.push(ev);
    }
}

/// `fight_has_players`: the scan runs when the countdown ends or the save asked for it; the change is an event.
fn scan_players(w: &mut World) {
    let due = {
        let f = w.resource_mut::<EndFight>().into_inner();
        if f.needs_scan || f.player_scan == 0 {
            f.needs_scan = false;
            f.player_scan = PLAYER_SCAN;
            true
        } else {
            f.player_scan -= 1;
            false
        }
    };
    if due {
        let present = fight::nearest_enemy(w, Vec3::ZERO, Some(REACH), None).is_some();
        fight::set_players(w, present);
    }
}

/// The spikes' crystals are placed once, on a fight that is not over (`place_spike_crystals`).
fn place_spikes(w: &mut World) {
    let todo = {
        let f = w.resource_mut::<EndFight>().into_inner();
        let todo = !f.crystals_placed && !f.killed;
        if todo {
            f.crystals_placed = true;
            f.dirty = true;
        }
        todo
    };
    if todo {
        let seed = crate::blocks::world_of(w).world_seed;
        for spike in spikes_for_seed(seed).iter() {
            crystals::place_spike(w, spike);
        }
    }
}

/// `find_or_create_dragon`: the fight's dragon is the one it has, else the saved one when it lives, else a new one. A dragon that
/// is dying still counts as there (its death ends it); after the kill, or during a respawn, no dragon is made.
fn find_or_create(w: &mut World) {
    let (listed, saved, killed, respawning) = {
        let f = w.resource::<EndFight>();
        (f.dragon.filter(|&d| w.get_entity(d).is_ok()), f.dragon_uuid, f.killed, f.respawn.is_some())
    };
    let found = match (listed, saved) {
        (Some(d), _) => Some(d),
        (None, Some(u)) => entity_by_uuid(w, to_words(u)).filter(|&e| w.get::<DragonRng>(e).is_some()),
        (None, None) => None,
    };
    if let Some(d) = found {
        let uuid = w.get::<EntityUuid>(d).map(|u| u.0);
        let silent = w.get::<Silent>(d).is_some();
        let f = w.resource_mut::<EndFight>().into_inner();
        f.dragon = Some(d);
        f.dragon_unseen = 0;
        f.silent = silent;
        if uuid.is_some() && uuid != f.dragon_uuid {
            f.dragon_uuid = uuid;
            f.dirty = true;
        }
        return;
    }
    let wait = {
        let f = w.resource_mut::<EndFight>().into_inner();
        f.dragon = None;
        if killed || respawning {
            true
        } else if f.dragon_uuid.is_some() {
            f.dragon_unseen += 1;
            f.dragon_unseen < UNSEEN_LIMIT
        } else {
            false
        }
    };
    if !wait {
        let f = w.resource_mut::<EndFight>().into_inner();
        f.dragon_uuid = None;
        f.dragon_unseen = 0;
        f.dirty = true;
        respawn::spawn_new_dragon(w);
    }
}

/// `boss_bar_remove`: the bar that the fight's dragon showed goes when the dragon does.
fn bar(w: &mut World) {
    let (was, now) = {
        let f = w.resource_mut::<EndFight>().into_inner();
        let now = f.dragon.is_some();
        let was = std::mem::replace(&mut f.bar_shown, now);
        (was, now)
    };
    if was && !now {
        push_out(w, FightEvent::BarGone);
    }
}

/// Moves the fight's events of the last tick to the host's queue, and a `Dirty` when the save must follow.
pub fn drain_out(w: &mut World) {
    let mut out = Vec::new();
    let silent = w.get_resource::<EndFight>().is_some_and(|f| f.silent);
    if let Some(ev) = w.get_resource::<FightEvents>() {
        out.extend(ev.crystal_destroyed.iter().map(|c| FightEvent::CrystalDestroyed { at: c.at, by: c.by }));
        out.extend(ev.players.iter().map(|p| FightEvent::PlayersChanged { present: p.present }));
        out.extend(ev.killed.iter().map(|k| FightEvent::Killed { by: k.by, last_blow: k.last_blow, silent }));
        out.extend(ev.credit.iter().map(|c| FightEvent::Credit { by: c.by, last_blow: c.last_blow, by_player: c.by_player }));
    }
    if let Some(mut f) = w.get_resource_mut::<EndFight>() {
        if !out.is_empty() {
            f.dirty = true;
        }
        if f.dirty {
            f.dirty = false;
            out.push(FightEvent::Dirty);
        }
    }
    if let Some(mut q) = w.get_resource_mut::<FightOut>() {
        q.0.extend(out);
    }
}
