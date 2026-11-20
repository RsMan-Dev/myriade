//! The lot 4 (wolf, cat, ocelot, equids, creeper) against real `Mob`s of the species
//! (`mob_kind`): the laws of first occurrence and of durations, N draws per side, with ids far apart (PIEGES 46: the
//! Java generator's first draw is correlated between neighbouring seeds). `--nocapture` prints. Cow body only.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, Mob, MobEvent, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::Entry, cow::*, goals::*, targeting::*};
use std::sync::Arc;

fn leak<T>(v: T) -> &'static T {
    Box::leak(Box::new(v))
}
fn ecs_with(h: &mut Herd, entries: &'static [Entry], x: f64, z: f64, seed: u64, phase: u64) -> Entity {
    let mut sp = *species();
    sp.goals = horizon_ecs_proto::species::Config { entries };
    set_lazy_look(&mut h.world, false);
    // The ECS generator is a xorshift whose seed is `| 1` (900 and 901 are one seed) and whose first draws of a small seed
    // are small: well-mixed seeds only (PIEGES 46).
    let seed = (seed + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Phase>(e).unwrap().0 = phase;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
/// Ids far apart: the Mob's seed comes from its id.
fn far_id(j: usize) -> u32 {
    1 + (j as u32) * 7919 % 100_003
}
fn mob_add(r: &mut MobRef, kind: Arc<mobs::MobType>, at: Vec3, j: usize) -> usize {
    let mut m = Mob::new(far_id(j), kind, at, 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    r.mobs.push(m);
    r.searches.push(0);
    r.mobs.len() - 1
}
fn on() -> bool {
    if !have_data() {
        return false;
    }
    let sp = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into());
    if sp != "minecraft:cow" {
        eprintln!("SKIP lot4_vs_mob: written for HORIZON_SPECIES=minecraft:cow, not {sp}");
        return false;
    }
    true
}
fn spot(j: usize, step: f64, row: usize) -> Vec3 {
    Vec3::new(-20.0 + step * (j % row) as f64 + 0.5, 1.0, -20.0 + step * (j / row) as f64 + 0.5)
}
fn near(j: usize) -> Vec3 {
    Vec3::new(-0.5 + 1.2 * (j % 8) as f64, 1.0, -3.5 + 1.2 * (j / 8) as f64)
}
/// A roof block over a column of the `Mob`'s world, with the light engine run on it (a raw `set_block` leaves the sky light as it
/// was: the `Mob` still sees the sky; the ECS's `BlockChanges` relights).
fn roof(r: &MobRef, x: i32, z: i32) {
    r.set_block(x, 6, z, "minecraft:stone");
    // SAFETY: as `MobRef::set_block`.
    unsafe {
        horizon_client_lib::mobs::world::relight_test_world(&mut *r.level.blocks.0, &[(x, 6, z)]);
    }
}
fn mean_of(v: &[Option<usize>]) -> (usize, f64) {
    let f: Vec<f64> = v.iter().filter_map(|x| x.map(|t| t as f64)).collect();
    (f.len(), f.iter().sum::<f64>() / f.len().max(1) as f64)
}

/// A block with properties in the world of the `Mob` (`MobRef`) and in the one of the ECS (the herd's level): the ECS can only
/// write names through `BlockChanges`, the controls that need a ripe bush write the state directly.
fn set_state(r: &MobRef, h: &Herd, x: i32, y: i32, z: i32, name: &str, props: &[(&str, &str)]) {
    let reg = r.w.registry;
    let base = reg.default_state(name).unwrap();
    let mut st = reg.state(base).unwrap().clone();
    for (k, v) in props {
        st.properties.insert((*k).into(), (*v).into());
    }
    let id = reg.id(&st).unwrap();
    for blocks in [r.level.blocks.0, h.world.resource::<Level>().blocks.0] {
        // SAFETY: the test thread owns the two worlds while it builds the scene (no reference to their blocks is alive).
        unsafe {
            let _ = (&mut *blocks).set(x, y, z, id);
        }
    }
}


/// The species of the registry with their `Mob`: the entry (name) and `Species` with all its goals.
fn ecs_named(h: &mut Herd, name: &str, entries: Option<&'static [Entry]>, x: f64, z: f64, seed: u64, phase: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name} is not in the registry"));
    if let Some(e) = entries {
        sp.goals = horizon_ecs_proto::species::Config { entries: e };
    }
    set_lazy_look(&mut h.world, false);
    let seed = (seed + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // `spawn_armed` admits the animal to the zone: with no player near it is born asleep; the benches keep it awake as `ecs_with` does.
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.get_mut::<Phase>(e).unwrap().0 = phase;
    h.world.resource_mut::<Entered>().0.push(e);
    // The kernel's stroll timer is armed for the animals of the build only: a spawned one is armed by hand (as `blocked` does).
    if h.world.get::<Stroll>(e).is_some() && h.world.get::<GoalSet>(e).is_some_and(|g| g.entries().iter().any(|en| en.module.name == "stroll")) {
        horizon_ecs_proto::behaviours::stroll::arm_now(&mut h.world, e);
    }
    e
}


fn ps_immortal(h: &mut Herd) {
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
}

const UUID: [i32; 4] = [1, 2, 3, 4];

/// Priority 1 (two porters' suspicion): in the `Mob`, `nav_move_to` asks a path with reach 1 and `MoveToBlock.tick` wants the feet within
/// 1 block of the centre above the block: does a real tame cat reach a bed in a pit (lie down), and does a real turtle with an egg lay it
/// on sand? Same scene on the ECS.
#[test]
fn reach_cat_bed_and_turtle_egg() {
    if !on() {
        return;
    }
    // Cat: on a 2-block platform (the search starts 2 rows down then 3 up: the row of the cat's own ground is never visited), a bed on the ground 3 blocks away.
    for j in 0..6 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let id = far_id(j + 1);
        let i = mob_add(&mut r, mob_kind("minecraft:cat"), Vec3::new(0.5, 3.0, 0.5), j + 1);
        mobs::tame::set_tame(&mut r.mobs[i], true);
        r.mobs[i].extra.owner = Some(UUID);
        r.mobs[i].extra.sitting = false;
        r.mobs[i].ai.no_action_time = 100;
        let e = ecs_named(&mut h, "minecraft:cat", None, 0.5, 0.5, 4000 + j as u64, u64::from(id));
        for x in -2..=1 {
            for z in -2..=2 {
                for y in 1..=2 {
                    r.set_block(x, y, z, "minecraft:stone");
                    h.set_block(x, y, z, "minecraft:stone");
                }
            }
        }
        h.teleport(e, Vec3::new(0.5, 3.0, 0.5));
        horizon_ecs_proto::tame::set_owner(&mut h.world, e, horizon_ecs_proto::targeting::Who::Player(1));
        for (x, y, name) in [(3, 0, "minecraft:red_bed")] {
            r.set_block(x, y, 0, name);
            h.set_block(x, y, 0, name);
        }
        let (mut tm, mut te) = (None, None);
        let (mut dm, mut de) = (f64::MAX, f64::MAX);
        // The owner is online (a tame cat whose owner is not loaded sits: `SitWhenOrderedToGoal`, owner null), on the platform.
        let owner = Vec3::new(-1.5, 3.0, 0.5);
        let mut view = player_view(owner, None);
        view.uuid = UUID;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: owner.x, y: owner.y, z: owner.z, main: 0, off: 0, ..Default::default() });
        for t in 0..800 {
            r.step(Some(&view));
            h.tick();
            let (pm, pe) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            dm = dm.min(((pm.x - 3.5).powi(2) + (pm.z - 0.5).powi(2)).sqrt());
            de = de.min(((pe.x - 3.5).powi(2) + (pe.z - 0.5).powi(2)).sqrt());
            if j == 0 && t % 100 == 0 {
                eprintln!("  t{t} mob goals {:?} tame {} sitting {} flag {}", r.mobs[i].ai.goals.running_names(), r.mobs[i].extra.tame, r.mobs[i].extra.sitting, r.mobs[i].extra.flag);
            }
            if tm.is_none() && r.mobs[i].extra.flag {
                tm = Some(t);
            }
            if te.is_none() && horizon_ecs_proto::behaviours::cat_state::pose(&h.world, e).lying {
                te = Some(t);
            }
        }
        eprintln!("cat bed #{j}: lies at tick mob {tm:?} ecs {te:?}; closest horizontal distance to the bed mob {dm:.2} ecs {de:.2}; end mob ({:.2},{:.2},{:.2})", r.mobs[i].position.x, r.mobs[i].position.y, r.mobs[i].position.z);
    }
    // Turtle: an egg, sand 6 to 14 blocks away.
    for j in 0..12 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let id = far_id(j + 1);
        let i = mob_add(&mut r, mob_kind("minecraft:turtle"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        r.mobs[i].extra.flag = true;
        let e = ecs_named(&mut h, "minecraft:turtle", None, 0.5, 0.5, 5000 + j as u64, u64::from(id));
        h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::lay_egg::Carrying);
        for x in 6..=14 {
            for z in -1..=1 {
                r.set_block(x, 0, z, "minecraft:sand");
                h.set_block(x, 0, z, "minecraft:sand");
            }
        }
        horizon_ecs_proto::behaviours::lay_egg::load(&mut h.world, e);
        let (mut tm, mut te) = (None, None);
        let (mut xm, mut xe) = (0.0f64, 0.0f64);
        for t in 0..1500 {
            r.step(None);
            h.tick();
            if (j == 1 || j == 2) && t % 150 == 0 {
                let m = &r.mobs[i];
                eprintln!("  turtle#{j} t{t} x {:.2} y {:.2} z {:.2} goals {:?} ecs x {:.2}", m.position.x, m.position.y, m.position.z, m.ai.goals.running_names(), h.world.get::<Pos>(e).unwrap().0.x);
            }
            xm = xm.max(r.mobs[i].position.x);
            xe = xe.max(h.world.get::<Pos>(e).unwrap().0.x);
            if tm.is_none() && !r.mobs[i].extra.flag {
                tm = Some(t);
            }
            if te.is_none() && h.world.get::<horizon_ecs_proto::behaviours::lay_egg::Carrying>(e).is_none() {
                te = Some(t);
            }
        }
        eprintln!("turtle egg #{j}: laid at tick mob {tm:?} ecs {te:?}; max x mob {xm:.2} ecs {xe:.2}");
    }
}

