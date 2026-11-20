//! `PatrollingMonster.LongDistancePatrolGoal`: a patrolling raider with a patrol target walks toward it in 10-block hops, curving around it (a
//! quarter turn, 0.4 of the way); the leader drags its companions (raiders within 16 blocks, not in a raid) by giving them its hop as their
//! target. With no companion left the patrol ends; a leader within 10 blocks of its target finds another within 500 blocks. A hop with no path
//! sends it a random 16-block step and refuses the goal for 200 ticks. The patrol is `raids::Patrol` (leader, patrolling, target, cooldown).
//! The failed hop is read the tick after: the walk was asked and nothing follows a path. Move flag.
use super::*;
use crate::raids::{Patrol, Raider};
use crate::targeting::Target;
use crate::{blocks, cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub leader_speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.7, leader_speed: 0.8 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Walker {
    p: Params,
    slot: u8,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Hopping {
    /// A hop was asked last tick: no path following now means no path was found.
    asked: bool,
}

pub static MODULE: Module = Module { name: "long_distance_patrol", flags: MOVE, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Walker { p: *params::<Params>(p), slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = w.resource::<Clock>().0;
    w.get::<Patrol>(e).is_some_and(|p| now >= p.cooldown_until && p.patrolling && p.target.is_some()) && w.get::<Target>(e).is_none() && !w.entity(e).contains::<crate::mount::Ridden>()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Hopping { asked: false });
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Hopping>();
}
/// The patrol is given a target (the leader's hop, a spawn): the walkers are asked.
pub fn wake(w: &mut World, e: Entity) {
    if let Some(slot) = slot_of(w, e, &MODULE) {
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
/// `PatrollingMonster.findPatrolTarget` from `at`: within 500 blocks.
pub fn find_target(w: &mut World, e: Entity, at: [i32; 3]) {
    let (dx, dz) = with_rng(w, e, |r| (-500 + r.next_int(1000), -500 + r.next_int(1000)));
    if let Some(mut p) = w.get_mut::<Patrol>(e) {
        p.target = Some([at[0] + dx, at[1], at[2] + dz]);
        p.patrolling = true;
    }
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(q: Query<(Entity, &Walker, Has<Following>, Has<NeedsPath>), With<Hopping>>, mut cmd: Commands) {
    for (e, t, following, asked_path) in &q {
        let (p, slot) = (t.p, t.slot);
        cmd.queue(move |w: &mut World| {
            let Some(own) = w.get::<Pos>(e).map(|p| p.0) else { return };
            let was_asked = w.get::<Hopping>(e).is_some_and(|h| h.asked);
            if following || asked_path {
                if let Some(mut h) = w.get_mut::<Hopping>(e) {
                    h.asked = false;
                }
                return;
            }
            if was_asked {
                // The hop found no path: a random step, and the goal rests for 200 ticks.
                let (dx, dz) = with_rng(w, e, |r| (r.next_int(16), r.next_int(16)));
                let at = ground(w, own.x.floor() as i32 - 8 + dx, own.z.floor() as i32 - 8 + dz);
                go_to_at(w, e, [f64::from(at[0]), f64::from(at[1]), f64::from(at[2])], p.speed, 0);
                let now = w.resource::<Clock>().0;
                if let Some(mut pt) = w.get_mut::<Patrol>(e) {
                    pt.cooldown_until = now + 200;
                }
                w.get_mut::<Hopping>(e).unwrap().asked = false;
                return;
            }
            let Some(pt) = w.get::<Patrol>(e).copied() else { return };
            let Some(target) = pt.target else { return };
            let friends = companions(w, e, own);
            if pt.patrolling && friends.is_empty() {
                w.get_mut::<Patrol>(e).unwrap().patrolling = false;
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
            } else if pt.leader && super::door_interact::closer_to_center(target, own, 10.0) {
                find_target(w, e, [own.x.floor() as i32, own.y.floor() as i32, own.z.floor() as i32]);
            } else {
                let to = Vec3::new(f64::from(target[0]) + 0.5, f64::from(target[1]), f64::from(target[2]) + 0.5);
                let d = Vec3::new(own.x - to.x, own.y - to.y, own.z - to.z);
                // `Vec3.yRot(90)`: the angle is in radians and the goal passes 90, as in the `Mob`.
                let (c, s) = (f64::from(90.0_f32.cos()), f64::from(90.0_f32.sin()));
                let r = Vec3::new(d.x * c + d.z * s, d.y, d.z * c - d.x * s);
                let to = Vec3::new(r.x * 0.4 + to.x, r.y * 0.4 + to.y, r.z * 0.4 + to.z);
                let (hx, hy, hz) = (to.x - own.x, to.y - own.y, to.z - own.z);
                let len = (hx * hx + hy * hy + hz * hz).sqrt().max(1.0e-9);
                let (hx, hz) = (hx / len * 10.0 + own.x, hz / len * 10.0 + own.z);
                let at = ground(w, hx.floor() as i32, hz.floor() as i32);
                let speed = if pt.leader { p.leader_speed } else { p.speed };
                go_to_at(w, e, [f64::from(at[0]), f64::from(at[1]), f64::from(at[2])], speed, 0);
                w.get_mut::<Hopping>(e).unwrap().asked = true;
                if pt.leader {
                    for f in friends {
                        if let Some(mut fp) = w.get_mut::<Patrol>(f) {
                            fp.target = Some(at);
                            fp.patrolling = true;
                        }
                        wake(w, f);
                    }
                }
            }
        });
    }
}
/// `Heightmap` position of the column (the first block that stops a walk, plus one).
fn ground(w: &World, x: i32, z: i32) -> [i32; 3] {
    horizon_client_lib::mobs::raider::heightmap_pos(blocks::world_of(w), x, z)
}
/// The patrol companions within 16 blocks that are not in a raid.
fn companions(w: &World, e: Entity, own: Vec3) -> Vec<Entity> {
    let mut v = Vec::new();
    w.resource::<Index>().for_radius(own.x, own.z, 16.0, |o| v.push(o));
    v.into_iter()
        .filter(|&o| {
            o != e
                && w.get::<Raider>(o).is_some_and(|r| r.raid.is_none())
                && w.get::<Health>(o).is_some_and(|h| h.0 > 0.0)
                && w.get::<Pos>(o).is_some_and(|q| (q.0.x - own.x).abs() <= 16.5 && (q.0.y - own.y).abs() <= 17.0 && (q.0.z - own.z).abs() <= 16.5)
        })
        .collect()
}
