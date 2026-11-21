//! A listener of game events (`Allay.VibrationUser`, `Allay.JukeboxListener`, `AllayAi.hearNoteblock`): the host posts an event
//! (`hear`), the entities of the box around it that carry `Ear` are found by the spatial index (never by a walk of the entities), and
//! each one acts by its own `Params`. A note event travels at a block a tick to the eyes, then is liked (a position and a deadline);
//! a start event of a jukebox starts a dance, its stop event ends it, and a check every 20 ticks ends a dance whose jukebox is gone.
//! Rare state is sparse (`Pending`, `Liked`, `Jukebox`, `Dancing`): an entity that hears nothing costs nothing. No goal, no system.
//! Left out: the occlusion of a vibration by wool (the host has no line in the frame), the vibration particle, and the dimension of the
//! liked block (a world of P holds one dimension).
use super::{Inst, Module, params};
use crate::cow::{after, now_of, Index, Pos};
use crate::dragon::flight::NoAi;
use crate::targeting::Eye;
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The game event that is liked, its listening radius in blocks, and the block it comes from.
    pub note_event: &'static str,
    pub note_radius: i32,
    pub note_block: &'static str,
    /// Ticks the liked block stays liked after the last note heard from it.
    pub forget: u64,
    /// A liked block farther than this (chessboard distance) is not listened to.
    pub liked_reach: i32,
    /// The start and stop events of a dance, the radius they are heard in and the block that keeps the dance going.
    pub play_event: &'static str,
    pub stop_event: &'static str,
    pub dance_radius: i32,
    pub dance_block: &'static str,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Ear(pub &'static Params);

/// A note on its way to the eyes: the block it was played at (one at a time, as `VibrationSystem` keeps one).
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Pending([i32; 3]);

/// `LIKED_NOTEBLOCK_POSITION` and its cooldown: the block, and the own time its 600 ticks end.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Liked {
    pub pos: [i32; 3],
    pub until: u64,
}

/// `Allay.jukeboxPos`: the jukebox the dance started from.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Jukebox(pub [i32; 3]);

/// `Allay.DATA_DANCING` is true; the own time of the next check of the jukebox.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Dancing(pub u64);

pub static MODULE: Module = Module { name: "event_listen", flags: 0, equip, requires: &[crate::behaviours::Cap::Transform], ..Module::DEFAULT };

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Ear(params::<Params>(p)));
}

fn block_of(p: [f64; 3]) -> [i32; 3] {
    p.map(|c| c.floor() as i32)
}

fn dist_sqr(a: [i32; 3], b: [i32; 3]) -> i64 {
    (0..3).map(|i| i64::from(a[i] - b[i]).pow(2)).sum()
}

/// A game event of the host at `at`: each listener within its radius of it (the eyes' block against the event's block) takes it.
pub fn hear(w: &mut World, event: &str, at: [f64; 3]) {
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(at[0], at[2], 17.0, |e| near.push(e));
    let source = block_of(at);
    for e in near {
        let (Some(&Ear(p)), Some(pos), Some(eye)) = (w.get::<Ear>(e), w.get::<Pos>(e).map(|p| p.0), w.get::<Eye>(e).map(|x| x.0)) else { continue };
        let eyes = [pos.x, pos.y + eye, pos.z];
        let d2 = dist_sqr(source, block_of(eyes));
        if event == p.note_event {
            if d2 <= i64::from(p.note_radius).pow(2) && w.get::<Pending>(e).is_none() && can_receive(w, e, &p, source) {
                let d = [at[0] - eyes[0], at[1] - eyes[1], at[2] - eyes[2]];
                let travel = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().floor() as i32;
                w.entity_mut(e).insert(Pending(source));
                after(w, e, travel, arrive);
            }
        } else if (event == p.play_event || event == p.stop_event) && d2 <= i64::from(p.dance_radius).pow(2) {
            set_jukebox_playing(w, e, &p, source, event == p.play_event);
        }
    }
}

/// `Allay.VibrationUser.canReceiveVibration`: not without AI; with no liked block any note, else only that block's while it is near.
fn can_receive(w: &World, e: Entity, p: &Params, pos: [i32; 3]) -> bool {
    if w.get::<NoAi>(e).is_some() {
        return false;
    }
    let Some(liked) = w.get::<Liked>(e) else { return true };
    let Some(at) = w.get::<Pos>(e).map(|x| block_of([x.0.x, x.0.y, x.0.z])) else { return false };
    let chessboard = (0..3).map(|i| (liked.pos[i] - at[i]).abs()).max().unwrap_or(0);
    chessboard <= p.liked_reach && liked.pos == pos
}

