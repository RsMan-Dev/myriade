//! `MakeLove` (`AnimalMakeLove` of the brain, `B::MakeLove`, `animal_brain.rs:1207-1238` start, `:1624-1700` tick, `:1833` stop): an
//! animal in love that sees a partner of its species in love chooses the nearest, tells it (its `breed_target`), and the two look at
//! each other and walk to each other (the initiator sets both sides each tick). `60 + nextInt(50)` ticks after the start, at less
//! than 3 blocks, the birth is `breed::give_birth` (the one the goal `breed` uses) and the initiator forgets its target and the
//! partner's. The love itself (`Love`, the food, the rest) is `breed`'s (`breed::LOVE` for the animal that has no goal `breed`).
//! The start conditions (`visible_mobs` present, no `breed_target`, not panicking) are the entry's `needs`, its duration (110) too.
use crate::behaviours::breed::{can_mate, finish_birth, give_birth, Love};
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::d2;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:make_love",
    mems: &[&BREED_TARGET, &WALK_TARGET, &LOOK_TARGET, &IS_PANICKING, &VISIBLE_MOBS],
    requires: &[Cap::Brain, Cap::Locomotion],
    can_start,
    start,
    tick,
    stop,
    damaged,
    ..Behaviour::DEFAULT
};

/// The species' values: the walk speed (1.0; the axolotl 0.2), how close is close enough (2), the birth's delay
/// (`spawn_min + nextInt(spawn_extra)`), and the two reactions that a species adds (the sniffer's state reset at the start and
/// its egg at the birth).
pub struct Params {
    pub speed: f32,
    pub close: i32,
    pub spawn_min: u64,
    pub spawn_extra: i32,
    pub on_start: fn(&mut World, Entity),
    pub birth: Birth,
    /// A condition on the entity and on its partner for them to mate (the sniffer: only in a calm state).
    pub ready: fn(&World, Entity) -> bool,
}
/// What the birth makes: a baby (`breed::give_birth`), or what the species lays (the parents rest and the experience is awarded;
/// the function makes the rest: an egg).
#[derive(Clone, Copy)]
pub enum Birth {
    Baby,
    Lays(fn(&mut World, Entity, Entity)),
}
fn none(_: &mut World, _: Entity) {}
fn always(_: &World, _: Entity) -> bool {
    true
}
impl Params {
    pub const DEFAULT: Params = Params { speed: 1.0, close: 2, spawn_min: 60, spawn_extra: 50, on_start: none, birth: Birth::Baby, ready: always };
}

/// The tick (own time) from which the birth may happen, while the behaviour runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SpawnAt(pub u64);

/// `AnimalMakeLove.findValidBreedPartner`: the nearest visible creature that can mate with it.
fn partner(w: &mut World, e: Entity, p: &Params) -> Option<Entity> {
    match find_closest(w, e, |w, who| matches!(who, Who::Entity(x) if can_mate(w, e, x) && (p.ready)(w, e) && (p.ready)(w, x)))? {
        Who::Entity(p) => Some(p),
        Who::Player(_) => None,
    }
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    partner(w, e, c.get::<Params>()).is_some()
}

fn gaze_and_walk(w: &mut World, e: Entity, with: Entity, p: &Params) {
    let t = Tracker::Entity(Who::Entity(with), true);
    set(w, e, &LOOK_TARGET, MemValue::Look(t), None);
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: t, speed: p.speed, close: p.close }), None);
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(other) = partner(w, e, p) else { return };
    (p.on_start)(w, e);
    set(w, e, &BREED_TARGET, MemValue::Who(Who::Entity(other)), None);
    // `BreedTarget` of the partner, then `lockGazeAndWalkToEachOther`: both sides, now and at each tick.
    set(w, other, &BREED_TARGET, MemValue::Who(Who::Entity(e)), None);
    gaze_and_walk(w, e, other, p);
    gaze_and_walk(w, other, e, p);
    let after = p.spawn_min + with_rng(w, e, |r| r.next_int(p.spawn_extra)) as u64;
    let at = now_of(w, e) + after;
    w.entity_mut(e).insert(SpawnAt(at));
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(MemValue::Who(Who::Entity(other))) = get(w, e, &BREED_TARGET).cloned() else { return false };
    let Some(SpawnAt(at)) = w.get::<SpawnAt>(e).copied() else { return false };
    let now = now_of(w, e);
    // `canMateWith`, `visible_contains`, the delay not passed, not panicking (the partner's panic is not read: as the `Mob`).
    let ok = info(w, Who::Entity(other)).is_some() && can_mate(w, e, other) && (p.ready)(w, e) && (p.ready)(w, other) && visible_contains(w, e, Who::Entity(other)) && now <= at && !present(w, e, &IS_PANICKING);
    if !ok {
        return false;
    }
    gaze_and_walk(w, e, other, p);
    gaze_and_walk(w, other, e, p);
    let (Some(me), Some(them)) = (w.get::<Pos>(e).map(|q| q.0), info(w, Who::Entity(other)).map(|i| i.pos)) else { return true };
    if d2(me, them) < 9.0 && now >= at {
        match p.birth {
            Birth::Baby => {
                give_birth(w, e, other);
            }
            Birth::Lays(lay) => {
                finish_birth(w, e, other);
                lay(w, e, other);
            }
        }
        // The partner forgets this animal; so does this one (its next tick ends the behaviour).
        if matches!(get(w, other, &BREED_TARGET), Some(MemValue::Who(Who::Entity(x))) if *x == e) {
            erase(w, other, &BREED_TARGET);
        }
        erase(w, e, &BREED_TARGET);
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<SpawnAt>();
    erase(w, e, &BREED_TARGET);
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
}

/// `Animal.actuallyHurt`: damage ends love.
fn damaged(w: &mut World, e: Entity, _: &Ctx, _: &crate::behaviours::Damaged) {
    w.entity_mut(e).remove::<Love>();
}
