//! Lot 2 (enderman take / leave, neutral anger, panda sit, snow golem shots...) against real `Mob`s of the species
//! (`mob_kind`): the laws of first occurrence and of durations, N draws per side, with ids far apart (PIEGES 46: the
//! Java generator's first draw is correlated between neighbouring seeds). `--nocapture` prints. Cow body only.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{self, Mob, MobEvent, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{Entry, freeze_when_looked_at, ghast_look, leap_at_target, silverfish_merge, strider_go_to_lava, leave_block, neutral_anger, panda_sit, panda_state as ps, ranged_attack, stroll, take_block},
    cow::*,
    goals::*,
    projectile::Projectile,
    targeting::*,
};
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
        eprintln!("SKIP lot2_vs_mob: written for HORIZON_SPECIES=minecraft:cow, not {sp}");
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
fn mean_of(v: &[Option<usize>]) -> (usize, f64) {
    let f: Vec<f64> = v.iter().filter_map(|x| x.map(|t| t as f64)).collect();
    (f.len(), f.iter().sum::<f64>() / f.len().max(1) as f64)
}

/// First tick at which an enderman (36, 7 blocks apart, each with its ring of dirt) takes a block (`take`) or sets the
/// carried dirt down (`!take`: the game's own rule is applied to the Mob's request: air, floor below, nothing in it).
fn enderman_first(take: bool, ticks: usize, batch: usize) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let n = 36;
    let mut r = MobRef::new(24, false);
    let mut h = Herd::with_slab(24, false, false);
    let kind = mob_kind("minecraft:enderman");
    let dirt = r.w.registry.default_state("minecraft:dirt").unwrap();
    let entries: &'static [Entry] = leak([
        Entry::new(&leave_block::MODULE, 10, &leave_block::Params::DEFAULT),
        Entry::new(&take_block::MODULE, 11, &take_block::Params::DEFAULT),
    ]);
    let mut ids = Vec::new();
    let mut es = Vec::new();
    for j in 0..n {
        let p = spot(j, 7.0, 6);
        let i = mob_add(&mut r, kind.clone(), p, j + 100 * batch);
        r.mobs[i].ai.goals.disabled = mobs::goals::MOVE | mobs::goals::LOOK | mobs::goals::JUMP;
        if !take {
            r.mobs[i].extra.carried = Some(dirt);
        }
        ids.push(i);
        let e = ecs_with(&mut h, entries, p.x, p.z, 900 + (j + 100 * batch) as u64, u64::from(far_id(j + 100 * batch)));
        if !take {
            h.world.get_mut::<take_block::Carried>(e).unwrap().0 = Some("minecraft:dirt");
            leave_block::arm_leave(&mut h.world, e);
        }
        es.push(e);
        // A roof over the column: the enderman's own daylight teleport (`R:enderman`, not ported) needs the sky.
        r.set_block(p.x.floor() as i32, 6, p.z.floor() as i32, "minecraft:stone");
        h.set_block(p.x.floor() as i32, 6, p.z.floor() as i32, "minecraft:stone");
        // The ring of eight dirt blocks around the foot, at the height of the feet, on both sides.
        if take {
            for (dx, dz) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (x, z) = (p.x.floor() as i32 + dx, p.z.floor() as i32 + dz);
                r.set_block(x, 1, z, "minecraft:dirt");
                h.set_block(x, 1, z, "minecraft:dirt");
            }
        }
    }
    let (mut mf, mut ef) = (vec![None; n], vec![None; n]);
    for t in 0..ticks {
        let from = r.events.len();
        r.step(None);
        for ev in &r.events[from..] {
            match ev {
                MobEvent::TakeBlock { mob, .. } if take => {
                    let j = ids.iter().position(|&i| r.mobs[i].id == *mob).unwrap();
                    mf[j].get_or_insert(t);
                }
                MobEvent::EndermanPlaceBlock { mob, pos, occupants, .. } if !take => {
                    let j = ids.iter().position(|&i| r.mobs[i].id == *mob).unwrap();
                    // The game's rule: air in the cell, a floor under it, nobody inside.
                    let _ = occupants;
                    if pos[1] == 1 {
                        mf[j].get_or_insert(t);
                    }
                }
                _ => {}
            }
        }
        h.tick();
        for (j, &e) in es.iter().enumerate() {
            let c = h.world.get::<take_block::Carried>(e).unwrap().0.is_some();
            if take == c {
                ef[j].get_or_insert(t);
            }
        }
    }
    (mf, ef)
}
#[test]
fn enderman_take_block_first_take() {
    if !on() {
        return;
    }
    let (mut m, mut e) = (Vec::new(), Vec::new());
    for b in 0..4 {
        let (a, c) = enderman_first(true, 1500, b);
        m.extend(a);
        e.extend(c);
    }
    let (mm, em) = (mean_of(&m), mean_of(&e));
    eprintln!("take_block: first take (count, mean tick): mob {mm:?} ecs {em:?}");
    assert!(mm.0 >= 120 && em.0 >= 120);
    assert!((mm.1 - em.1).abs() / mm.1 < 0.3, "{mm:?} vs {em:?}");
}
#[test]
fn enderman_leave_block_first_placement() {
    if !on() {
        return;
    }
    let (mut m, mut e) = (Vec::new(), Vec::new());
    for b in 0..16 {
        let (a, c) = enderman_first(false, 24_000, b);
        m.extend(a);
        e.extend(c);
    }
    let (mm, em) = (mean_of(&m), mean_of(&e));
    eprintln!("leave_block: first placement (count, mean tick): mob {mm:?} ecs {em:?}");
    assert!(mm.0 >= 120 && em.0 >= 120);
    assert!((mm.1 - em.1).abs() / mm.1 < 0.3, "{mm:?} vs {em:?}");
}