/// Probe of the cause: the `Mob`'s own `nav_move_to` (reach 1) to the centre of a free cell 6 blocks away, by species, without any goal.
#[test]
fn nav_move_to_end_point_by_species() {
    if !on() {
        return;
    }
    for name in ["minecraft:cat", "minecraft:turtle", "minecraft:cow", "minecraft:zombie"] {
        let mut ends = Vec::new();
        for j in 0..4 {
            let mut r = MobRef::new(24, false);
            let i = mob_add(&mut r, mob_kind(name), Vec3::new(0.5, 1.0, 0.5), j + 1);
            r.mobs[i].ai.goals.clear();
            r.step(None);
            r.step(None);
            let ok = r.mobs[i].nav_move_to(r.w, [6.5, 1.0, 0.5], 1.0);
            for _ in 0..500 {
                r.step(None);
            }
            let p = r.mobs[i].position;
            ends.push(format!("{ok} ({:.2},{:.2})", p.x, p.z));
        }
        eprintln!("nav_move_to (6.5,1,0.5) from (0.5,1,0.5): {name}: {}", ends.join("  "));
    }
}

/// `CatSitOnBlockGoal` on flat ground (the row of the cat's own ground, visited by its search): a bed 6 blocks away, flush with the floor.
/// Does the `Mob` cat sit on it (`sitting_pose`), and the ECS cat?
#[test]
fn cat_sits_on_a_flat_bed() {
    if !on() {
        return;
    }
    for j in 0..6 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let id = far_id(j + 1);
        let i = mob_add(&mut r, mob_kind("minecraft:cat"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        mobs::tame::set_tame(&mut r.mobs[i], true);
        r.mobs[i].extra.owner = Some(UUID);
        let e = ecs_named(&mut h, "minecraft:cat", None, 0.5, 0.5, 6000 + j as u64, u64::from(id));
        horizon_ecs_proto::tame::set_owner(&mut h.world, e, horizon_ecs_proto::targeting::Who::Player(1));
        r.set_block(6, 0, 0, "minecraft:red_bed");
        h.set_block(6, 0, 0, "minecraft:red_bed");
        let owner = Vec3::new(-1.5, 1.0, 0.5);
        let mut view = player_view(owner, None);
        view.uuid = UUID;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: owner.x, y: owner.y, z: owner.z, main: 0, off: 0, ..Default::default() });
        let (mut tm, mut te, mut dm, mut de) = (None, None, f64::MAX, f64::MAX);
        for t in 0..1500 {
            r.step(Some(&view));
            h.tick();
            let (pm, pe) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            dm = dm.min(((pm.x - 6.5).powi(2) + (pm.z - 0.5).powi(2)).sqrt());
            de = de.min(((pe.x - 6.5).powi(2) + (pe.z - 0.5).powi(2)).sqrt());
            if tm.is_none() && r.mobs[i].ai.goals.running_names().iter().any(|n| n.contains("CatSitOnBlock")) && r.mobs[i].extra.sitting_pose {
                tm = Some(t);
            }
            if te.is_none() && horizon_ecs_proto::behaviours::cat_state::pose(&h.world, e).sitting_pose {
                te = Some(t);
            }
        }
        eprintln!("cat sits on flat bed #{j}: sits at tick mob {tm:?} ecs {te:?}; closest to the bed mob {dm:.2} ecs {de:.2}");
    }
}

