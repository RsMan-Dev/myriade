//! `InteractWithDoor.create()`, the core behaviour of a brain (piglins, villagers): while the creature follows a path it has started, open the
//! doors it is leaving and entering and close the ones it opened or passed through (within 3 blocks, and not the two of the step it takes). No
//! flag. Woken by a path that has a node behind it; its state (the last node looked at, the cooldown, the doors to close) exists only while it runs.
//! Not done: another creature coming through the door is not looked for (as in the `Mob`).
use super::door_interact::{self as door};
use super::*;
use crate::{cow::*, goals::*};

#[derive(Component, Clone, Copy, Debug)]
pub struct InteractSeeker {
    pub slot: u8,
}
/// `lastCheckedNode`, `remainingCooldown` and `DOORS_TO_CLOSE`.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Interacting {
    last: Option<[i32; 3]>,
    cooldown: i32,
    doors: Vec<[i32; 3]>,
}

pub static MODULE: Module = Module { name: "interact_with_door", flags: 0, equip, can_use, start, stop, systems, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(InteractSeeker { slot: inst.slot });
}
/// The step of the path being followed: the node left and the node ahead.
fn step(w: &World, e: Entity) -> Option<([i32; 3], [i32; 3])> {
    let path = w.get::<Nav>(e)?.path.as_ref().filter(|p| p.next > 0 && !p.is_done())?;
    let n = |i: usize| [path.nodes[i].x, path.nodes[i].y, path.nodes[i].z];
    Some((n(path.next - 1), n(path.next)))
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    step(w, e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Interacting::default());
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Interacting>();
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((seek, run).in_set(slot));
}
fn seek(q: Query<(Entity, &InteractSeeker, &Nav), (With<Following>, Without<Interacting>)>, mut reqs: ResMut<Requests>) {
    for (e, s, n) in &q {
        if n.path.as_ref().is_some_and(|p| p.next > 0 && !p.is_done()) {
            reqs.0.push(Req::Start(e, s.slot));
        }
    }
}
fn run(q: Query<(Entity, &InteractSeeker), With<Interacting>>, mut cmd: Commands) {
    for (e, s) in &q {
        let slot = s.slot;
        cmd.queue(move |w: &mut World| {
            let Some((from, to)) = step(w, e) else {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
                return;
            };
            let mut st = w.entity_mut(e).take::<Interacting>().unwrap_or_default();
            if st.last == Some(to) {
                st.cooldown = 20;
            } else {
                st.cooldown -= 1;
                if st.cooldown > 0 {
                    w.entity_mut(e).insert(st);
                    return;
                }
            }
            st.last = Some(to);
            for (p, always) in [(from, true), (to, false)] {
                let Some(open) = door::mob_door_open(w, p) else { continue };
                if !open {
                    door::set_open_at(w, p, true);
                }
                if (always || !open) && !st.doors.contains(&p) {
                    st.doors.push(p);
                }
            }
            let at = w.get::<Pos>(e).map(|p| p.0);
            let doors = std::mem::take(&mut st.doors);
            for d in doors {
                if from == d || to == d {
                    st.doors.push(d);
                    continue;
                }
                if at.is_some_and(|a| door::closer_to_center(d, a, 3.0)) && door::mob_door_open(w, d) == Some(true) {
                    door::set_open_at(w, d, false);
                }
            }
            w.entity_mut(e).insert(st);
        });
    }
}