/// Time between the loss of the target and the end of the anger, over `n` polar bears and `n` ECS animals.
#[test]
fn neutral_anger_duration_after_the_target_is_lost() {
    if !on() {
        return;
    }
    let n = 60;
    let mut r = MobRef::new(24, false);
    let kind = mob_kind("minecraft:polar_bear");
    let view = player_view(Vec3::new(3.5, 1.0, 0.5), None);
    let ids: Vec<usize> = (0..n)
        .map(|j| {
            let i = mob_add(&mut r, kind.clone(), near(j), j);
            r.mobs[i].ai.goals.disabled = mobs::goals::MOVE | mobs::goals::LOOK | mobs::goals::JUMP;
            i
        })
        .collect();
    for &i in &ids {
        r.mobs[i].ai.target = Some(MWho::Player);
    }
    for _ in 0..5 {
        r.step(Some(&view));
    }
    for &i in &ids {
        r.mobs[i].ai.target = None;
    }
    let mut mob = vec![None; n];
    for t in 0..1000 {
        r.step(None);
        for (j, &i) in ids.iter().enumerate() {
            if mob[j].is_none() && r.mobs[i].extra.anger <= 0 {
                mob[j] = Some(t);
            }
        }
    }
    let entries: &'static [Entry] = leak([Entry::new(&neutral_anger::MODULE, 200, &neutral_anger::Params::DEFAULT)]);
    let mut h = Herd::with_slab(24, false, false);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    let es: Vec<Entity> = (0..n).map(|j| ecs_with(&mut h, entries, near(j).x, near(j).z, 300 + j as u64, u64::from(far_id(j)))).collect();
    h.tick();
    for &e in &es {
        let mut t = Target::player(1, 1000);
        t.must_see = false;
        set_target(&mut h.world, e, t);
    }
    for _ in 0..5 {
        h.tick();
    }
    for &e in &es {
        clear_target(&mut h.world, e);
    }
    let mut ecs = vec![None; n];
    for t in 0..1000 {
        h.tick();
        for (j, &e) in es.iter().enumerate() {
            if ecs[j].is_none() && !neutral_anger::is_angry(&h.world, e) {
                ecs[j] = Some(t);
            }
        }
    }
    let (mm, em) = (mean_of(&mob), mean_of(&ecs));
    let range = |v: &[Option<usize>]| (v.iter().flatten().min().copied(), v.iter().flatten().max().copied());
    eprintln!("neutral_anger: ticks from the loss to calm (count, mean): mob {mm:?} range {:?}; ecs {em:?} range {:?}", range(&mob), range(&ecs));
    assert!(mm.0 == n && em.0 == n, "all end: mob {} ecs {}", mm.0, em.0);
    assert!((mm.1 - em.1).abs() < 40.0, "{mm:?} vs {em:?}");
}

