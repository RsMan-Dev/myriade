//! The targeting of creatures against a real `Mob` in the same scenario (`HORIZON_TEST_RESOURCES` set): a zombie that hunts
//! a cow (a stand-in for the villager: a passive species of the table), the nearest of two, the loss of a dead target,
//! the preference between two instances by priority, a creature that hits a zombie (hurt-by, priority 1) and the alert.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{
    Damage, DamageKind, Species, Who as MWho,
    goals::{HurtByTarget, NearestTarget},
};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{Entry, hurt_by, nearest_target},
    cow::*,
    goals::*,
    species::{Config, zombie},
    targeting::*,
};

/// The zombie's selector with a cow where the villager is: hurt-by (1, alerts), the nearest player (2), the nearest cow
/// (3, not seen needed), then the zombie's goals.
fn zombie_config() -> Config {
    let cow: &'static nearest_target::Params = Box::leak(Box::new(nearest_target::Params { types: &["minecraft:cow"], must_see: false, ..nearest_target::Params::DEFAULT }));
    let mut entries = vec![
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::ALERT),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, cow),
    ];
    entries.extend_from_slice(zombie::DEF.goals);
    Config { entries: Box::leak(entries.into_boxed_slice()) }
}
struct World2 {
    r: MobRef,
    h: Herd,
    zombie: Species2,
    cow: Species2,
}
type Species2 = horizon_ecs_proto::cow::Species;
fn world() -> World2 {
    let r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    // The controls compare with the `Mob`, which looks around for every animal: the lazy look (on by default) is off.
    horizon_ecs_proto::goals::set_lazy_look(&mut h.world, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut zombie = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
    zombie.goals = zombie_config();
    let cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    World2 { r, h, zombie, cow }
}
impl World2 {
    /// A zombie on both sides; returns (mob index, entity).
    fn zombie(&mut self, at: Vec3, seed: u64) -> (usize, Entity) {
        let i = self.r.add(at);
        let m = &mut self.r.mobs[i];
        m.ai.targets.clear();
        m.ai.targets.add(1, HurtByTarget::new().alert());
        m.ai.targets.add(2, NearestTarget::player(true));
        m.ai.targets.add(3, NearestTarget::species(&[Species::Cow], false));
        let e = spawn_species(&mut self.h.world, &self.zombie, at.x, at.z, seed);
        self.h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        self.h.world.resource_mut::<Entered>().0.push(e);
        (i, e)
    }
    /// A cow that stands still on both sides.
    fn prey(&mut self, at: Vec3, seed: u64) -> (usize, Entity) {
        self.creature(at, seed, "minecraft:cow")
    }
    /// A creature of the named species that stands still on both sides.
    fn creature(&mut self, at: Vec3, seed: u64, name: &str) -> (usize, Entity) {
        let i = self.r.add_kind(at, horizon_ecs_proto::cow::mob_kind(name));
        let m = &mut self.r.mobs[i];
        m.ai.goals.clear();
        m.ai.targets.clear();
        let sp = *self.h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap();
        let e = spawn_species(&mut self.h.world, &sp, at.x, at.z, seed);
        self.h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
        (i, e)
    }
    fn step(&mut self, player: Option<&mobs_view::PlayerView>) {
        self.r.step(player);
        self.h.tick();
    }
    fn mob_target(&self, i: usize) -> Option<MWho> {
        self.r.mobs[i].ai.target
    }
    fn ecs_target(&self, e: Entity) -> Option<Who> {
        self.h.world.get::<Target>(e).map(|t| t.who)
    }
}
mod mobs_view {
    pub use horizon_client_lib::mobs::PlayerView;
}
fn skip() -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:zombie") || horizon_ecs_proto::cow::skip_unless(&nearest_target::MODULE, "creature targets")
}
fn mean_tick(v: &[Option<i64>]) -> (f64, usize) {
    let n = v.iter().flatten().count();
    (v.iter().flatten().sum::<i64>() as f64 / n.max(1) as f64, n)
}

/// 40 zombies each with a cow 6 blocks away: the tick of the acquisition (one chance in 5 per ask, an ask every 2 ticks),
/// and that the target is the cow.
#[test]
fn a_zombie_takes_the_nearest_cow_as_a_mob_does() {
    if skip() {
        return;
    }
    let mut w = world();
    let mut pairs = Vec::new();
    for k in 0..40 {
        let (x, z) = (-56.0 + 14.0 * f64::from(k % 8), -28.0 + 14.0 * f64::from(k / 8));
        let zb = w.zombie(Vec3::new(x + 0.5, 1.0, z + 0.5), 100 + k as u64);
        let pr = w.prey(Vec3::new(x + 6.5, 1.0, z + 0.5), 500 + k as u64);
        pairs.push((zb, pr));
    }
    let (mut mob, mut ecs) = (vec![None; 40], vec![None; 40]);
    for t in 1..=150i64 {
        w.step(None);
        for (k, ((mi, ee), (pi, pe))) in pairs.iter().enumerate() {
            if mob[k].is_none() && w.mob_target(*mi) == Some(MWho::Mob(w.r.mobs[*pi].id)) {
                mob[k] = Some(t);
            }
            if ecs[k].is_none() && w.ecs_target(*ee) == Some(Who::Entity(*pe)) {
                ecs[k] = Some(t);
            }
        }
    }
    let ((mm, mn), (em, en)) = (mean_tick(&mob), mean_tick(&ecs));
    eprintln!("acquired the cow: mob {mn}/40 at {mm:.1}, ecs {en}/40 at {em:.1}");
    assert!(mn >= 38 && en >= 38, "mob {mn}, ecs {en}");
    assert!((mm - em).abs() < 4.0, "mob {mm:.1}, ecs {em:.1}");
}

