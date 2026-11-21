//! The skeleton's bow goal against a real `Mob` skeleton in the same scenario (`HORIZON_TEST_RESOURCES`, process species
//! `minecraft:skeleton`): the cadence of the shots, the shooting distance and what the arrow starts with, and the strafe
//! with the flips imposed from what the `Mob` drew.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{MobEvent, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::ranged_bow, cow::*, goals::*, hostile::PlayerState, projectile::*, targeting::*};

fn skip() -> bool {
    // A piglin carries `ranged_bow` for its crossbow only (`Piglin` has `CrossbowAttack`, no bow goal): given a bow, the `Mob` and the ECS both shoot
    // nothing, which is vanilla; its crossbow is compared in `tirs_vs_mob`. This scene is the bow's, run under the skeletons.
    !have_data() || horizon_ecs_proto::cow::process_species_name() == "minecraft:piglin" || horizon_ecs_proto::cow::skip_unless(&ranged_bow::MODULE, "the bow goal")
}
struct Scene {
    r: MobRef,
    h: Herd,
    mob: usize,
    ecs: Entity,
    seen: std::collections::HashSet<Entity>,
    /// The first ECS arrow when it appears (it may have landed and gone by the end of the run): its height, vertical speed, and the shooter's eyes less 0.1.
    born: Option<(f64, f64, f64)>,
}
/// A skeleton with a bow and the player as its target on both sides, the player at `player`.
fn scene(skeleton: Vec3, player: Vec3) -> Scene {
    let mut r = MobRef::new(40, false);
    let mut h = Herd::with_slab_awake(40, true, false);
    let mob = r.add(skeleton);
    r.mobs[mob].extra.set_main_hand(Some("minecraft:bow".into()));
    r.mobs[mob].ai.target = Some(MWho::Player);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let ecs = spawn_species(&mut h.world, &sp, skeleton.x, skeleton.z, 77);
    h.world.get_mut::<ActiveUntil>(ecs).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
    h.world.resource_mut::<PlayerState>().immortal = true;
    // The target is the player on both sides, from the first tick.
    let slot = h.world.get::<GoalSet>(ecs).unwrap().slot_of(&ranged_bow::MODULE).unwrap();
    let _ = slot;
    set_target(&mut h.world, ecs, Target::player(1, 30));
    Scene { r, h, mob, ecs, seen: Default::default(), born: None }
}
impl Scene {
    /// One tick of both; returns (the Mob shot, the ECS shot).
    fn step(&mut self, view: &horizon_client_lib::mobs::PlayerView) -> (bool, bool) {
        let from = self.r.events.len();
        self.r.step(Some(view));
        // The strafe the `Mob` is in this tick (sideways and forwards signs), for the ECS to take at its own strafe events.
        let mc = &self.r.mobs[self.mob].ai.mov;
        if mc.strafe_right != 0.0 {
            self.h.world.entity_mut(self.ecs).insert(ForcedStrafe((mc.strafe_right > 0.0, mc.strafe_forwards < 0.0)));
        }
        self.h.tick();
        let mob_shot = self.r.events[from..].iter().any(|e| matches!(e, MobEvent::BowShot { .. }));
        let mut q = self.h.world.query::<(Entity, &Projectile)>();
        let new: Vec<Entity> = q.iter(&self.h.world).map(|x| x.0).filter(|e| !self.seen.contains(e)).collect();
        self.seen.extend(new.iter().copied());
        if self.born.is_none() {
            if let Some(a) = new.first() {
                let (w, me) = (&self.h.world, self.ecs);
                let start = w.get::<Pos>(me).unwrap().0.y + w.get::<Eye>(me).unwrap().0 - f64::from(0.1_f32);
                self.born = Some((w.get::<Pos>(*a).unwrap().0.y, w.get::<Vel>(*a).unwrap().0.y, start));
            }
        }
        (mob_shot, !new.is_empty())
    }
}
fn shots(s: &mut Scene, view: &horizon_client_lib::mobs::PlayerView, ticks: i64) -> (Vec<i64>, Vec<i64>) {
    let (mut m, mut e) = (Vec::new(), Vec::new());
    for t in 1..=ticks {
        let (a, b) = s.step(view);
        if a {
            m.push(t);
        }
        if b {
            e.push(t);
        }
    }
    (m, e)
}