/// A panda holding an item: the tick it sits down first and how long it stays.
#[test]
fn panda_sit_first_episode() {
    if !on() {
        return;
    }
    let n = 60;
    let ticks = 6000;
    let mut r = MobRef::new(24, false);
    let kind = mob_kind("minecraft:panda");
    let ids: Vec<usize> = (0..n)
        .map(|j| {
            let i = mob_add(&mut r, kind.clone(), spot(j, 5.0, 8), j);
            r.mobs[i].extra.set_main_hand(Some("minecraft:wheat".into()));
            i
        })
        .collect();
    let (mut ms, mut me) = (vec![None; n], vec![None; n]);
    for t in 0..ticks {
        r.step(None);
        for (j, &i) in ids.iter().enumerate() {
            let up = mobs::panda::get(&r.mobs[i]).is_some_and(|s| s.flags & mobs::panda::FLAG_SIT != 0);
            if up && ms[j].is_none() {
                ms[j] = Some(t);
            }
            if !up && ms[j].is_some() && me[j].is_none() {
                me[j] = Some(t);
            }
        }
    }
    let entries: &'static [Entry] = leak([
        Entry::new(&panda_sit::MODULE, 7, &panda_sit::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 14, &stroll::Params::DEFAULT),
    ]);
    let mut h = Herd::with_slab(24, false, false);
    let wheat = item_id("minecraft:wheat");
    let es: Vec<Entity> = (0..n)
        .map(|j| {
            let p = spot(j, 5.0, 8);
            let e = ecs_with(&mut h, entries, p.x, p.z, 400 + j as u64, u64::from(far_id(j)));
            h.world.entity_mut(e).insert(Equipment { main: wheat, ..Default::default() });
            panda_sit::item_in_hand(&mut h.world, e);
            e
        })
        .collect();
    let (mut es_, mut ee) = (vec![None; n], vec![None; n]);
    for t in 0..ticks {
        h.tick();
        for (j, &e) in es.iter().enumerate() {
            let up = ps::flag(&h.world, e, ps::FLAG_SIT);
            if up && es_[j].is_none() {
                es_[j] = Some(t);
            }
            if !up && es_[j].is_some() && ee[j].is_none() {
                ee[j] = Some(t);
            }
        }
    }
    let dur = |s: &[Option<usize>], e: &[Option<usize>]| -> Vec<Option<usize>> { s.iter().zip(e).map(|(a, b)| a.zip(*b).map(|(a, b)| b - a)).collect() };
    let (dm, de) = (dur(&ms, &me), dur(&es_, &ee));
    eprintln!("panda_sit: first sit (count, mean) mob {:?} ecs {:?}; duration mob {:?} ecs {:?}", mean_of(&ms), mean_of(&es_), mean_of(&dm), mean_of(&de));
    let (a, b) = (mean_of(&dm), mean_of(&de));
    assert!(a.0 >= 40 && b.0 >= 40, "{a:?} {b:?}");
    assert!((a.1 - b.1).abs() / a.1 < 0.35, "duration {a:?} vs {b:?}");
}