/// Two cows, at 5 and 8 blocks: the nearest is the target, on both sides.
#[test]
fn the_nearest_of_two_cows_is_chosen() {
    if skip() {
        return;
    }
    let mut w = world();
    let mut sets = Vec::new();
    for k in 0..10 {
        let (x, z) = (-50.0 + 20.0 * f64::from(k % 5), -20.0 + 40.0 * f64::from(k / 5));
        let zb = w.zombie(Vec3::new(x + 0.5, 1.0, z + 0.5), 200 + k as u64);
        let near = w.prey(Vec3::new(x + 5.5, 1.0, z + 0.5), 600 + 2 * k as u64);
        let far = w.prey(Vec3::new(x - 7.5, 1.0, z + 0.5), 601 + 2 * k as u64);
        sets.push((zb, near, far));
    }
    for _ in 0..120 {
        w.step(None);
    }
    for ((mi, ee), (npi, npe), (fpi, _)) in sets {
        assert_eq!(w.mob_target(mi), Some(MWho::Mob(w.r.mobs[npi].id)), "the Mob takes the nearer cow");
        assert_eq!(w.ecs_target(ee), Some(Who::Entity(npe)), "the ECS zombie takes the nearer cow");
        let _ = fpi;
    }
}

/// The target dies: both sides lose it within a tick or two of each other.
#[test]
fn a_dead_target_is_lost_on_both_sides() {
    if skip() {
        return;
    }
    let mut w = world();
    let mut rows = Vec::new();
    for k in 0..12 {
        let (x, z) = (-50.0 + 20.0 * f64::from(k % 5), -30.0 + 30.0 * f64::from(k / 5));
        let zb = w.zombie(Vec3::new(x + 0.5, 1.0, z + 0.5), 300 + k as u64);
        let pr = w.prey(Vec3::new(x + 5.5, 1.0, z + 0.5), 700 + k as u64);
        rows.push((zb, pr));
    }
    for _ in 0..100 {
        w.step(None);
    }
    for (_, ((_, _), (pi, pe))) in rows.iter().enumerate().map(|r| (r.0, *r.1)) {
        let mut lvl = w.r.tl.level();
        w.r.mobs[pi].damage(&mut lvl, 100.0, Damage::of(DamageKind::MobAttack));
        drop(lvl);
        w.h.world.resource_mut::<DamageQueue>().add(pe, 100.0, Cause::Entity);
    }
    let (mut mob, mut ecs): (Vec<Option<i64>>, Vec<Option<i64>>) = (vec![None; 12], vec![None; 12]);
    for t in 1..=40i64 {
        w.step(None);
        for (k, ((mi, ee), _)) in rows.iter().enumerate() {
            if mob[k].is_none() && w.mob_target(*mi).is_none() {
                mob[k] = Some(t);
            }
            if ecs[k].is_none() && w.ecs_target(*ee).is_none() {
                ecs[k] = Some(t);
            }
        }
    }
    let ((mm, mn), (em, en)) = (mean_tick(&mob), mean_tick(&ecs));
    eprintln!("target lost after its death: mob {mn}/12 at {mm:.1}, ecs {en}/12 at {em:.1}");
    assert!(mn == 12 && en == 12 && (mm - em).abs() < 3.0, "mob {mm:.1} ({mn}), ecs {em:.1} ({en})");
}

/// A zombie that hunts a cow (priority 3) and a player who comes into view (priority 2): the player replaces the cow.
#[test]
fn a_higher_priority_instance_takes_the_target_over() {
    if skip() {
        return;
    }
    let mut w = world();
    let mut rows = Vec::new();
    for k in 0..20 {
        let (x, z) = (-52.0 + 26.0 * f64::from(k % 4), -40.0 + 20.0 * f64::from(k / 4));
        let zb = w.zombie(Vec3::new(x + 0.5, 1.0, z + 0.5), 400 + k as u64);
        let pr = w.prey(Vec3::new(x + 4.5, 1.0, z - 3.5), 800 + k as u64);
        rows.push((zb, pr));
    }
    for _ in 0..80 {
        w.step(None);
    }
    // One player far from everyone but within sight range of nobody: it walks to the middle of the zombies' row.
    let (mut mob, mut ecs): (Vec<Option<i64>>, Vec<Option<i64>>) = (vec![None; 20], vec![None; 20]);
    w.h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    let view = player_view(Vec3::new(0.5, 1.0, 0.5), None);
    for t in 1..=150i64 {
        w.step(Some(&view));
        for (k, ((mi, ee), _)) in rows.iter().enumerate() {
            if mob[k].is_none() && w.mob_target(*mi) == Some(MWho::Player) {
                mob[k] = Some(t);
            }
            if ecs[k].is_none() && w.ecs_target(*ee) == Some(Who::Player(1)) {
                ecs[k] = Some(t);
            }
        }
    }
    let ((mm, mn), (em, en)) = (mean_tick(&mob), mean_tick(&ecs));
    eprintln!("the player takes over from the cow: mob {mn}/20 at {mm:.1}, ecs {en}/20 at {em:.1}");
    // A zombie near the follow range (35 blocks) is in or out by the way it happens to walk: one may differ.
    assert!(mn.abs_diff(en) <= 1 && mn > 4, "the zombies that switch: mob {mn}, ecs {en}");
    assert!((mm - em).abs() < 6.0, "mob {mm:.1}, ecs {em:.1}");
}