/// The wheel's call: the note reached the eyes; `AllayAi.hearNoteblock` likes its block (or starts its wait over).
fn arrive(w: &mut World, e: Entity) {
    let Some(Pending(pos)) = w.entity_mut(e).take::<Pending>() else { return };
    let Some(&Ear(p)) = w.get::<Ear>(e) else { return };
    let until = now_of(w, e) + p.forget;
    match w.get::<Liked>(e).copied() {
        None => {
            w.entity_mut(e).insert(Liked { pos, until });
        }
        Some(l) if l.pos == pos => w.get_mut::<Liked>(e).unwrap().until = until,
        Some(_) => {}
    }
}

/// `shouldDepositItemsAtLikedNoteblock` and `getItemDepositPosition`: the point above the liked block while it is near, still the
/// block of the species and its wait runs; else the memory is erased.
pub fn liked_deposit(w: &mut World, e: Entity) -> Option<[f64; 3]> {
    let &Ear(p) = w.get::<Ear>(e)?;
    let l = *w.get::<Liked>(e)?;
    let at = w.get::<Pos>(e).map(|x| block_of([x.0.x, x.0.y, x.0.z]))?;
    let near = (0..3).map(|i| (l.pos[i] - at[i]).abs()).max().unwrap_or(0) <= p.liked_reach;
    if now_of(w, e) < l.until && near && crate::level::block_name(w, l.pos) == p.note_block {
        return Some([f64::from(l.pos[0]) + 0.5, f64::from(l.pos[1]) + 1.5, f64::from(l.pos[2]) + 0.5]);
    }
    w.entity_mut(e).remove::<Liked>();
    None
}

/// `Allay.setDancing`: not without AI, and a panicking one does not start.
fn set_dancing(w: &mut World, e: Entity, on: bool) {
    if w.get::<NoAi>(e).is_some() || on && w.get::<super::animal_panic::AnimalPanicking>(e).is_some() {
        return;
    }
    if on {
        let at = now_of(w, e) + 20;
        w.entity_mut(e).insert(Dancing(at));
        after(w, e, 20, dance_check);
    } else {
        w.entity_mut(e).remove::<Dancing>();
    }
}

/// `Allay.setJukeboxPlaying`: a start keeps that jukebox while not dancing yet; a stop ends the dance from the kept one or with none kept.
fn set_jukebox_playing(w: &mut World, e: Entity, _: &Params, jukebox: [i32; 3], playing: bool) {
    if playing {
        if w.get::<Dancing>(e).is_none() {
            w.entity_mut(e).insert(Jukebox(jukebox));
            set_dancing(w, e, true);
        }
    } else if w.get::<Jukebox>(e).is_none_or(|j| j.0 == jukebox) {
        w.entity_mut(e).remove::<Jukebox>();
        set_dancing(w, e, false);
    }
}

/// `Allay.aiStep`'s check, each 20 ticks of a dance: it ends when no jukebox is kept, it is not within the radius of the allay
/// (the centre of its block) or is no jukebox any more.
fn dance_check(w: &mut World, e: Entity) {
    let Some(d) = w.get::<Dancing>(e) else { return };
    let now = now_of(w, e);
    if now < d.0 {
        return;
    }
    let (Some(&Ear(p)), Some(pos)) = (w.get::<Ear>(e), w.get::<Pos>(e).map(|x| x.0)) else { return };
    let stays = w.get::<Jukebox>(e).is_some_and(|j| {
        let c = j.0.map(|v| f64::from(v) + 0.5);
        let d = [c[0] - pos.x, c[1] - pos.y, c[2] - pos.z];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < f64::from(p.dance_radius).powi(2) && crate::level::block_name(w, j.0) == p.dance_block
    });
    if !stays {
        set_dancing(w, e, false);
        w.entity_mut(e).remove::<Jukebox>();
    }
    if w.get::<Dancing>(e).is_some() {
        w.get_mut::<Dancing>(e).unwrap().0 = now + 20;
        after(w, e, 20, dance_check);
    }
}

/// `Allay.tick`: a panicking allay stops dancing (called when its panic starts).
pub fn panic_started(w: &mut World, e: Entity) {
    if w.get::<Dancing>(e).is_some() {
        set_dancing(w, e, false);
    }
}