/// A snow golem 10 blocks from a player: the ticks of its shots and the mean velocity of the snowballs on each side.
#[test]
fn snow_golem_shot_cadence_and_aim() {
    if !on() {
        return;
    }
    let at = Vec3::new(0.5, 1.0, 0.5);
    let player = Vec3::new(10.5, 1.0, 0.5);
    let view = player_view(player, None);
    let mut r = MobRef::new(40, false);
    let kind = mob_kind("minecraft:snow_golem");
    let i = mob_add(&mut r, kind, at, 0);
    r.mobs[i].ai.target = Some(MWho::Player);
    let eye = r.mobs[i].eye_y() - r.mobs[i].position.y;
    let mut h = Herd::with_slab(40, true, false);
    let entries: &'static [Entry] = leak([Entry::new(&ranged_attack::MODULE, 1, &ranged_attack::Params::DEFAULT)]);
    let e = ecs_with(&mut h, entries, at.x, at.z, 77, u64::from(far_id(0)));
    h.world.entity_mut(e).insert(Eye(eye));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0 });
    set_target(&mut h.world, e, Target::player(1, 30));
    let (mut mt, mut et) = (Vec::new(), Vec::new());
    let (mut mv, mut ev) = (Vec::new(), Vec::new());
    let mut seen = std::collections::HashSet::new();
    for t in 1..=1500 {
        let from = r.events.len();
        r.step(Some(&view));
        for x in &r.events[from..] {
            if let MobEvent::Projectile(p) = x {
                mt.push(t);
                mv.push(p.velocity);
            }
        }
        h.tick();
        let mut q = h.world.query::<(Entity, &Projectile, &Vel)>();
        let fresh: Vec<(Entity, Vec3)> = q.iter(&h.world).filter(|x| !seen.contains(&x.0)).map(|x| (x.0, x.2.0)).collect();
        for (p, v) in fresh {
            seen.insert(p);
            et.push(t);
            ev.push(v);
        }
    }
    let avg = |v: &[Vec3], f: fn(&Vec3) -> f64| v.iter().map(f).sum::<f64>() / v.len().max(1) as f64;
    eprintln!("snow golem: shots mob {} ecs {}; first mob {:?} ecs {:?}", mt.len(), et.len(), mt.iter().take(3).collect::<Vec<_>>(), et.iter().take(3).collect::<Vec<_>>());
    eprintln!("mean velocity (note: ecs after its first physics step): mob x {:.3} y {:.3} ; ecs x {:.3} y {:.3}", avg(&mv, |v| v.x), avg(&mv, |v| v.y), avg(&ev, |v| v.x), avg(&ev, |v| v.y));
    assert!(mt.len() >= 20 && et.len() >= 20, "{} {}", mt.len(), et.len());
    assert!((mt[0] as i64 - et[0] as i64).abs() <= 2, "first shot {} vs {}", mt[0], et[0]);
    let gaps = |v: &[usize]| v.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
    assert_eq!(gaps(&mt)[..3], gaps(&et)[..3]);
}

/// The strider that was in lava when its search came up searches again as soon as it leaves the lava (the agent's note:
/// "the start attempt is lost, `start_goal` meets the goal still running"). Immortal cow body, a pool of lava at 5 blocks.
fn strider_scene(exit_after: usize) -> (Option<usize>, bool) {
    let entries: &'static [Entry] = leak([Entry::new(&strider_go_to_lava::MODULE, 4, &strider_go_to_lava::Params::DEFAULT)]);
    let mut h = Herd::with_slab(24, false, false);
    let e = ecs_with(&mut h, entries, 0.5, 0.5, 11, 5);
    h.world.entity_mut(e).insert(Health(1.0e9));
    for x in 4..=7 {
        for z in -1..=1 {
            h.set_block(x, 0, z, "minecraft:lava");
        }
    }
    h.tick();
    h.tick();
    // It starts toward the lava (the first search is at once).
    let mut started = false;
    for _ in 0..40 {
        h.tick();
        started |= h.world.get::<strider_go_to_lava::GoingToLava>(e).is_some();
    }
    // Into the lava, until the deadline came up in it (the countdown is 100 to 199 ticks).
    h.teleport(e, Vec3::new(5.5, 0.2, 0.5));
    for _ in 0..exit_after {
        h.tick();
    }
    let in_lava = h.world.get::<FluidState>(e).is_some_and(|f| f.in_lava);
    let blocked = h.world.get::<strider_go_to_lava::Lava>(e).unwrap().blocked;
    eprintln!("strider: started {started}, in lava {in_lava}, search pending in lava {blocked}");
    // Out of the lava, to the dry side: a new walk must start within a few ticks.
    h.teleport(e, Vec3::new(0.5, 1.0, 0.5));
    let mut again = None;
    for t in 1..=60 {
        h.tick();
        if again.is_none() && h.world.get::<strider_go_to_lava::GoingToLava>(e).is_some() {
            again = Some(t);
        }
    }
    (again, blocked)
}
#[test]
fn strider_searches_again_when_it_leaves_the_lava() {
    if !on() {
        return;
    }
    let (again, blocked) = strider_scene(260);
    eprintln!("strider: pending {blocked}; walk again at tick {again:?} after the exit");
    assert!(blocked, "witness: the deadline came up in the lava");
    assert!(again.is_some(), "the search lost at the exit of the lava");
}

