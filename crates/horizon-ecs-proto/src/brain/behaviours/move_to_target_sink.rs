//! `MoveToTargetSink`: walks to the walk target (a path asked for, made again when the target moves by more than 2 blocks), for
//! its duration or until it is reached or the walk is over. `path` is present while it runs.
use crate::IFloor as _;
use crate::brain::*;
use crate::cow::*;
use crate::goals::stop_nav;

#[derive(Component)]
pub struct Sink {
    cooldown: i32,
    last_target: Option<[i32; 3]>,
}
pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:move_to_target_sink", mems: &[&WALK_TARGET, &PATH, &CANT_REACH_WALK_TARGET_SINCE], equip, can_start, start, tick, stop, sleeps, ..Behaviour::DEFAULT };
fn equip(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).insert(Sink { cooldown: 0, last_target: None });
}
fn block_of(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).unwrap().0;
    [p.x.ifloor(), p.y.ifloor(), p.z.ifloor()]
}
fn reached(w: &World, e: Entity, walk: &WalkTarget) -> bool {
    walk.target.block(w).is_some_and(|p| {
        let b = block_of(w, e);
        (p[0] - b[0]).abs() + (p[1] - b[1]).abs() + (p[2] - b[2]).abs() <= walk.close
    })
}
fn ask(w: &mut World, e: Entity, target: [i32; 3], speed: f32) {
    walk_to(w, e, [f64::from(target[0]) + 0.5, f64::from(target[1]), f64::from(target[2]) + 0.5], f64::from(speed));
}
fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if w.get::<Sink>(e).unwrap().cooldown > 0 {
        w.get_mut::<Sink>(e).unwrap().cooldown -= 1;
        return false;
    }
    let Some(walk) = walk_target(w, e) else { return false };
    if !reached(w, e, &walk) && let Some(t) = walk.target.block(w) {
        ask(w, e, t, walk.speed);
        w.get_mut::<Sink>(e).unwrap().last_target = Some(t);
        set(w, e, &PATH, MemValue::Unit, None);
        return true;
    }
    erase(w, e, &WALK_TARGET);
    if reached(w, e, &walk) {
        erase(w, e, &CANT_REACH_WALK_TARGET_SINCE);
    }
    false
}
fn start(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<SinkWatch>();
}
fn sleeps(w: &World, e: Entity, _: &Ctx) -> bool {
    w.get::<SinkWatch>(e).is_some()
}
fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(walk) = walk_target(w, e) else { return false };
    if nav_done(w, e) || reached(w, e, &walk) {
        return false;
    }
    let (Some(last), Some(now)) = (w.get::<Sink>(e).unwrap().last_target, walk.target.block(w)) else { return false };
    let d = (0..3).map(|i| i64::from(now[i] - last[i]).pow(2)).sum::<i64>();
    if d > 4 {
        ask(w, e, now, walk.speed);
        w.get_mut::<Sink>(e).unwrap().last_target = Some(now);
    }
    // A fixed target: nothing to do until the walk ends or the entity is close (the watch wakes the brain), so the brain may sleep.
    match (walk.target, w.get::<SinkWatch>(e).is_some()) {
        (Tracker::Block(_) | Tracker::Point(_), false) => {
            w.entity_mut(e).insert(SinkWatch { target: now, close: walk.close });
        }
        (Tracker::Entity(..), true) => {
            w.entity_mut(e).remove::<SinkWatch>();
        }
        _ => {}
    }
    true
}
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<SinkWatch>();
    stop_nav(w, e);
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.reach = 1;
    }
    erase(w, e, &WALK_TARGET);
    erase(w, e, &PATH);
}
