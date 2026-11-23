//! Navigation: the path search, following a path, the stroll spot.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// What the path search reads of an entity that walks.
#[derive(Component, Clone, Copy)]
pub struct NavParams {
    pub malus: [Option<f32>; PATH_TYPES],
    pub can_float: bool,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_walk_over_fences: bool,
    pub follow_range: f32,
}

/// The number of failed attempts before the first success of a 1-in-`n` draw each tick (the same law as
/// drawing `nextInt(n) == 0` every tick, sampled once).
/// The goal selector asks a goal that is not running whether it can start every OTHER tick: the wait is twice the
/// number of failed asks.
pub fn idle_wait(rng: &mut Rng, n: i32) -> i32 {
    2 * geometric(rng, n)
}

pub fn geometric(rng: &mut Rng, n: i32) -> i32 {
    let u = rng.next_f64().max(1.0e-12);
    (u.ln() / (1.0 - 1.0 / f64::from(n)).ln()).ifloor()
}

// ---- systems ---------------------------------------------------------------------------------------------
/// `RandomStrollGoal.canUse` and `LandRandomPos`: after the wait, 10 tries of a spot within 10 x 7 (the first valid
/// one wins as all the ground weighs the same); the path to it is made by `pathfind`.
/// Arms the stroll attempt `idle_wait` ticks from now (the goal selector asks every other tick, 1 chance in 60).
pub(crate) fn arm_stroll(wheel: &mut Wheel, clock: &Clock, e: Entity, s: &mut Stroll, r: &mut Rng) {
    let now = clock.of(e);
    let at = now + idle_wait(r, s.interval).max(1) as u64;
    wheel.push(clock.at(e, at), e, Due::Stroll);
    s.scheduled = true;
}

/// `RandomStrollGoal.canUse` and `LandRandomPos`, at the tick the attempt was due: 10 tries of a spot within
/// 10 x 7 (the first valid one wins as all the ground weighs the same); the path to it is made by `pathfind`.
/// Returns whether a spot was picked.
pub(crate) fn pick_spot(level: &Level, pos: &Pos, s: &mut Stroll, r: &mut Rng, nav: &NavParams, walk: crate::spots::WalkValue, in_water: bool) -> bool {
    use crate::spots::{spotter, Pick};
    let (h, v) = (s.horizontal_range, s.vertical_range);
    let mut sp = spotter(level, [pos.0.x, pos.0.y, pos.0.z], nav, walk, r);
    // `WaterAvoidingRandomStrollGoal.getPosition`: in water a land spot first, else a land spot, a plain one 1 time in 1000.
    let picked = if in_water {
        sp.pick(Pick::Land { h: h + 5, v }).or_else(|| sp.pick(Pick::Default { h, v }))
    } else if sp.rng.next_float() >= s.plain {
        sp.pick(Pick::Land { h, v })
    } else {
        sp.pick(Pick::Default { h, v })
    };
    let Some(p) = picked else { return false };
    s.walking = true;
    s.tx = p[0];
    s.tz = p[2];
    s.ty = p[1].ifloor();
    true
}

/// The cells of row `zz` (columns, inclusive; a run is empty when its start is past its end) that may lie in one of
/// two circles of 32 blocks (a player's place before and after, `None`: absent) and not in the other: the cells either
/// circle touches except those both surely contain, as two runs. A superset, with a margin of a cell or two.

