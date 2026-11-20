//! `BreezeAttackEntitySensor`: the nearest of `mobs` the entity may attack (`Sensor.isEntityAttackable`: `TargetingConditions.forCombat()`
//! within the follow range, in sight), a player only when not creative or spectator, a creature only when `Params.creature` allows its
//! species (the breeze: iron golems). Written to `nearest_attackable`, erased when none.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::{self, Eye, FollowRange, Who};

pub static NEAREST_ATTACKABLE: MemKey = MemKey::new("minecraft:nearest_attackable", Shape::Who);

pub static SENSOR: Sensor = Sensor { name: "minecraft:breeze_attack_entity", mems: &[&NEAREST_ATTACKABLE, &ATTACK_TARGET], scan, requires: &[Cap::Targets], ..Sensor::DEFAULT };

/// The species a creature must be to be attacked (a name, resolved with the registry; `None`: any creature).
pub struct Params {
    pub creature: Option<&'static str>,
    /// `Sensor.wasEntityAttackableLastNTicks(window)`: the attack target's last attackable time is kept, and the brain is woken `window` ticks after
    /// it (a deadline of the wheel) to drop a target that was not seen since. 0: not kept.
    pub window: u64,
}
/// The own time at which the attack target was last attackable (written by this sensor's scans).
#[derive(Component, Clone, Copy)]
pub struct LastAttackable(pub u64);
impl Params {
    pub const DEFAULT: Params = Params { creature: None, window: 0 };
}
fn params(c: &Ctx) -> &'static Params {
    let p: &'static dyn std::any::Any = c.params;
    p.downcast_ref::<Params>().unwrap_or(&Params::DEFAULT)
}

/// `Sensor.isEntityAttackable`: in the follow range, in sight, and (`LivingEntity.canAttack` with the species rule) a creature of the
/// allowed species or a player who is not creative or spectator.
pub fn attackable(w: &World, e: Entity, who: Who, creature: Option<&str>) -> bool {
    let Some(i) = targeting::info(w, who) else { return false };
    if i.exempt {
        return false;
    }
    if let (Who::Entity(_), Some(name)) = (who, creature)
        && crate::species::registry().id(name) != Some(usize::from(i.species))
    {
        return false;
    }
    let (Some(pos), Some(eye), Some(range)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Eye>(e), w.get::<FollowRange>(e)) else { return false };
    if d2(pos, i.pos) > range.0 * range.0 {
        return false;
    }
    let mw = w.resource::<Level>().mw.0;
    mw.line_of_sight_for(Some(pos.y), horizon_core::geometry::Vec3::new(pos.x, pos.y + eye.0, pos.z), horizon_core::geometry::Vec3::new(i.pos.x, i.eye, i.pos.z))
}

thread_local! { static BUF: std::cell::RefCell<Vec<(Who, f64)>> = const { std::cell::RefCell::new(Vec::new()) }; }

/// The first candidate that `ok` accepts, in the order of the stable distance sort. The first `PICKS` are taken lazily (the nearest is usually
/// accepted: no sort); past that the rest is sorted, as a lazy selection over a long list that accepts nothing is quadratic. Ties keep their
/// position either way (`remove` keeps the order of the rest, and `sort_by` is stable).
fn first_in_sorted_order<F: FnMut(Who) -> bool>(cands: &mut Vec<(Who, f64)>, mut ok: F) -> Option<Who> {
    const PICKS: usize = 3;
    for _ in 0..PICKS {
        let best = (0..cands.len()).reduce(|b, i| if cands[i].1.total_cmp(&cands[b].1).is_lt() { i } else { b })?;
        let (who, _) = cands.remove(best);
        if ok(who) {
            return Some(who);
        }
    }
    cands.sort_by(|a, b| a.1.total_cmp(&b.1));
    cands.iter().map(|c| c.0).find(|&who| ok(who))
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let (creature, window) = (params(c).creature, params(c).window);
    if window > 0
        && let Some(MemValue::Who(t)) = get(w, e, &ATTACK_TARGET)
        && attackable(w, e, *t, creature)
    {
        let now = now_of(w, e);
        w.entity_mut(e).insert(LastAttackable(now));
        arm_at(w, e, now + window + 1, Due::BrainWake);
    }
    // The `Mob` runs `sense_living` first in the same scan: the list is read afresh, not from `mobs`.
    let found = {
        let ww: &World = &*w;
        BUF.with_borrow_mut(|buf| {
            super::living::living_candidates(ww, e, buf);
            first_in_sorted_order(buf, |who| attackable(ww, e, who, creature))
        })
    };
    match found {
        Some(who) => set(w, e, &NEAREST_ATTACKABLE, MemValue::Who(who), None),
        None => erase(w, e, &NEAREST_ATTACKABLE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference: a stable sort by distance, then the first accepted candidate.
    #[inline(never)]
    fn sorted_find<F: Fn(Who) -> bool>(cands: &[(Who, f64)], ok: F) -> Option<Who> {
        let mut sorted = cands.to_vec();
        sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
        sorted.iter().map(|c| c.0).find(|&who| ok(who))
    }

    #[test]
    #[ignore = "bench"]
    fn lazy_vs_sort_bench() {
        use std::time::Instant;
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        for n in [4usize, 8, 16, 40, 100, 300] {
            let cands: Vec<(Who, f64)> = (0..n)
                .map(|i| {
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    (Who::Player(i as u32), (s % 10_000) as f64)
                })
                .collect();
            for (label, hit) in [("none attackable", usize::MAX), ("first attackable", 0)] {
                let ok = |who: Who| matches!(who, Who::Player(p) if p as usize == hit);
                let first = cands.iter().min_by(|a, b| a.1.total_cmp(&b.1)).map(|c| c.0);
                let ok = |who: Who| if hit == 0 { Some(who) == first } else { ok(who) };
                let reps = 2_000_000 / n;
                let mut buf = Vec::new();
                let t = Instant::now();
                for _ in 0..reps {
                    buf.clear();
                    buf.extend_from_slice(&cands);
                    std::hint::black_box(first_in_sorted_order(&mut buf, ok));
                }
                let lazy = t.elapsed().as_nanos() as f64 / reps as f64;
                let t = Instant::now();
                for _ in 0..reps {
                    buf.clear();
                    buf.extend_from_slice(&cands);
                    std::hint::black_box(sorted_find(&buf, ok));
                }
                let sort = t.elapsed().as_nanos() as f64 / reps as f64;
                println!("n {n:4} {label:17} lazy {lazy:9.0} ns  sort {sort:9.0} ns");
            }
        }
    }

    #[test]
    fn lazy_first_equals_sorted_find() {
        const DISTS: [f64; 9] = [0.0, -0.0, 0.5, 1.0, 2.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 2.0];
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for case in 0..5_000u64 {
            let n = next() % 41;
            let modulus = 2 + next() % 9;
            let cands: Vec<(Who, f64)> = (0..n)
                .map(|i| (Who::Player(i as u32), DISTS[(next() % DISTS.len() as u64) as usize]))
                .collect();
            let ok = move |who: Who| match who {
                Who::Player(p) => (u64::from(p).wrapping_mul(0x9E37_79B9) ^ case) % modulus == 0,
                Who::Entity(_) => false,
            };
            let want = sorted_find(&cands, ok);
            let got = first_in_sorted_order(&mut cands.clone(), ok);
            assert_eq!(got, want, "case {case}");
        }
    }
}