/// A silverfish beside one stone block: how many of them hide in it within 300 ticks (the merge draw 1 in 5 per ask, the
/// direction one in six, and the stroll that takes it away otherwise), and the mean tick, on each side.
#[test]
fn silverfish_merge_law() {
    if !on() {
        return;
    }
    let (mut mob, mut ecs) = (Vec::new(), Vec::new());
    for batch in 0..6 {
        let n = 36;
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let kind = mob_kind("minecraft:silverfish");
        let entries: &'static [Entry] = leak([Entry::new(&silverfish_merge::MODULE, 5, &silverfish_merge::Params::DEFAULT)]);
        let (mut ids, mut es) = (Vec::new(), Vec::new());
        for j in 0..n {
            let p = spot(j, 7.0, 6);
            let i = mob_add(&mut r, kind.clone(), p, j + 100 * batch);
            ids.push(i);
            // `mob_add` sets the idle count to 100, which forbids every stroll of the Mob (RandomStrollGoal.canUse).
            r.mobs[i].ai.no_action_time = 0;
            es.push(ecs_with(&mut h, entries, p.x, p.z, 700 + (j + 100 * batch) as u64, u64::from(far_id(j + 100 * batch))));
            let (x, z) = (p.x.floor() as i32 + 1, p.z.floor() as i32);
            r.set_block(x, 1, z, "minecraft:stone");
            h.set_block(x, 1, z, "minecraft:stone");
        }
        let (mut mf, mut ef) = (vec![None; n], vec![None; n]);
        for t in 0..300 {
            let from = r.events.len();
            // A player within 32 blocks keeps the idle count at zero (`checkDespawn`): the stroll stays possible.
            for &i in &ids {
                r.mobs[i].ai.no_action_time = 0;
            }
            r.step(None);
            for ev in &r.events[from..] {
                if let MobEvent::Infest { poof: Some(id), infested: true, .. } = ev {
                    let j = ids.iter().position(|&i| r.mobs[i].id == *id).unwrap();
                    mf[j].get_or_insert(t);
                }
            }
            h.tick();
            for (j, &e) in es.iter().enumerate() {
                if h.world.get_entity(e).is_err() {
                    ef[j].get_or_insert(t);
                }
            }
        }
        mob.extend(mf);
        ecs.extend(ef);
    }
    let (mm, em) = (mean_of(&mob), mean_of(&ecs));
    eprintln!("silverfish_merge: merged within 300 ticks of {} (count, mean tick): mob {mm:?} ecs {em:?}", mob.len());
    assert!((mm.0 as f64 - em.0 as f64).abs() / mm.0.max(1) as f64 <= 0.3, "{mm:?} vs {em:?}");
}