/// `Mob.checkDespawn`'s reset of `noActionTime`, by event: nothing is read while the players stay where they are
/// and the cows stay in their cells. When a player moves (`Players.dirty`), only the cows of the cells its old and
/// new circles of 32 blocks do not both contain or both exclude are tested (the thin ring a step sweeps; a player
/// that comes or goes sweeps its whole circle); a cow that changes cell is tested against the players as well.
/// A test is against the players of the 3 x 3 bins of 32 blocks around the cow, and only the cows that enter or leave
/// the 32 blocks are touched: an entering cow is active without end (`ActiveUntil(u64::MAX)`) and its stroll attempt
/// is armed if it had none; a leaving cow stays active for 100 ticks more (the `noActionTime` it has to count up to).
pub(crate) fn player_near(
    clock: Res<Clock>,
    mut players: ResMut<Players>,
    mut idx: ResMut<Index>,
    mut wheel: ResMut<Wheel>,
    mut entered: ResMut<crate::goals::Entered>,
    mut left: Option<ResMut<crate::despawn::LeftRing>>,
    mut active: Query<(&Bounds, &mut ActiveUntil, Option<&mut Stroll>, Option<&mut Rng>)>,
) {
    const BIN: f64 = 32.0;
    const R2: f64 = BIN * BIN;
    if !players.dirty && idx.crossed.is_empty() {
        return;
    }
    let idx = &mut *idx;
    let pl = &mut *players;
    let origin = idx.origin;
    let bins = ((idx.width as f64 * CELL / BIN).ceil() as i64).max(1) + 2;
    let bin = |v: f64| (((v - origin) / BIN).floor() as i64 + 1).clamp(0, bins - 1);
    let tick = clock.0 as u32 + 1;
    pl.candidates.clear();
    if pl.dirty {
        pl.dirty = false;
        // The bins hold the players as they are now.
        pl.n = bins;
        pl.bins.iter_mut().for_each(Vec::clear);
        pl.bins.resize((bins * bins) as usize, Vec::new());
        for i in 0..pl.list.len() {
            let (px, pz) = pl.list[i];
            pl.bins[(bin(pz) * bins + bin(px)) as usize].push((px, pz));
        }
        std::mem::swap(&mut pl.prev, &mut pl.old);
        pl.prev.clear();
        pl.prev.extend_from_slice(&pl.list);
        // The cells a moved player's two circles do not agree on, each once.
        for i in 0..pl.list.len().max(pl.old.len()) {
            let (n, o) = (pl.list.get(i).copied(), pl.old.get(i).copied());
            if n == o {
                continue;
            }
            let (mut x0, mut x1, mut z0, mut z1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for (px, pz) in [n, o].into_iter().flatten() {
                (x0, x1, z0, z1) = (x0.min(px - BIN), x1.max(px + BIN), z0.min(pz - BIN), z1.max(pz + BIN));
            }
            let (x0, x1) = (idx.column(x0).max(0), idx.column(x1).min(idx.width as i64 - 1));
            let (z0, z1) = (idx.column(z0).max(0), idx.column(z1).min(idx.width as i64 - 1));
            // A rectangle of columns the two circles agree on (inside both, or outside both) holds no animal whose state
            // changes: only the blocks and the columns that hold an animal and are not so are looked at.
            let agree = |cx0: f64, cx1: f64, cz0: f64, cz1: f64| -> bool {
                let (mut inside, mut outside) = (true, true);
                for q in [n, o] {
                    let Some((px, pz)) = q else { inside = false; continue };
                    let (nx, nz) = (px.clamp(cx0, cx1) - px, pz.clamp(cz0, cz1) - pz);
                    let (fx, fz) = ((px - cx0).abs().max((px - cx1).abs()), (pz - cz0).abs().max((pz - cz1).abs()));
                    inside &= fx * fx + fz * fz < R2;
                    outside &= nx * nx + nz * nz >= R2;
                }
                inside || outside
            };
            let b = BLOCK as i64;
            for bz in z0 / b..=z1 / b {
                for bx in x0 / b..=x1 / b {
                    if idx.blocks[(bz * idx.bw as i64 + bx) as usize] == 0 {
                        continue;
                    }
                    let (bx0, bz0) = (origin + (bx * b) as f64 * CELL, origin + (bz * b) as f64 * CELL);
                    if agree(bx0, bx0 + b as f64 * CELL, bz0, bz0 + b as f64 * CELL) {
                        continue;
                    }
                    let mut bits = idx.blocks[(bz * idx.bw as i64 + bx) as usize];
                    while bits != 0 {
                        let k = bits.trailing_zeros() as i64;
                        bits &= bits - 1;
                        let (xx, zz) = (bx * b + k % b, bz * b + k / b);
                        let c = zz as usize * idx.width + xx as usize;
                        if idx.stamp[c] == tick {
                            continue;
                        }
                        let (cx0, cz0) = (origin + xx as f64 * CELL, origin + zz as f64 * CELL);
                        if !agree(cx0, cx0 + CELL, cz0, cz0 + CELL) {
                            idx.stamp[c] = tick;
                            pl.candidates.push(c);
                        }
                    }
                }
            }
        }
    }
    let list_empty = pl.list.is_empty();
    let (bins_n, nbins) = (pl.n, &pl.bins);
    let within = |x: f64, z: f64| {
        let (bx, bz) = (bin(x), bin(z));
        (bz - 1..=bz + 1).any(|zz| {
            (bx - 1..=bx + 1).any(|xx| {
                (0..bins_n).contains(&xx)
                    && (0..bins_n).contains(&zz)
                    && nbins[(zz * bins_n + xx) as usize].iter().any(|&(px, pz)| (x - px).powi(2) + (z - pz).powi(2) < R2)
            })
        })
    };
    let mut test = |e: Entity, active: &mut Query<(&Bounds, &mut ActiveUntil, Option<&mut Stroll>, Option<&mut Rng>)>, wheel: &mut Wheel| {
        let Ok((b, mut a, s, r)) = active.get_mut(e) else { return };
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let now_in = within((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        if now_in && a.0 != u64::MAX {
            a.0 = u64::MAX;
            entered.0.push(e);
            // The first stroll attempt belongs to an entity that walks and has goals; an object only wakes.
            if let (Some(mut s), Some(mut r)) = (s, r)
                && !s.scheduled
                && !s.walking
            {
                arm_stroll(wheel, &clock, e, &mut s, &mut r);
            }
        } else if !now_in && a.0 == u64::MAX {
            a.0 = clock.of(e) + 100;
            if let Some(l) = left.as_mut() {
                l.0.push(e);
            }
        }
    };
    for &c in &pl.candidates {
        for &e in &idx.cells[c] {
            test(e, &mut active, &mut wheel);
        }
    }
    if !list_empty {
        for &(e, cell) in &idx.crossed {
            // A column more or less changes nothing for an animal with no player within a bin of it.
            let (cx, cz) = ((cell as usize % idx.width) as f64 + 0.5 + origin / CELL, (cell as usize / idx.width) as f64 + 0.5 + origin / CELL);
            let (bx, bz) = (bin(cx * CELL), bin(cz * CELL));
            let near = (bz - 1..=bz + 1).any(|zz| (bx - 1..=bx + 1).any(|xx| (0..bins_n).contains(&xx) && (0..bins_n).contains(&zz) && !nbins[(zz * bins_n + xx) as usize].is_empty()));
            if near {
                test(e, &mut active, &mut wheel);
            }
        }
    }
    idx.crossed.clear();
}

pub(crate) fn comfortable_fall_distance(f: f32) -> i32 {
    (f + 3.0).ifloor()
}

/// `GroundPathNavigation.findSurfacePosition`: the spot a path target stands on, searched within the build range (`min_y` and `max_y`
/// kept). From above, the search goes down to the first non-air block; from inside a solid block, up to the first non-solid one.
pub fn surface_search(min_y: i32, max_y: i32, air: impl Fn([i32; 3]) -> bool, solid: impl Fn([i32; 3]) -> bool, mut p: [i32; 3]) -> [i32; 3] {
    if air(p) {
        let mut q = [p[0], (p[1] - 1).min(max_y), p[2]];
        while q[1] >= min_y && air(q) {
            q[1] -= 1;
        }
        if q[1] >= min_y {
            return [q[0], q[1] + 1, q[2]];
        }
        q[1] = (p[1] + 1).max(min_y);
        while q[1] <= max_y && air(q) {
            q[1] += 1;
        }
        p = q;
    }
    if !solid(p) {
        return p;
    }
    let mut q = [p[0], p[1] + 1, p[2]];
    while q[1] <= max_y && solid(q) {
        q[1] += 1;
    }
    q
}

/// `PathNavigation.createPath` + `moveTo` for the spot a stroll picked: Horizon's real path finder. Only the cows
/// that asked for a path are visited, in parallel when asked; what follows (the next module, the end of the walk)
/// is a command.
#[allow(clippy::type_complexity)]
pub(crate) fn pathfind(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    day: Res<crate::hostile::Daylight>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &Contact, &mut Stroll, &mut Nav, Has<InFluid>, &Vel, &NavParams, Has<crate::locomotion::lava::StandsOnLava>, Has<crate::locomotion::climb::Climber>, Has<crate::behaviours::restrict_sun::AvoidSun>, Option<&crate::locomotion::swim::SwimParams>, Has<crate::mount::Riding>, Option<(&crate::fall_by_health::FallByHealth, &crate::cow::Health, Has<crate::targeting::Target>)>), (With<NeedsPath>, Or<(With<crate::locomotion::WalkMode>, With<crate::locomotion::LavaMode>, Without<crate::locomotion::Modes>)>)>,
) {
    let level = &*level;
    let one = |e: Entity, pos: &Pos, body: &Body, c: &Contact, s: &mut Stroll, nav: &mut Nav, fluid: bool, _vel: &Vel, sp: &NavParams, lava: bool, climber: bool, sunless: bool, swp: Option<&crate::locomotion::swim::SwimParams>, riding: bool, fall: Option<(&crate::fall_by_health::FallByHealth, &crate::cow::Health, bool)>| {
        let now = clock.of(e);
        let mw = level.mw.0;
        let pm = PathMob {
            position: pos.0,
            width: body.width,
            height: body.height,
            on_ground: c.on_ground,
            in_water: false,
            max_up_step: body.step_height,
            max_fall_distance: crate::fall_by_health::path_max_fall(fall.map(|f| f.1.0), fall.is_some_and(|f| f.2)),
            malus: sp.malus,
            can_float: sp.can_float,
            can_open_doors: sp.can_open_doors,
            can_pass_doors: sp.can_pass_doors,
            can_walk_over_fences: sp.can_walk_over_fences,
            stands_on_lava: lava,
        };
        // `PathNavigation.getMaxPathLength`: max(follow range, `requiredPathLength` = 16): a vindicator (12) searches as far as a cow.
        let max_len = sp.follow_range.max(16.0);
        let max_visited = (max_len * 16.0).ifloor();
        let shape = mw.world.shape;
        let target = surface_search(
            shape.min_y,
            shape.max_y() - 1,
            |p| !shape.contains_y(p[1]) || mw.facts.is_air(mw.world.block(p[0], p[1], p[2])),
            |p| shape.contains_y(p[1]) && mw.solid(p[0], p[1], p[2]),
            [s.tx.ifloor(), s.ty, s.tz.ifloor()],
        );
        // `GroundPathNavigation.canUpdatePath`: no path is made in the air (on the ground, in a liquid, riding).
        // The amphibious navigation (`AmphibiousPathNavigation`) serves both media: its evaluator, in the water and out of it.
        let amph = swp.and_then(|p| p.amphibious);
        let eval = || match amph {
            Some(prefers_shallow) => Evaluator::Amphibious { prefers_shallow },
            None => Evaluator::Walk,
        };
        let path = (c.on_ground || fluid || amph.is_some() || riding)
            .then(|| {
                nav.searches += 1;
                crate::pathsearch::probe!(crate::goals::FINDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
                if crate::pathsearch::use_own() {
                    crate::pathsearch::Finder::new(mw, &pm, eval()).with_occ(level.occ).find(&[target], max_len, nav.reach, max_visited)
                } else {
                    Finder::new(mw, &pm, eval()).find(&[target], max_len, nav.reach, max_visited)
                }
            })
            .flatten();
        if climber {
            // `WallClimberNavigation.pathToPosition`: the climb goes on straight to the target when the path is over (or when there is none).
            let (to, speed) = (target, nav.speed);
            par.command_scope(e, |mut cmd| {
                cmd.queue(move |w: &mut World| crate::locomotion::climb::wish(w, e, to, speed));
            });
        }
        // `GroundPathNavigation.trimPath` with `avoidSun`: a mob out of the sky's sight keeps only the nodes before the first one the sky sees.
        let path = path.map(|mut p| {
            if sunless && day.0 {
                let at = [pos.0.x.ifloor(), (pos.0.y + 0.5).ifloor(), pos.0.z.ifloor()];
                if !mw.can_see_sky(at[0], at[1], at[2]) {
                    if let Some(i) = p.nodes.iter().position(|n| mw.can_see_sky(n.x, n.y, n.z)) {
                        p.nodes.truncate(i);
                    }
                }
            }
            p
        });
        // `MoveToTargetSink`: a path that does not reach its target starts the clock of the unreachable, one that does clears it.
        if path.as_ref().is_some_and(|p| p.reached && !p.nodes.is_empty()) {
            nav.cant_reach_since = 0;
        } else if nav.cant_reach_since == 0 {
            nav.cant_reach_since = now.max(1);
        }
        par.command_scope(e, |mut cmd| match path {
            Some(p) if !p.nodes.is_empty() => {
                crate::pathsearch::probe!(crate::goals::PATH_NODES.fetch_add(p.nodes.len() as u64, std::sync::atomic::Ordering::Relaxed));
                crate::pathsearch::probe!(crate::goals::PATHS_FOUND.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
                nav.path = Some(p);
                // `PathNavigation.moveTo` sets `lastStuckCheck` before the tick of the navigation counts (the first tick of a path is `+1`).
                nav.last_check = now.saturating_sub(1);
                nav.last_pos = pos.0;
                cmd.entity(e).remove::<NeedsPath>().insert((Following, Moving));
            }
            _ => {
                nav.path = None;
                s.walking = false;
                cmd.entity(e).remove::<NeedsPath>().insert(PathEnded);
            }
        });
    };
    // At most `path_budget` searches (0: no limit): the first of the queue in the table order, the same whatever the thread count. ONE call
    // site of `one` per mode: a third inlined copy slowed the unbudgeted systems by 0.5 % (measured, goal_bench cow idle 1530 -> 1538).
    let mut left = if cfg.path_budget > 0 { cfg.path_budget } else { usize::MAX };
    if cfg.parallel {
        let mut chosen: Vec<Entity> = if cfg.path_budget > 0 { q.iter().map(|x| x.0).take(left).collect() } else { Vec::new() };
        chosen.sort_unstable();
        let (limited, chosen) = (cfg.path_budget > 0, &chosen);
        q.par_iter_mut().for_each(|(e, p, b, c, mut s, mut n, f, v, i, l, cl, u, w, r, fa)| {
            if !limited || chosen.binary_search(&e).is_ok() {
                one(e, p, b, c, &mut s, &mut n, f, v, i, l, cl, u, w, r, fa);
            }
        });
    } else {
        for (e, p, b, c, mut s, mut n, f, v, i, l, cl, u, w, r, fa) in &mut q {
            if left == 0 {
                break;
            }
            left -= 1;
            one(e, p, b, c, &mut s, &mut n, f, v, i, l, cl, u, w, r, fa);
        }
    }
    par.flush();
}

/// `PathNavigation.tick` -> `followThePath` (ground): reach the waypoint, or cut the corner, then steer the move
/// control at the next node. (The recompute on a block change is not here.) Visits only the cows that follow a path.
#[allow(clippy::type_complexity)]
pub(crate) fn follow(
    cfg: Res<Config>,
    clock: Res<Clock>,
    level: Res<Level>,
    mut par: crate::cow::OrdCmds,
    mut q: Query<(Entity, &Pos, &Body, &mut Drive, &mut Stroll, &mut Nav, &mut MoveTo), (With<Following>, Without<Stilled>, Or<(With<crate::locomotion::WalkMode>, With<crate::locomotion::LavaMode>, Without<crate::locomotion::Modes>)>)>,
) {
    let level = &*level;
    let one = |e: Entity, pos: &Pos, body: &Body, drive: &mut Drive, s: &mut Stroll, nav: &mut Nav, m: &mut MoveTo| {
        let now = clock.of(e);
        let mw = level.mw.0;
        let mut end = false;
        match nav.path.as_mut() {
            None => end = true,
            Some(path) => {
                let mob = Vec3::new(pos.0.x, (pos.0.y + 0.5).floor(), pos.0.z);
                let width = body.width;
                let max_distance = if width > 0.75 { width / 2.0 } else { 0.75 - width / 2.0 };
                let n = path.next_node();
                let xd = (pos.0.x - (f64::from(n.x) + 0.5)).abs();
                let yd = (pos.0.y - f64::from(n.y)).abs();
                let zd = (pos.0.z - (f64::from(n.z) + 0.5)).abs();
                let max = f64::from(max_distance);
                let close = xd < max && zd < max && yd < 1.0;
                if close || (Navigation::can_cut_corner(n.kind) && should_target_next_node(path, mob, width)) {
                    path.advance();
                }
                // `doStuckDetection`: every 100 ticks, a cow that has moved less than a quarter of what its speed
                // would carry it gives its path up.
                let mut stuck = false;
                if now - nav.last_check > 100 {
                    let speed = drive.speed;
                    let effective = if speed >= 1.0 { speed } else { speed * speed };
                    let threshold = f64::from(effective * 100.0 * 0.25);
                    let d = (mob.x - nav.last_pos.x).powi(2) + (mob.y - nav.last_pos.y).powi(2) + (mob.z - nav.last_pos.z).powi(2);
                    stuck = d < threshold * threshold;
                    nav.last_check = now;
                    nav.last_pos = mob;
                }
                let path = nav.path.as_ref().unwrap();
                if stuck || path.is_done() {
                    end = true;
                } else {
                    let target = path.entity_pos_at(path.next, width);
                    let (bx, by, bz) = (target.x.ifloor(), target.y.ifloor(), target.z.ifloor());
                    let y = if mw.facts.is_air(mw.world.block(bx, by.saturating_sub(1), bz)) {
                        target.y
                    } else {
                        mw.floor_level(bx, by, bz)
                    };
                    *m = MoveTo { active: true, x: target.x, y, z: target.z, speed_modifier: nav.speed };
                }
            }
        }
        if end {
            nav.path = None;
            s.walking = false;
            drive.zza = 0.0;
            drive.jump = false;
            m.active = false;
            par.command_scope(e, |mut cmd| {
                cmd.entity(e).remove::<Following>().insert(PathEnded);
            });
        }
    };
    if cfg.parallel {
        q.par_iter_mut().for_each(|(e, p, b, mut d, mut s, mut n, mut m)| one(e, p, b, &mut d, &mut s, &mut n, &mut m));
    } else {
        for (e, p, b, mut d, mut s, mut n, mut m) in &mut q {
            one(e, p, b, &mut d, &mut s, &mut n, &mut m);
        }
    }
    par.flush();
}

/// The walks that are over end the goal that asked for them (`goals.rs`), which arms what comes next.
pub(crate) fn path_ended(q: Query<(Entity, &crate::goals::GoalSet, Option<&crate::mount::Mount>), With<PathEnded>>, sets: Query<&crate::goals::GoalSet>, mut reqs: ResMut<crate::goals::Requests>, mut commands: Commands) {
    for (e, g, mount) in &q {
        crate::goals::path_ended(e, g, &mut reqs);
        // The walk of a mount steered by a creature is the creature's walk (`Mob.getNavigation` of a rider).
        if let Some(crate::targeting::Who::Entity(r)) = mount.and_then(|m| m.rider)
            && let Ok(rg) = sets.get(r)
        {
            crate::goals::path_ended(r, rg, &mut reqs);
        }
        commands.entity(e).remove::<PathEnded>();
        commands.queue(move |w: &mut World| crate::walk_paces::pace(w, e, 0.0));
    }
}

// Vanilla's NaN-sensitive `!(a < b)` form.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub(crate) fn should_target_next_node(path: &Path, mob: Vec3, width: f32) -> bool {
    let _ = width;
    if path.next + 1 >= path.nodes.len() {
        return false;
    }
    let n = path.nodes[path.next];
    let current = Vec3::new(f64::from(n.x) + 0.5, f64::from(n.y), f64::from(n.z) + 0.5);
    let d = |a: Vec3, b: Vec3| (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2);
    if d(mob, current) >= 4.0 {
        return false;
    }
    // `canMoveDirectly` is false for the ground navigation.
    let m = path.nodes[path.next + 1];
    let next = Vec3::new(f64::from(m.x) + 0.5, f64::from(m.y), f64::from(m.z) + 0.5);
    let to_current = Vec3::new(current.x - mob.x, current.y - mob.y, current.z - mob.z);
    let to_next = Vec3::new(next.x - mob.x, next.y - mob.y, next.z - mob.z);
    let (cs, ns) = (to_current.length_squared(), to_next.length_squared());
    if !(ns < cs) && !(cs < 0.5) {
        return false;
    }
    let a = to_current.normalize();
    let b = to_next.normalize();
    a.x * b.x + a.y * b.y + a.z * b.z < 0.0
}