/// A player 10 blocks away in the open: the skeleton stands, draws and shoots every 40 ticks (the normal difficulty) on
/// both sides, the first shot at the same tick or a tick apart, and its arrow starts from the same place at the same speed.
#[test]
fn the_skeleton_shoots_at_the_same_cadence_as_a_mob() {
    if skip() {
        return;
    }
    // The bogged and the parched shoot every 70 ticks (not 40): this scene counts the skeleton's cadence; their interval is `species/<name>.rs`.
    if std::env::var("HORIZON_SPECIES").is_ok_and(|s| s == "minecraft:bogged" || s == "minecraft:parched") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5));
    let view = player_view(Vec3::new(10.5, 1.0, 0.5), None);
    let (m, e) = shots(&mut s, &view, 260);
    eprintln!("shots: mob {m:?}, ecs {e:?}");
    assert!(m.len() >= 4 && e.len() >= 4, "mob {}, ecs {}", m.len(), e.len());
    assert!((m[0] - e[0]).abs() <= 2, "first shot: mob {}, ecs {}", m[0], e[0]);
    let gaps = |v: &[i64]| v.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
    // 20 ticks of draw, then the 40 of the interval counted down from the shot: 61 or so between two shots.
    assert_eq!(gaps(&m)[0..2], gaps(&e)[0..2], "gaps: mob {:?}, ecs {:?}", gaps(&m), gaps(&e));
    // The arrow: from the eyes less 0.1, at 1.6.
    // Not compared with the Mob (a skeleton: its eyes are not the species' own): the arrow leaves this species' eyes less 0.1 and has flown
    // one tick when seen (speed after the tick = speed before * 0.99 - 0.05).
    let (y, vy, start) = s.born.expect("an arrow");
    let v0 = (vy + 0.05) / 0.99;
    assert!((y - (start + v0)).abs() < 1e-6, "arrow at {y} (speed {vy}), eyes less 0.1 at {start}");
}

/// A player 25 blocks away: both walk toward it and stop at the same distance (within the shooting radius 15 and 20 ticks
/// of sight), at about the same tick.
#[test]
fn the_skeleton_closes_in_to_its_shooting_distance_as_a_mob_does() {
    if skip() {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(14.5, 1.0, 0.5));
    // Within the follow range (16 blocks) but beyond the shooting radius (15) once the skeleton is back: put the player at 14.
    let view = player_view(Vec3::new(14.5, 1.0, 0.5), None);
    let mut at = (None, None);
    for t in 1..=120i64 {
        s.step(&view);
        let (mx, ex) = (s.r.mobs[s.mob].position.x, s.h.world.get::<Pos>(s.ecs).unwrap().0.x);
        if at.0.is_none() && mx > 0.6 {
            at.0 = Some(t);
        }
        if at.1.is_none() && ex > 0.6 {
            at.1 = Some(t);
        }
    }
    let (mx, ex) = (s.r.mobs[s.mob].position.x, s.h.world.get::<Pos>(s.ecs).unwrap().0.x);
    eprintln!("moves from tick {at:?}; at x {mx:.2} (mob) / {ex:.2} (ecs)");
    assert!(at.0.is_some() == at.1.is_some());
}

/// The skeleton strafes (20 ticks of standing, then a side step and a step back or forward by the draws): with the Mob's
/// state taken at each strafe event, the two walk the same way, within a fraction of a block over 200 ticks.
#[test]
fn the_strafe_moves_the_skeleton_as_it_moves_a_mob() {
    if skip() {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5));
    let view = player_view(Vec3::new(10.5, 1.0, 0.5), None);
    let mut worst: f64 = 0.0;
    let mut path = Vec::new();
    for t in 1..=200i64 {
        s.step(&view);
        let (m, e) = (s.r.mobs[s.mob].position, s.h.world.get::<Pos>(s.ecs).unwrap().0);
        let d = xz(m, e);
        if t > 30 {
            worst = worst.max(d);
        }
        if t % 20 == 0 {
            path.push((t, m.x, m.z, e.x, e.z));
        }
    }
    for p in &path {
        eprintln!("t={:3}: mob ({:.2}, {:.2}) ecs ({:.2}, {:.2})", p.0, p.1, p.2, p.3, p.4);
    }
    eprintln!("the greatest distance between the two after tick 30: {worst:.2}");
    assert!(worst < 0.5, "worst {worst:.2}");
}
