//! `SpellcasterUseSpellGoal`: a spell of a caster that has a living target: it waits `warmup` ticks after the start, then casts its
//! effect, and for `casting` ticks the caster stands still (`spell_casting`); it is tried again `interval` ticks after the start.
//! One entry per spell; the species picks the spells (`Spell`), the module knows no species. The state is one component for all the
//! entries of the caster (only one spell runs at a time, the cast being the lock): `next[nth]` the earliest start of each entry.
//!
//! Spells: `Vexes` (fewer than a roll of 1 to 8 summons around it within 16 blocks: three of species `species`, owned, limited to 30 to
//! 120 s of life), `Fangs` (a ring of 13 when the target is within 3 blocks, else a line of 16: they bite 6 magic damage when their
//! delay `+ 9` ticks is over, everything but the caster and its `allies`), `Disappear` (invisible for 60 s).
//! Not done: the wololo and the blindness spells (no sheep colour in the ECS, no blindness effect), the sounds, the game event of each
//! vex, the fang floor test counts any
//! solid block with air above.
use super::spell_casting::Casting;
use super::*;
use crate::cow::*;
use crate::effects;
use crate::goals::*;
use crate::hostile::PlayerState;
use crate::targeting::{self, Target, Who};
use horizon_core::geometry::{trig, Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub enum Spell {
    Vexes { species: &'static str },
    Fangs { allies: &'static [&'static str] },
    Disappear,
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub spell: Spell,
    /// The clients' spell id (`IllagerSpell`: 1 summon, 2 fangs, 4 disappear).
    pub id: u8,
    pub warmup: i32,
    pub casting: i32,
    pub interval: i64,
}
impl Params {
    pub const MIRROR: Self = Self { spell: Spell::Disappear, id: 4, warmup: 20, casting: 20, interval: 340 };
}
/// The entries of the caster: their ranks, the spell being warmed up (`running`: the entry's `nth`) and its warm-up.
#[derive(Component, Clone, Copy, Debug)]
pub struct Spells {
    slot: [u8; 3],
    p: [Option<Params>; 3],
    next: [u64; 3],
    running: u8,
    warmup: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Spelling;
/// A fang (`EvokerFangs`): its own entity, a component of the spot, the owner and the countdown (`warmup`, which bites at -8). It is not a
/// creature: only this module's tick knows it, and each fang holds one carrier of the module (`Carriers`) so that the tick goes on until it is gone.
#[derive(Component, Clone, Copy, Debug)]
pub struct Fang {
    at: Vec3,
    delay: i32,
    owner: Entity,
    allies: &'static [&'static str],
}

pub static MODULE: Module = Module {
    name: "spell_use",
    flags: 0,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Goals, Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let n = (inst.nth as usize).min(2);
    let p = *params::<Params>(p);
    if w.get::<Spells>(e).is_none() {
        w.entity_mut(e).insert(Spells { slot: [u8::MAX; 3], p: [None; 3], next: [0; 3], running: 0, warmup: 0 });
    }
    let mut s = w.get_mut::<Spells>(e).unwrap();
    s.slot[n] = inst.slot;
    s.p[n] = Some(p);
}
fn alive_target(w: &World, e: Entity) -> bool {
    w.get::<Target>(e).is_some_and(|t| targeting::info(w, t.who).is_some_and(|i| !i.exempt))
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let n = (inst.nth as usize).min(2);
    let Some(s) = w.get::<Spells>(e).copied() else { return false };
    let Some(p) = s.p[n] else { return false };
    if !alive_target(w, e) || w.get::<Casting>(e).is_some() || now_of(w, e) < s.next[n] {
        return false;
    }
    match p.spell {
        // `EvokerSummonSpellGoal`: fewer of its vexes within 16 blocks than a roll of 1 to 8.
        Spell::Vexes { species } => {
            let Some(id) = crate::species::registry().id(species) else { return false };
            let me = w.get::<Pos>(e).unwrap().0;
            let near = w.query::<(&Pos, &SpeciesId, &Health)>().iter(w).filter(|(p, s, h)| s.0 as usize == id && h.0 > 0.0 && d2(p.0, me) <= 256.0).count() as i32;
            with_rng(w, e, |r| r.next_int(8) + 1) > near
        }
        Spell::Disappear => !effects::has(w, e, effects::Kind::Invisibility),
        Spell::Fangs { .. } => true,
    }
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let n = (inst.nth as usize).min(2);
    let now = now_of(w, e);
    let p = {
        let mut s = w.get_mut::<Spells>(e).unwrap();
        let p = s.p[n].unwrap();
        s.running = n as u8;
        s.warmup = (p.warmup + 1) / 2;
        s.next[n] = now + p.interval as u64;
        p
    };
    w.entity_mut(e).insert((Casting { ticks: p.casting, spell: p.id }, Spelling));
    wake(w, e, &super::spell_casting::MODULE);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Spelling>();
}
/// `canContinueToUse`: a living target and a warm-up not over.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, warm);
    pending(w);
}
fn warm(mut q: Query<(Entity, &mut Spells, Option<&Target>), With<Spelling>>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for (e, mut s, target) in &mut q {
        let slot = s.slot[s.running as usize];
        if target.is_none() {
            reqs.0.push(Req::End(e, slot));
            continue;
        }
        s.warmup -= 1;
        if s.warmup == 0 {
            let p = s.p[s.running as usize].unwrap();
            cmd.queue(move |w: &mut World| cast(w, e, p));
        }
        if s.warmup <= 0 {
            reqs.0.push(Req::End(e, slot));
        }
    }
}
/// `performSpellCasting`.
fn cast(w: &mut World, e: Entity, p: Params) {
    match p.spell {
        Spell::Vexes { species } => summon(w, e, species),
        Spell::Fangs { allies } => fangs(w, e, allies),
        Spell::Disappear => {
            effects::add(w, e, effects::Kind::Invisibility, 0, 1200);
        }
    }
}
/// `EvokerSummonSpellGoal.performSpellCasting`: three, at the evoker's block shifted by -2 to +2, a block up, living 30 to 120 s.
fn summon(w: &mut World, e: Entity, species: &'static str) {
    let Some(id) = crate::species::registry().id(species) else { return };
    let (Some(me), Some(sp)) = (w.get::<Pos>(e).map(|p| p.0), w.resource::<SpeciesTable>().0.get(id).copied()) else { return };
    let now = now_of(w, e);
    for _ in 0..3 {
        let (dx, dz, life, seed) = with_rng(w, e, |r| (r.next_int(5) - 2, r.next_int(5) - 2, 20 * (30 + r.next_int(90)), u64::from(r.next_u32())));
        let at = Vec3::new(me.x.floor() + f64::from(dx) + 0.5, me.y.floor() + 1.0, me.z.floor() + f64::from(dz) + 0.5);
        let v = spawn_species(w, &sp, at.x, at.z, seed);
        w.get_mut::<Pos>(v).unwrap().0 = at;
        w.entity_mut(v).insert(crate::tame::Owner(Some(Who::Entity(e))));
        crate::goals::arm_timers(w, v);
        w.entity_mut(v).insert(crate::behaviours::limited_life::Lifespan(now + life as u64));
    }
}
/// `EvokerAttackSpellGoal.performSpellCasting`: a ring of 5 and a ring of 8 (delay 3) near, else a line of 16 (delay 0 to 15).
fn fangs(w: &mut World, e: Entity, allies: &'static [&'static str]) {
    let Some(t) = w.get::<Target>(e).and_then(|t| targeting::info(w, t.who)) else { return };
    let me = w.get::<Pos>(e).unwrap().0;
    let (min_y, max_y) = (t.pos.y.min(me.y), t.pos.y.max(me.y) + 1.0);
    let angle = (t.pos.z - me.z).atan2(t.pos.x - me.x) as f32;
    let mut spots: Vec<(f64, f64, i32)> = Vec::new();
    let (cos, sin) = (|a: f32| f64::from(trig::cos(f64::from(a))), |a: f32| f64::from(trig::sin(f64::from(a))));
    if d2(me, t.pos) < 9.0 {
        for i in 0..5 {
            let a = angle + i as f32 * std::f32::consts::PI * 0.4;
            spots.push((me.x + cos(a) * 1.5, me.z + sin(a) * 1.5, 0));
        }
        for i in 0..8 {
            let a = angle + i as f32 * std::f32::consts::PI * 2.0 / 8.0 + std::f32::consts::PI * 2.0 / 5.0;
            spots.push((me.x + cos(a) * 2.5, me.z + sin(a) * 2.5, 3));
        }
    } else {
        for i in 0..16 {
            let reach = 1.25 * f64::from(i + 1);
            spots.push((me.x + cos(angle) * reach, me.z + sin(angle) * reach, i));
        }
    }
    for (x, z, delay) in spots {
        if let Some(y) = floor(w, x, z, min_y, max_y) {
            w.spawn(Fang { at: Vec3::new(x, y, z), delay, owner: e, allies });
            if let Some(i) = crate::species::registry().module_id(&MODULE) {
                w.resource_mut::<Carriers>().0[i] += 1;
            }
        }
    }
}
/// `createSpellEntity`: the first block, from `max_y` down to under `min_y`, that has a solid block under it and none in it.
fn floor(w: &World, x: f64, z: f64, min_y: f64, max_y: f64) -> Option<f64> {
    let level = w.resource::<Level>();
    let (bx, bz) = (x.floor() as i32, z.floor() as i32);
    let mut y = max_y.floor() as i32;
    while y >= min_y.floor() as i32 - 1 {
        let cell = |lo: f64, hi: f64| Aabb::new(Vec3::new(f64::from(bx) + 0.4, lo, f64::from(bz) + 0.4), Vec3::new(f64::from(bx) + 0.6, hi, f64::from(bz) + 0.6));
        if !level.no_collision(cell(f64::from(y) - 0.05, f64::from(y))) && level.no_collision(cell(f64::from(y), f64::from(y) + 0.8)) {
            return Some(f64::from(y));
        }
        y -= 1;
    }
    None
}
/// The fangs count down and bite when `warmup` reaches -8 (`EvokerFangs.tick`), then are removed.
fn pending(w: &mut World) {
    let mut bites: Vec<(Vec3, Entity, &'static [&'static str])> = Vec::new();
    let mut gone: Vec<Entity> = Vec::new();
    for (f, mut fang) in w.query::<(Entity, &mut Fang)>().iter_mut(w) {
        fang.delay -= 1;
        if fang.delay == -8 {
            bites.push((fang.at, fang.owner, fang.allies));
        }
        if fang.delay <= -8 {
            gone.push(f);
        }
    }
    for f in gone {
        w.despawn(f);
        if let Some(i) = crate::species::registry().module_id(&MODULE) {
            let c = &mut w.resource_mut::<Carriers>().0[i];
            *c = c.saturating_sub(1);
        }
    }
    for (at, owner, allies) in bites {
        bite(w, at, owner, allies);
    }
}
/// `EvokerFangs.dealDamageTo`: 6 of indirect magic to everything alive in the box (0.5 wide inflated 0.2 sideways, 0.8 high) but the
/// owner and its allies; a missing owner hurts everyone.
fn bite(w: &mut World, at: Vec3, owner: Entity, allies: &'static [&'static str]) {
    let half = 0.45;
    let b = Aabb::new(Vec3::new(at.x - half, at.y, at.z - half), Vec3::new(at.x + half, at.y + 0.8, at.z + half));
    let owner_alive = w.get::<Health>(owner).is_some_and(|h| h.0 > 0.0);
    let by = owner_alive.then_some(Who::Entity(owner));
    let set = crate::behaviours::avoid_entity::species_mask(crate::behaviours::avoid_entity::Avoid::Named(allies));
    let from = (at.x, at.z);
    let victims: Vec<Entity> = w
        .query::<(Entity, &Bounds, &SpeciesId, &Health)>()
        .iter(w)
        .filter(|(v, bb, s, h)| h.0 > 0.0 && *v != owner && bb.0.intersects(b) && !(owner_alive && set.contains(s.0)))
        .map(|(v, ..)| v)
        .collect();
    let tick = w.resource::<Clock>().0;
    for v in victims {
        w.resource_mut::<DamageQueue>().0.push(Hit { target: v, amount: 6.0, cause: Cause::Magic, from: Some(from), by });
    }
    let mut players: Vec<u32> = Vec::new();
    w.resource::<Online>().for_near(at.x, at.z, 2.0, |pl| {
        if crate::targeting::player_box(&pl).intersects(b) {
            players.push(pl.id);
        }
    });
    for id in players {
        w.resource_mut::<PlayerState>().hurt(tick, id, 6.0, from, by);
    }
}
