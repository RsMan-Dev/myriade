//! `SilverfishWakeUpFriendsGoal` (fiche 13, `mobs/hostile.rs:101-169`): a hit by an entity starts a countdown of 10 selector
//! ticks (`reduced(20)`); at zero, the infested blocks of the sweep break until a coin flip stops it. Only what a sweep does
//! to the world is ported here: no drops, and no stone with griefing off (DOUTE in the notes).
use super::*;
use crate::{cow::*, goals::*};
use crate::rules::silverfish_wake_up_friends::{is_infested, offsets};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `mobGriefing`: the infested blocks break (`BreakBlock`); off, the Mob turns them back into stone (not ported).
    pub griefing: bool,
    /// Selector ticks of the countdown: `reduced(20)`.
    pub wait: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { griefing: true, wait: 10 };
}
/// Selector ticks left before the sweep; 0 = inactive.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WakeCount(pub i32);
/// The configuration of this animal (read when a hit starts the countdown and by the sweep).
#[derive(Component, Clone, Copy, Debug)]
pub struct Friends {
    pub griefing: bool,
    pub wait: i32,
}
/// The goal runs: its tick is visited on the selector's ticks.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WakingFriends;

pub static MODULE: Module = Module {
    name: "silverfish_wake_up_friends",
    flags: 0,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Living],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert((WakeCount(0), Friends { griefing: p.griefing, wait: p.wait }));
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<WakeCount>(e).is_some_and(|c| c.0 > 0)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(WakingFriends);
    // The start tick is a tick of the goal (GUIDE 9.1).
    tick(w, e);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<WakingFriends>();
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((wake_on_hit.after(apply_damage::<false>), wake_run).in_set(slot));
}
/// `Silverfish.hurtServer`: a hit by an entity starts the countdown of an animal that has none (magic is not a cause here).
fn wake_on_hit(causes: Res<HurtCauses>, mut q: Query<(&mut WakeCount, &Friends, &GoalSet)>, mut reqs: ResMut<Requests>) {
    for &(e, cause) in &causes.0 {
        if cause != Cause::Entity {
            continue;
        }
        let Ok((mut count, friends, g)) = q.get_mut(e) else { continue };
        if count.0 != 0 {
            continue;
        }
        count.0 = friends.wait;
        if let Some(slot) = g.slot_of(&MODULE) {
            reqs.0.push(Req::Start(e, slot));
        }
    }
}
/// On the selector's ticks, a running goal ticks (or ends when its count is at zero: `canContinue`).
fn wake_run(clock: Res<Clock>, q: Query<(Entity, &WakeCount, &GoalSet, Option<&Phase>), With<WakingFriends>>, mut reqs: ResMut<Requests>) {
    for (e, count, g, phase) in &q {
        let now = clock.of(e);
        let phase = phase.map_or(0, |p| p.0);
        if (now + phase) % 2 == 1 && now > 1 {
            continue;
        }
        let Some(slot) = g.slot_of(&MODULE) else { continue };
        if count.0 <= 0 {
            reqs.0.push(Req::End(e, slot));
        } else {
            reqs.0.push(Req::Call(e, tick));
        }
    }
}
/// `SilverfishWakeUpFriendsGoal.tick`: one count down; at zero, the sweep.
fn tick(w: &mut World, e: Entity) {
    if w.get::<WakingFriends>(e).is_none() {
        return;
    }
    let Some(mut count) = w.get_mut::<WakeCount>(e) else { return };
    count.0 -= 1;
    if count.0 > 0 {
        return;
    }
    sweep(w, e);
}
/// The sweep: y within 5, x and z within 10, in the order of the Mob; each infested block found is broken, and a coin flip
/// after it stops the sweep.
fn sweep(w: &mut World, e: Entity) {
    let Some(p) = w.get::<Pos>(e).map(|p| [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32]) else { return };
    let griefing = w.get::<Friends>(e).is_some_and(|f| f.griefing);
    let mw = w.resource::<Level>().mw.0;
    let mut rng = *w.get::<Rng>(e).unwrap();
    let mut changes = Vec::new();
    'sweep: for y in offsets(5) {
        for x in offsets(10) {
            for z in offsets(10) {
                let q = [p[0] + x, p[1] + y, p[2] + z];
                if !is_infested(mw.block_name(q[0], q[1], q[2])) {
                    continue;
                }
                if griefing {
                    changes.push((q[0], q[1], q[2], "minecraft:air"));
                }
                if rng.next_int(2) == 0 {
                    break 'sweep;
                }
            }
        }
    }
    *w.get_mut::<Rng>(e).unwrap() = rng;
    w.resource_mut::<BlockChanges>().0.extend(changes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;
    use crate::{cow::{ActiveUntil, DamageQueue, Herd, SpeciesTable, spawn_species}, goals::set_lazy_look};

    fn guard(test: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            crate::cow::skip_note(test, "HORIZON_TEST_RESOURCES is not set");
            return true;
        }
        false
    }
    fn herd_with(entries: Vec<Entry>) -> (Herd, Entity) {
        let mut h = Herd::with_slab_awake(20, true, false);
        set_lazy_look(&mut h.world, false);
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = Config { entries: Box::leak(entries.into_boxed_slice()) };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        for _ in 0..40 {
            h.tick();
        }
        (h, e)
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn count(h: &Herd, e: Entity) -> i32 {
        h.world.get::<WakeCount>(e).unwrap().0
    }
    /// A 4 x 3 x 5 block of infested stone beside the cow, on its level and one above and below.
    fn infest(h: &mut Herd, e: Entity) -> Vec<[i32; 3]> {
        let p = h.world.get::<Pos>(e).unwrap().0;
        let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let mut cells = Vec::new();
        for x in bx + 3..bx + 7 {
            for y in by - 1..=by + 1 {
                for z in bz - 2..=bz + 2 {
                    h.set_block(x, y, z, "minecraft:infested_stone");
                    cells.push([x, y, z]);
                }
            }
        }
        h.tick();
        cells
    }
    fn infested_left(h: &Herd, cells: &[[i32; 3]]) -> usize {
        let mw = h.world.resource::<Level>().mw.0;
        cells.iter().filter(|c| mw.block_name(c[0], c[1], c[2]).starts_with("minecraft:infested_")).count()
    }

    #[test]
    fn a_hit_by_an_entity_breaks_blocks_after_the_countdown() {
        if guard("a_hit_by_an_entity_breaks_blocks_after_the_countdown") {
            return;
        }
        let (mut h, e) = herd_with(vec![Entry::new(&MODULE, 3, &Params::DEFAULT)]);
        let cells = infest(&mut h, e);
        let before = infested_left(&h, &cells);
        h.hit(e, 1.0, (-5.0, 0.5));
        let mut first_change = None;
        for t in 1..=40 {
            h.tick();
            if t == 4 {
                assert!(running(&h, e) && count(&h, e) > 0, "the hit starts the countdown");
            }
            if first_change.is_none() && infested_left(&h, &cells) < before {
                first_change = Some(t);
            }
        }
        let t = first_change.expect("witness: blocks break after the countdown");
        eprintln!("first infested block broken {t} ticks after the hit");
        assert!((10..=25).contains(&t), "about 20 ticks after the hit, got {t}");
        assert!(!running(&h, e) && count(&h, e) == 0, "the goal ends on the selector tick after the sweep");
    }

    #[test]
    fn a_fall_wakes_nobody_and_a_hit_does() {
        if guard("a_fall_wakes_nobody_and_a_hit_does") {
            return;
        }
        let (mut h, e) = herd_with(vec![Entry::new(&MODULE, 3, &Params::DEFAULT)]);
        let cells = infest(&mut h, e);
        let before = infested_left(&h, &cells);
        h.world.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Fall);
        for _ in 0..40 {
            h.tick();
        }
        assert!(count(&h, e) == 0 && !running(&h, e) && infested_left(&h, &cells) == before, "a fall does not wake");
        h.hit(e, 1.0, (-5.0, 0.5));
        h.tick();
        h.tick();
        assert!(count(&h, e) > 0 || running(&h, e), "witness: an entity's hit does wake it");
    }

    #[test]
    fn a_countdown_is_not_restarted_by_a_second_hit() {
        if guard("a_countdown_is_not_restarted_by_a_second_hit") {
            return;
        }
        let (mut h, e) = herd_with(vec![Entry::new(&MODULE, 3, &Params::DEFAULT)]);
        infest(&mut h, e);
        h.hit(e, 1.0, (-5.0, 0.5));
        h.tick();
        h.tick();
        let left = count(&h, e);
        assert!(left > 0 && left < 10, "witness: the countdown runs, {left} left");
        h.hit(e, 1.0, (-5.0, 0.5));
        h.tick();
        assert!(count(&h, e) <= left, "a hit while counting does not reset the count");
    }
}