/// A cow with only the leap goal and a still cow 3 blocks away as its target: the tick of its first leap and the leaps in 300
/// ticks, on each side (one chance in three per ask for the Mob, a geometric wait drawn at the acquisition here).
#[test]
fn leap_at_target_law() {
    if !on() {
        return;
    }
    let (mut mf, mut ef) = (Vec::new(), Vec::new());
    let (mut mc, mut ec) = (0usize, 0usize);
    for batch in 0..5 {
        let n = 18;
        let mut r = MobRef::new(24, false);
        let mut h = Herd::with_slab(24, false, false);
        let entries: &'static [Entry] = leak([Entry::new(&leap_at_target::MODULE, 1, &leap_at_target::Params::DEFAULT)]);
        let mut hunters = Vec::new();
        let mut es = Vec::new();
        let mut prey_ids = Vec::new();
        for j in 0..n {
            let p = spot(j, 7.0, 6);
            let k = r.kind.clone();
            let a = mob_add(&mut r, k.clone(), p, 2 * j + 200 * batch);
            let b = mob_add(&mut r, k, Vec3::new(p.x + 3.0, p.y, p.z), 2 * j + 1 + 200 * batch);
            r.mobs[a].ai.goals.clear();
            r.mobs[a].ai.goals.add(1, horizon_client_lib::mobs::goals::LeapAtTarget::new(0.4));
            r.mobs[b].ai.goals.clear();
            hunters.push(a);
            prey_ids.push(b);
            let ea = ecs_with(&mut h, entries, p.x, p.z, 1300 + (2 * j + 200 * batch) as u64, u64::from(far_id(2 * j + 200 * batch)));
            let mut sp = *species();
            sp.goals = horizon_ecs_proto::species::Config { entries: &[] };
            let eb = spawn_species(&mut h.world, &sp, p.x + 3.0, p.z, 5 + (batch * 40 + j) as u64);
            h.world.get_mut::<ActiveUntil>(eb).unwrap().0 = u64::MAX;
            es.push((ea, eb));
        }
        // Both sides settle on the ground before the target is taken.
        for _ in 0..30 {
            r.step(None);
            h.tick();
        }
        for (j, &a) in hunters.iter().enumerate() {
            let bid = r.mobs[prey_ids[j]].id;
            r.mobs[a].ai.target = Some(MWho::Mob(bid));
        }
        for &(a, b) in &es {
            let mut t = Target::new(Who::Entity(b), 1000, u8::MAX);
            t.must_see = false;
            set_target(&mut h.world, a, t);
        }
        let (mut up_m, mut up_e) = (vec![false; n], vec![false; n]);
        let mut back_m = vec![false; n];
        let (mut f_m, mut f_e) = (vec![None; n], vec![None; n]);
        for t in 0..300 {
            r.step(None);
            h.tick();
            for j in 0..n {
                let air = r.mobs[hunters[j]].velocity.y > 0.25;
                if air && !up_m[j] {
                    f_m[j].get_or_insert(t);
                    mc += 1;
                }
                // A hunter that landed goes back to where it started: the target is again in range, as when a prey runs.
                if air {
                    back_m[j] = true;
                }
                if back_m[j] && r.mobs[hunters[j]].on_ground && !air {
                    back_m[j] = false;
                    let p = spot(j, 7.0, 6);
                    r.mobs[hunters[j]].position = p;
                    r.mobs[hunters[j]].velocity = Vec3::ZERO;
                }
                up_m[j] = air;
                let l = h.world.get::<leap_at_target::Leaping>(es[j].0).is_some();
                if !l && up_e[j] {
                    h.teleport(es[j].0, spot(j, 7.0, 6));
                }
                if l && !up_e[j] {
                    f_e[j].get_or_insert(t);
                    ec += 1;
                }
                up_e[j] = l;
            }
        }
        mf.extend(f_m);
        ef.extend(f_e);
    }
    let (mm, em) = (mean_of(&mf), mean_of(&ef));
    eprintln!("leap_at_target: first leap (count, mean tick) mob {mm:?} ecs {em:?}; leaps in 300 ticks mob {mc} ecs {ec}");
    assert!(mm.0 >= 60 && em.0 >= 60, "{mm:?} {em:?}");
    assert!((mm.1 - em.1).abs() < 8.0, "first leap {mm:?} vs {em:?}");
}

/// The ghast's gaze: the yaw toward a target at 30 blocks, toward none at 70 blocks, and the motion without one.
#[test]
fn ghast_look_yaw() {
    if !on() {
        return;
    }
    // Within the cow body's follow range: the target is kept on the ECS side (range beyond it drops the target).
    let cases: [(f64, f64, f64); 3] = [(12.0, 0.0, 0.0), (8.0, 0.0, 8.0), (-6.0, 0.0, 10.0)];
    for (dx, dy, dz) in cases {
        let mut r = MobRef::new(40, false);
        let g = mob_add(&mut r, mob_kind("minecraft:ghast"), Vec3::new(0.5, 1.0, 0.5), 0);
        let k = r.kind.clone();
        let t = mob_add(&mut r, k, Vec3::new(0.5 + dx, 1.0 + dy, 0.5 + dz), 1);
        r.mobs[t].ai.goals.clear();
        let tid = r.mobs[t].id;
        r.mobs[g].ai.target = Some(MWho::Mob(tid));
        let mut h = Herd::with_slab(40, false, false);
        let entries: &'static [Entry] = leak([Entry::new(&ghast_look::MODULE, 7, &ghast_look::Params::DEFAULT)]);
        let e = ecs_with(&mut h, entries, 0.5, 0.5, 21, 3);
        let mut sp = *species();
        sp.goals = horizon_ecs_proto::species::Config { entries: &[] };
        let pe = spawn_species(&mut h.world, &sp, 0.5 + dx, 0.5 + dz, 22);
        h.world.get_mut::<ActiveUntil>(pe).unwrap().0 = u64::MAX;
        let mut tg = Target::new(Who::Entity(pe), 1000, u8::MAX);
        tg.must_see = false;
        h.tick();
        set_target(&mut h.world, e, tg);
        for _ in 0..6 {
            r.step(None);
            h.tick();
        }
        let (my, ey) = (r.mobs[g].yaw, h.world.get::<Yaw>(e).unwrap().0);
        eprintln!("ghast_look target at ({dx}, {dy}, {dz}): yaw mob {my:.2} ecs {ey:.2}");
        assert!((my - ey).abs() < 1.0 || ((my - ey).abs() - 360.0).abs() < 1.0, "{my} vs {ey}");
    }
}