/// Priority 2: a wild cat refuses `CatLieOnBlock` (not tame); the `Mob` asks again every other tick, the ECS asks again when `tame` or
/// the order to stand wakes the goal. The tick (after the taming at tick 4, before the ECS cat's own stroll can take it off the platform) at which the cat lies in the bed, on each side.
#[test]
fn cat_lies_down_after_taming() {
    if !on() {
        return;
    }
    let (mut sm, mut se, mut n) = (0.0, 0.0, 0.0);
    for j in 0..10 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let id = far_id(j + 1);
        let i = mob_add(&mut r, mob_kind("minecraft:cat"), Vec3::new(0.5, 3.0, 0.5), j + 1);
        let e = ecs_named(&mut h, "minecraft:cat", None, 0.5, 0.5, 7000 + j as u64, u64::from(id));
        for x in -2..=1 {
            for z in -2..=2 {
                for y in 1..=2 {
                    r.set_block(x, y, z, "minecraft:stone");
                    h.set_block(x, y, z, "minecraft:stone");
                }
            }
        }
        h.teleport(e, Vec3::new(0.5, 3.0, 0.5));
        r.set_block(3, 0, 0, "minecraft:red_bed");
        h.set_block(3, 0, 0, "minecraft:red_bed");
        let owner = Vec3::new(-1.5, 3.0, 0.5);
        let mut view = player_view(owner, None);
        view.uuid = UUID;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: owner.x, y: owner.y, z: owner.z, main: 0, off: 0, ..Default::default() });
        let (mut tm, mut te) = (None, None);
        for t in 0..600 {
            if t == 4 {
                mobs::tame::set_tame(&mut r.mobs[i], true);
                r.mobs[i].extra.owner = Some(UUID);
                r.mobs[i].extra.sitting = false;
                horizon_ecs_proto::tame::tame(&mut h.world, e, horizon_ecs_proto::targeting::Who::Player(1), 10.0);
                horizon_ecs_proto::tame::order_sit(&mut h.world, e, false);
            }
            r.step(Some(&view));
            h.tick();
            if j == 2 && (t == 5 || t == 20 || t == 40 || t == 60 || t == 100) {
                let g = h.world.get::<GoalSet>(e).unwrap();
                let names: Vec<_> = g.entries().iter().filter(|en| g.is_running(en.module)).map(|en| en.module.name).collect();
                eprintln!("  t{t} ecs goals {names:?} pos {:.2},{:.2},{:.2}", h.world.get::<Pos>(e).unwrap().0.x, h.world.get::<Pos>(e).unwrap().0.y, h.world.get::<Pos>(e).unwrap().0.z);
            }
            if tm.is_none() && r.mobs[i].extra.flag {
                tm = Some(t - 4);
            }
            if te.is_none() && horizon_ecs_proto::behaviours::cat_state::pose(&h.world, e).lying {
                te = Some(t - 4);
            }
        }
        eprintln!("cat lies after taming #{j}: ticks after taming mob {tm:?} ecs {te:?}");
        if let (Some(a), Some(b)) = (tm, te) {
            sm += a as f64;
            se += b as f64;
            n += 1.0;
        }
    }
    eprintln!("cat lies after taming: mean mob {:.1} ecs {:.1} over {n} cats", sm / n, se / n);
    assert!(n >= 9.0, "the ECS cat lies down after the taming as the Mob's does");
}

