//! The listener of game events on the allay (`event_listen`): a note heard in range is liked and forgotten at its deadline, a note out of
//! range is not heard, a jukebox starts and stops the dance. The `Mob` has no public entry for an event: the expected values are the jar's
//! (16 blocks for a note, 600 ticks, 10 blocks for a jukebox, a check every 20 ticks that a jukebox is still there).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::event_listen::{hear, liked_deposit, Dancing, Jukebox, Liked};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::Entered;

const NOTE: &str = "minecraft:note_block_play";
const PLAY: &str = "minecraft:jukebox_play";
const STOP: &str = "minecraft:jukebox_stop_play";

fn scene() -> Option<(Herd, Entity, [f64; 3])> {
    if !have_data() || wrong_species(&["minecraft:allay"]) {
        return None;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(e);
    h.tick();
    let p = h.world.get::<Pos>(e).unwrap().0;
    Some((h, e, [p.x, p.y, p.z]))
}

fn at(p: [f64; 3], dx: f64) -> [f64; 3] {
    [p[0].floor() + dx + 0.5, p[1].floor() + 0.5, p[2].floor() + 0.5]
}

#[test]
fn a_note_in_range_is_liked_and_forgotten_at_its_deadline() {
    let Some((mut h, e, p)) = scene() else { return };
    let note = at(p, 5.0);
    hear(&mut h.world, NOTE, note);
    assert!(h.world.get::<Liked>(e).is_none(), "the note travels first");
    for _ in 0..8 {
        h.tick();
    }
    let l = *h.world.get::<Liked>(e).expect("the note was heard");
    assert_eq!(l.pos, [note[0].floor() as i32, note[1].floor() as i32, note[2].floor() as i32]);
    // A note from another block is not heard while one is liked.
    hear(&mut h.world, NOTE, at(p, -6.0));
    for _ in 0..10 {
        h.tick();
    }
    assert_eq!(h.world.get::<Liked>(e).unwrap().pos, l.pos);
    for _ in 0..620 {
        h.tick();
    }
    assert!(liked_deposit(&mut h.world, e).is_none(), "the liking is over after 600 ticks");
    assert!(h.world.get::<Liked>(e).is_none(), "the memory is erased");
}

#[test]
fn a_note_out_of_range_is_not_heard() {
    let Some((mut h, e, p)) = scene() else { return };
    hear(&mut h.world, NOTE, at(p, 20.0));
    for _ in 0..30 {
        h.tick();
    }
    assert!(h.world.get::<Liked>(e).is_none());
}

#[test]
fn a_jukebox_starts_the_dance_and_its_stop_ends_it() {
    let Some((mut h, e, p)) = scene() else { return };
    let juke = at(p, 3.0);
    hear(&mut h.world, PLAY, juke);
    assert!(h.world.get::<Dancing>(e).is_some() && h.world.get::<Jukebox>(e).is_some());
    hear(&mut h.world, STOP, at(p, 4.0));
    assert!(h.world.get::<Dancing>(e).is_some(), "another jukebox does not stop it");
    hear(&mut h.world, STOP, juke);
    assert!(h.world.get::<Dancing>(e).is_none() && h.world.get::<Jukebox>(e).is_none());
}

#[test]
fn a_jukebox_out_of_range_does_not_start_the_dance() {
    let Some((mut h, e, p)) = scene() else { return };
    hear(&mut h.world, PLAY, at(p, 12.0));
    assert!(h.world.get::<Dancing>(e).is_none());
}

#[test]
fn the_dance_ends_at_the_check_when_the_jukebox_is_gone() {
    let Some((mut h, e, p)) = scene() else { return };
    hear(&mut h.world, PLAY, at(p, 3.0));
    for _ in 0..25 {
        h.tick();
    }
    assert!(h.world.get::<Dancing>(e).is_none(), "the block there is no jukebox");
}