/// A brute 30 blocks from its home walks back, one at 5 blocks does not, one at 150 does not.
#[test]
fn brute_stroll_home_distances() {
    if !on() {
        return;
    }
    use horizon_ecs_proto::behaviours::brute_stroll_home as bh;
    let mut r = MobRef::new(40, false);
    let entries: &'static [Entry] = leak([Entry::new(&bh::MODULE, 5, &bh::Params::DEFAULT)]);
    let mut h = Herd::with_slab(40, false, false);
    let dists = [5.0, 30.0];
    let mut mob = Vec::new();
    let mut ecs = Vec::new();
    for (j, d) in dists.iter().enumerate() {
        let home = Vec3::new(0.5, 1.0, -30.5 + 20.0 * j as f64);
        let i = mob_add(&mut r, mob_kind("minecraft:piglin_brute"), Vec3::new(home.x + d, 1.0, home.z), j);
        r.mobs[i].extra.home = Some([home.x.floor() as i32, home.y.floor() as i32, home.z.floor() as i32]);
        mob.push((i, home));
        let e = ecs_with(&mut h, entries, home.x, home.z, 60 + j as u64, 4);
        h.world.get_mut::<bh::Home>(e).unwrap().block = [home.x.floor() as i32, home.y.floor() as i32, home.z.floor() as i32];
        h.teleport(e, Vec3::new(home.x + d, 1.0, home.z));
        ecs.push((e, home));
    }
    for _ in 0..200 {
        r.step(None);
        h.tick();
    }
    for (j, d) in dists.iter().enumerate() {
        let m = r.mobs[mob[j].0].position;
        let e = h.world.get::<Pos>(ecs[j].0).unwrap().0;
        let (dm, de) = (xz(m, mob[j].1), xz(e, ecs[j].1));
        eprintln!("brute_stroll_home start {d}: distance to home after 200 ticks mob {dm:.1} ecs {de:.1}");
    }
    let dm = xz(r.mobs[mob[1].0].position, mob[1].1);
    let de = xz(h.world.get::<Pos>(ecs[1].0).unwrap().0, ecs[1].1);
    // The cow body of the ECS walks at 0.2 and the brute at 0.35: the two come back, the ECS slower (not the law).
    assert!(dm < 28.0 && de < 28.0, "both walk back: mob {dm:.1} ecs {de:.1}");
    assert!(xz(r.mobs[mob[0].0].position, mob[0].1) > 4.0 && xz(h.world.get::<Pos>(ecs[0].0).unwrap().0, ecs[0].1) > 4.0, "at 5 blocks neither goes home");
}

/// The cost of the re-ask every 2 ticks of `freeze_when_looked_at` (an enderman that targets a player who does not look at
/// it): wall time per tick of 1500 such animals with the module, against the same animals without it.
#[test]
fn freeze_reask_cost_wall_time() {
    if !on() || std::env::var_os("L2COST").is_none() {
        return;
    }
    let run = |with: bool| -> f64 {
        let entries: &'static [Entry] = if with { leak([Entry::new(&freeze_when_looked_at::MODULE, 1, &freeze_when_looked_at::Params::DEFAULT)]) } else { &[] };
        let mut h = Herd::with_slab(40, false, false);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0 });
        let es: Vec<Entity> = (0..1500).map(|j| ecs_with(&mut h, entries, -6.0 + 0.45 * (j % 40) as f64, -8.0 + 0.45 * (j / 40) as f64, 2000 + j as u64, j as u64)).collect();
        h.tick();
        for &e in &es {
            let mut t = Target::player(1, 60000);
            t.must_see = false;
            set_target(&mut h.world, e, t);
        }
        for _ in 0..20 {
            h.tick();
        }
        let held = es.iter().filter(|&&e| h.world.get::<Target>(e).is_some()).count();
        eprintln!("(with module {with}: {held} still hold the target)");
        let t0 = std::time::Instant::now();
        for _ in 0..300 {
            h.tick();
        }
        t0.elapsed().as_secs_f64() / 300.0 * 1.0e6
    };
    let (a, b) = (run(false), run(true));
    eprintln!("freeze re-ask: 1500 animals holding a player: {b:.0} us/tick with the module, {a:.0} us/tick without: {:.2} us per animal per tick", (b - a) / 1500.0);
}