/// Priority 3: the ocelot's strike cadence (`hostile::strike` is the common blow): an ocelot and a chicken 3 blocks away; the ticks at
/// which the chicken loses health (first and second blow), on each side, over 12 draws.
#[test]
fn ocelot_strikes_a_chicken() {
    if !on() {
        return;
    }
    let (mut a1, mut a2, mut b1, mut b2, mut na, mut nb) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for j in 0..12 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let id = far_id(j + 1);
        let oi = mob_add(&mut r, mob_kind("minecraft:ocelot"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        r.mobs[oi].ai.no_action_time = 0;
        let ci = mob_add(&mut r, mob_kind("minecraft:chicken"), Vec3::new(3.5, 1.0, 0.5), j + 40);
        let oe = ecs_named(&mut h, "minecraft:ocelot", None, 0.5, 0.5, 8000 + j as u64, u64::from(id));
        let ce = ecs_named(&mut h, "minecraft:chicken", None, 3.5, 0.5, 9000 + j as u64, u64::from(far_id(j + 40)));
                let (mut hm, mut he): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
        let (mut lm, mut le) = (r.mobs[ci].health, h.world.get::<horizon_ecs_proto::cow::Health>(ce).map_or(0.0, |x| x.0));
        let mut evn = r.events.len();
        let (mut tgm, mut tge) = (None, None);
        for t in 0..400 {
            r.step(None);
            h.tick();
            let (cm, ceh) = (r.mobs[ci].health, h.world.get::<horizon_ecs_proto::cow::Health>(ce).map_or(0.0, |x| x.0));
            for ev in &r.events[evn..] {
                if matches!(ev, MobEvent::AttackMob { .. } | MobEvent::DamageMob { .. }) {
                    hm.push(t);
                }
            }
            evn = r.events.len();
            if tgm.is_none() && r.mobs[oi].ai.target.is_some() {
                tgm = Some(t);
            }
            if tge.is_none() && h.world.get::<Target>(oe).is_some() {
                tge = Some(t);
            }
            let _ = cm;
            if ceh < le {
                he.push(t);
            }
            lm = cm;
            le = ceh;
        }
        let _ = oi;
        assert!(hm.windows(2).all(|w| w[1] - w[0] == 20) && he.windows(2).all(|w| w[1] - w[0] == 20), "the blows come every 20 ticks on both sides");
        eprintln!("ocelot strikes #{j}: target at mob {tgm:?} ecs {tge:?}; first blows mob {:?} ecs {:?}", hm.first(), he.first());
        if hm.len() >= 2 {
            a1 += hm[0] as f64;
            a2 += hm[1] as f64;
            na += 1.0;
        }
        if he.len() >= 2 {
            b1 += he[0] as f64;
            b2 += he[1] as f64;
            nb += 1.0;
        }
    }
    eprintln!("ocelot: first blow mean mob {:.1} ecs {:.1}; second mob {:.1} ecs {:.1} (draws with two blows: mob {na} ecs {nb})", a1 / na, b1 / nb, a2 / na, b2 / nb);
}

/// Priority 4: the wolf's `BegGoal`: a player 3 blocks away holds a bone; the ticks the goal runs (first run), on each side, over 16 draws
/// (the duration is `reduced(40 + nextInt(40))`: 20 to 39 asks, 40 to 78 ticks... the law is compared by its mean and range).
#[test]
fn wolf_begs_for_a_bone() {
    if !on() {
        return;
    }
    let bone = horizon_ecs_proto::goals::item_id("minecraft:bone");
    let (mut sm, mut se, mut n) = (0.0, 0.0, 0.0);
    for j in 0..16 {
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let id = far_id(j + 1);
        let i = mob_add(&mut r, mob_kind("minecraft:wolf"), Vec3::new(0.5, 1.0, 0.5), j + 1);
        let e = ecs_named(&mut h, "minecraft:wolf", None, 0.5, 0.5, 10_000 + j as u64, u64::from(id));
        let at = Vec3::new(3.5, 1.0, 0.5);
        let view = player_view(at, Some("minecraft:bone"));
        ps_immortal(&mut h);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: bone, off: 0, ..Default::default() });
        let (mut a, mut b, mut ga, mut gb) = (0usize, 0usize, 0usize, 0usize);
        let (mut pa, mut pb) = (false, false);
        for _ in 0..200 {
            r.step(Some(&view));
            h.tick();
            let mob_runs = r.mobs[i].ai.goals.running_names().iter().any(|n| n.contains("Beg"));
            let g = h.world.get::<GoalSet>(e).unwrap();
            let ecs_runs = g.entries().iter().any(|en| en.module.name == "beg" && g.is_running(en.module));
            a += usize::from(mob_runs);
            b += usize::from(ecs_runs);
            ga += usize::from(pa && !mob_runs);
            gb += usize::from(pb && !ecs_runs);
            (pa, pb) = (mob_runs, ecs_runs);
        }
        eprintln!("wolf beg #{j}: ticks running in 200: mob {a} ecs {b}; ends: mob {ga} ecs {gb}");
        {
            sm += a as f64;
            se += b as f64;
            n += 1.0;
        }
    }
    eprintln!("wolf beg: mean mob {:.1} ecs {:.1} over {n} draws (ticks running in 200)", sm / n, se / n);
}

/// One startup check per species of the lot: it spawns, ticks 40 times with a player near and stays alive, and its goals run.
#[test]
fn the_species_of_lot_4_start() {
    if !on() {
        return;
    }
    for name in ["minecraft:wolf", "minecraft:cat", "minecraft:ocelot", "minecraft:creeper", "minecraft:horse", "minecraft:donkey", "minecraft:mule", "minecraft:skeleton_horse", "minecraft:zombie_horse", "minecraft:llama"] {
        let mut h = Herd::with_slab(24, false, false);
        let e = ecs_named(&mut h, name, None, 0.5, 0.5, 31, 3);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 4.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        for _ in 0..40 {
            h.tick();
        }
        assert!(h.world.get_entity(e).is_ok() && h.world.get::<GoalSet>(e).is_some_and(|g| g.len() > 0), "{name} starts");
    }
}
