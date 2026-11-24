//! `SpellcasterUseSpellGoal`: a spell of a caster that has a living target: it waits `warmup` ticks after the start, then casts its
//! effect, and for `casting` ticks the caster stands still (`spell_casting`); it is tried again `interval` ticks after the start.
//! One entry per spell; the species picks the spells (`Spell`), the module knows no species. The state is one component for all the
//! entries of the caster (only one spell runs at a time, the cast being the lock): `next[nth]` the earliest start of each entry.
//!
//! Spells: `Vexes` (fewer than a roll of 1 to 8 summons around it within 16 blocks: three of species `species`, owned, limited to 30 to
//! 120 s of life), `Fangs` (a ring of 13 when the target is within 3 blocks, else a line of 16: they bite 6 magic damage when their
//! delay `+ 9` ticks is over, everything but the caster and its `allies`), `Disappear` (invisible for 60 s).
//! `Wololo` (no target, griefing on, a blue sheep within 16 x 4 x 16 scaled by its visibility to the caster: it turns red when the warm-up is over; the entry is asked again
//! every 2 ticks while the caster has no target, as the Mob's selector does), `Blindness` (not on the target it last blinded, only
//! on hard: the Mob reads the local difficulty above 2, which the ECS does not carry; blindness 20 s on a creature or a player).
//! Not done: the sounds, the game event of each vex; the fang floor test counts any
//! solid block with air above.
use super::spell_casting::Casting;
use super::*;
use crate::cow::*;
use crate::effects;
use crate::goals::*;
use crate::rules::spell_use as rules;
use crate::targeting::{self, Target, Who};
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub enum Spell {
    Vexes { species: &'static str },
    Fangs { allies: &'static [&'static str] },
    Disappear,
    Wololo,
    Blindness,
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub spell: Spell,
    /// The clients' spell id (`IllagerSpell`: 1 summon, 2 fangs, 3 wololo, 4 disappear, 5 blindness).
    pub id: u8,
    pub warmup: i32,
    pub casting: i32,
    pub interval: i64,
}
impl Params {
    pub const MIRROR: Self = Self { spell: Spell::Disappear, id: 4, warmup: 20, casting: 20, interval: 340 };
    /// `EvokerWololoSpellGoal`: warm-up 40, casting 60, every 140 ticks.
    pub const WOLOLO: Self = Self { spell: Spell::Wololo, id: 3, warmup: 40, casting: 60, interval: 140 };
    /// `IllusionerBlindnessSpellGoal`: warm-up 20, casting 20, every 180 ticks.
    pub const BLINDNESS: Self = Self { spell: Spell::Blindness, id: 5, warmup: 20, casting: 20, interval: 180 };
}
/// The entries of the caster: their ranks, the spell being warmed up (`running`: the entry's `nth`) and its warm-up.
#[derive(Component, Clone, Copy, Debug)]
pub struct Spells {
    slot: [u8; 3],
    p: [Option<Params>; 3],
    next: [u64; 3],
    running: u8,
    warmup: i32,
    /// The sheep a wololo aims at, or the last target a blindness hit (never both on one caster).
    mark: Option<Who>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Spelling;
pub static MODULE: Module = Module {
    name: "spell_use",
    flags: 0,
    resources,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Goals, Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};
/// A new target wakes every spell entry (the selector asks `can_use` again every 2 ticks while the target stays: `can_use` re-arms itself).
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        if let Some(s) = w.get::<Spells>(e) {
            let slots: Vec<u8> = s.slot.iter().copied().filter(|&x| x != u8::MAX).collect();
            for slot in slots {
                w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
            }
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let n = (inst.nth as usize).min(2);
    let p = *params::<Params>(p);
    if w.get::<Spells>(e).is_none() {
        w.entity_mut(e).insert(Spells { slot: [u8::MAX; 3], p: [None; 3], next: [0; 3], running: 0, warmup: 0, mark: None });
    }
    let mut s = w.get_mut::<Spells>(e).unwrap();
    s.slot[n] = inst.slot;
    s.p[n] = Some(p);
    // A wololo has no target to wake it: it asks itself every 2 ticks.
    if matches!(p.spell, Spell::Wololo) {
        crate::goals::arm(w, e, inst.slot, 2);
    }
}
fn alive_target(w: &World, e: Entity) -> bool {
    w.get::<Target>(e).is_some_and(|t| targeting::info(w, t.who).is_some_and(|i| !i.exempt))
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let ok = wants(w, e, inst);
    let wololo = w.get::<Spells>(e).and_then(|s| s.p[(inst.nth as usize).min(2)]).is_some_and(|p| matches!(p.spell, Spell::Wololo));
    if !ok && (wololo || w.get::<Target>(e).is_some()) {
        // Asked again in 2 ticks while there is a target (`Goal.canUse` is asked every 2 ticks): no scan, one deadline. While the entry cools
        // down the answers are known to be no: the deadline is the first asking (same 2-tick phase) at or after the end of the cooldown.
        let (now, n) = (now_of(w, e), (inst.nth as usize).min(2));
        let wait = w.get::<Spells>(e).map_or(0, |s| s.next[n].saturating_sub(now));
        crate::goals::arm(w, e, inst.slot, rules::recheck_in(wait));
    }
    ok
}
/// The world's query over the wool-bearing creatures, built on the first ask and kept.
#[derive(Resource)]
struct SheepQuery(QueryState<(Entity, &'static Pos, &'static crate::farm::Wool, &'static Health)>);
impl FromWorld for SheepQuery {
    fn from_world(w: &mut World) -> Self {
        SheepQuery(w.query())
    }
}

fn wants(w: &mut World, e: Entity, inst: Inst) -> bool {
    let n = (inst.nth as usize).min(2);
    let Some(s) = w.get::<Spells>(e).copied() else { return false };
    let Some(p) = s.p[n] else { return false };
    let wololo = matches!(p.spell, Spell::Wololo);
    if (!wololo && !alive_target(w, e)) || w.get::<Casting>(e).is_some() || now_of(w, e) < s.next[n] {
        return false;
    }
    match p.spell {
        // `EvokerWololoSpellGoal.canUse`: no target, griefing on, a blue sheep (colour 11) of the box, one of them at random.
        Spell::Wololo => {
            if w.get::<Target>(e).is_some() || !crate::level::can_grief(w, e) {
                return false;
            }
            let me = w.get::<Pos>(e).unwrap().0;
            // One query state for the world (built once): with no sheep nearby the pass costs one empty archetype walk, not a state built per ask.
            if !w.contains_resource::<SheepQuery>() {
                w.init_resource::<SheepQuery>();
            }
            // The range is `forNonCombat().range(16)` scaled by the sheep's visibility to the caster (`sight_range`, at least 2 blocks).
            let looker = w.get::<SpeciesId>(e).map(|s| s.0);
            let mut sheep: Vec<Entity> = w.resource_scope(|w: &mut World, mut qs: Mut<SheepQuery>| {
                qs.0.iter(w)
                    .filter(|(x, p, c, h)| {
                        c.color == 11 && h.0 > 0.0 && {
                            let reach = targeting::sight_range(16.0, targeting::creature_visibility_to(w, *x, looker));
                            rules::near_sheep(p.0.x - me.x, p.0.y - me.y, p.0.z - me.z, d2(p.0, me), reach)
                        }
                    })
                    .map(|(x, ..)| x)
                    .collect()
            });
            if sheep.is_empty() {
                return false;
            }
            sheep.sort();
            let pick = with_rng(w, e, |r| r.next_int(sheep.len() as i32)) as usize;
            w.get_mut::<Spells>(e).unwrap().mark = Some(Who::Entity(sheep[pick]));
            true
        }
        Spell::Blindness => {
            let who = w.get::<Target>(e).map(|t| t.who);
            effects::difficulty(w) >= 3 && who != s.mark
        }
        // `EvokerSummonSpellGoal`: fewer of its vexes within 16 blocks than a roll of 1 to 8.
        Spell::Vexes { species } => {
            let Some(id) = crate::species::registry().id(species) else { return false };
            let me = w.get::<Pos>(e).unwrap().0;
            let near = crate::queries::with::<(&'static Pos, &'static SpeciesId, &'static Health), (), _>(w, |q, w| q.iter(w).filter(|(p, s, h)| s.0 as usize == id && h.0 > 0.0 && d2(p.0, me) <= 256.0).count() as i32);
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
        s.warmup = p.warmup;
        s.next[n] = now + p.interval as u64;
        p
    };
    if matches!(p.spell, Spell::Blindness) {
        let who = w.get::<Target>(e).map(|t| t.who);
        w.get_mut::<Spells>(e).unwrap().mark = who;
    }
    w.entity_mut(e).insert((Casting { ticks: p.casting, spell: p.id }, Spelling));
    wake(w, e, &super::spell_casting::MODULE);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Spelling>();
    let n = (inst.nth as usize).min(2);
    let Some(mut s) = w.get_mut::<Spells>(e) else { return };
    if s.p[n].is_some_and(|p| matches!(p.spell, Spell::Wololo)) {
        s.mark = None;
        // The selector asks the goal again every 2 ticks while it has no target.
        crate::goals::arm(w, e, inst.slot, 2);
    }
}
/// `canContinueToUse`: a living target and a warm-up not over.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, warm);
}
fn warm(mut q: Query<(Entity, &mut Spells, Option<&Target>), With<Spelling>>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for (e, mut s, target) in &mut q {
        let slot = s.slot[s.running as usize];
        let wololo = s.p[s.running as usize].is_some_and(|p| matches!(p.spell, Spell::Wololo));
        if (wololo && s.mark.is_none()) || (!wololo && target.is_none()) {
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
        // `EvokerWololoSpellGoal.performSpellCasting`: the sheep, if it lives, turns red (colour 14).
        Spell::Wololo => {
            if let Some(Who::Entity(sheep)) = w.get::<Spells>(e).and_then(|s| s.mark) {
                if w.get::<Health>(sheep).is_some_and(|h| h.0 > 0.0) {
                    if let Some(mut c) = w.get_mut::<crate::farm::Wool>(sheep) {
                        c.color = 14;
                    }
                }
            }
        }
        // `IllusionerBlindnessSpellGoal.performSpellCasting`: blindness for 20 s on whatever it aims at.
        Spell::Blindness => match w.get::<Target>(e).map(|t| t.who) {
            Some(Who::Entity(t)) => {
                effects::add(w, t, effects::Kind::Blindness, 0, 400);
            }
            Some(Who::Player(id)) => effects::add_player(w, id, effects::Kind::Blindness, 0, 400),
            None => {}
        },
    }
}
/// `EvokerSummonSpellGoal.performSpellCasting`: three, at the evoker's block shifted by -2 to +2, a block up, living 30 to 120 s.
fn summon(w: &mut World, e: Entity, species: &'static str) {
    let Some(id) = crate::species::registry().id(species) else { return };
    let (Some(me), Some(sp)) = (w.get::<Pos>(e).map(|p| p.0), w.resource::<SpeciesTable>().0.get(id).copied()) else { return };
    let now = now_of(w, e);
    for _ in 0..3 {
        let (dx, dz, life, seed) = with_rng(w, e, |r| (r.next_int(5) - 2, r.next_int(5) - 2, rules::vex_life(r.next_int(90)), u64::from(r.next_u32())));
        let at = rules::summon_spot(me, dx, dz);
        let v = spawn_species(w, &sp, at.x, at.z, seed);
        w.get_mut::<Pos>(v).unwrap().0 = at;
        w.entity_mut(v).insert(crate::tame::Owner(Some(Who::Entity(e))));
        crate::goals::arm_timers(w, v);
        w.entity_mut(v).insert(crate::behaviours::limited_life::Lifespan(now + life as u64));
    }
}
/// `EvokerAttackSpellGoal.performSpellCasting`: a ring of 5 and a ring of 8 (delay 3) near, else a line of 16 (delay 0 to 15).
fn fangs(w: &mut World, e: Entity, _allies: &'static [&'static str]) {
    let Some(t) = w.get::<Target>(e).and_then(|t| targeting::info(w, t.who)) else { return };
    let me = w.get::<Pos>(e).unwrap().0;
    let (min_y, max_y) = (t.pos.y.min(me.y), t.pos.y.max(me.y) + 1.0);
    for (x, z, delay) in rules::fang_spots(me, t.pos, d2(me, t.pos) < 9.0) {
        if let Some(y) = floor(w, x, z, min_y, max_y) {
            crate::fangs::spawn_fang(w, Vec3::new(x, y, z), delay, Some(Who::Entity(e)));
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