/// A cow hits a zombie that hunts a player (hurt-by is priority 1, the player's goal 2): the cow becomes the target at the
/// next selector tick, on both sides. And a cow that hits a zombie with no target alerts the zombie that stands near it.
#[test]
fn a_creature_that_hits_a_zombie_becomes_its_target_and_alerts_the_others() {
    if skip() {
        return;
    }
    let mut w = world();
    let (zm, ze) = w.zombie(Vec3::new(10.5, 1.0, 0.5), 1);
    let (pm, pe) = w.prey(Vec3::new(12.5, 1.0, -1.5), 3);
    // Far from the player (50 blocks): a zombie with no target, and another near it to be alerted.
    let (gm, ge) = w.zombie(Vec3::new(50.5, 1.0, 0.5), 4);
    let (hm, he) = w.zombie(Vec3::new(52.5, 1.0, 2.5), 5);
    // A pig, which the zombies' cow instance does not take: the idle zombies have no target before the hit.
    let (qm, qe) = w.creature(Vec3::new(52.5, 1.0, -1.5), 6, "minecraft:pig");
    w.h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    let view = player_view(Vec3::new(0.5, 1.0, 0.5), None);
    for _ in 0..150 {
        w.step(Some(&view));
    }
    assert_eq!((w.mob_target(zm), w.ecs_target(ze)), (Some(MWho::Player), Some(Who::Player(1))), "both hunt the player");
    let hit = |w: &mut World2, victim: (usize, Entity), by: (usize, Entity), at: [f64; 3]| {
        let id = w.r.mobs[by.0].id;
        let mut lvl = w.r.tl.level();
        w.r.mobs[victim.0].damage(&mut lvl, 1.0, Damage { kind: DamageKind::MobAttack, attacker: Some(MWho::Mob(id)), from: Some(at), attacker_species: Some(Species::Pig) });
        drop(lvl);
        w.h.world.resource_mut::<DamageQueue>().add_by_entity(victim.1, 1.0, (at[0], at[2]), by.1);
    };
    hit(&mut w, (zm, ze), (pm, pe), [12.5, 1.0, -1.5]);
    hit(&mut w, (gm, ge), (qm, qe), [52.5, 1.0, -1.5]);
    let cow = w.r.mobs[pm].id;
    let other = w.r.mobs[qm].id;
    let mut ticks: [(Option<i64>, Option<i64>); 3] = [(None, None); 3];
    for t in 1..=10i64 {
        let from = w.r.events.len();
        w.step(Some(&view));
        // The alert of `HurtByTargetGoal` is an event the game applies; the ECS side applies it itself.
        for ev in &w.r.events[from..] {
            if let horizon_client_lib::mobs::MobEvent::SetTarget { mob, .. } = ev
                && *mob as usize == hm + 1
            {
                ticks[2].0.get_or_insert(t);
            }
        }
        let now = [
            (w.mob_target(zm) == Some(MWho::Mob(cow)), w.ecs_target(ze) == Some(Who::Entity(pe))),
            (w.mob_target(gm) == Some(MWho::Mob(other)), w.ecs_target(ge) == Some(Who::Entity(qe))),
            (false, w.ecs_target(he) == Some(Who::Entity(qe))),
        ];
        for (k, (m, e)) in now.into_iter().enumerate() {
            if m {
                ticks[k].0.get_or_insert(t);
            }
            if e {
                ticks[k].1.get_or_insert(t);
            }
        }
    }
    eprintln!("(mob tick, ecs tick): hunting zombie takes the cow {:?}; idle zombie takes its hitter {:?}; its neighbour is alerted {:?}", ticks[0], ticks[1], ticks[2]);
    for (k, what) in ["hunting zombie", "idle zombie", "alerted neighbour"].iter().enumerate() {
        let (m, e) = ticks[k];
        assert!(m.is_some() && e.is_some(), "{what}: mob {m:?}, ecs {e:?}");
        assert!((m.unwrap() - e.unwrap()).abs() <= 2, "{what}: mob {m:?}, ecs {e:?}");
    }
}
