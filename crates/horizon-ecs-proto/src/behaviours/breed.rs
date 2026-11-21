//! `BreedGoal`: an animal in love walks to the nearest other animal in love of its species and, after the delay,
//! gives birth to a baby. Love is given by `feed`.
use super::*;
use crate::rules::breed::{birth_due, birth_orbs, gives_up, inherited_deadline, is_selector_tick, love_ticks};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The range within which a partner is looked for.
    pub range: f64,
    /// `reducedTickDelay(60)`: ticks before the birth.
    pub delay: u64,
    /// Ticks of love after a feeding, ticks of rest after a birth, age of a newborn (ticks before it grows up).
    pub love: u64,
    pub rest: u64,
    pub baby_age: u64,
    /// The items that put the animal in love (`Animal.isFood`), by name.
    pub foods: &'static [&'static str],
    /// `finalizeSpawnChildFromBreeding`: with the `mob_drops` rule on, a draw of `1 + nextInt(7)` orbs at the parents'
    /// place. False for a species whose birth gives no experience.
    pub xp: bool,
    /// The birth is a pregnancy, not a baby (`Frog.spawnChildFromBreeding`): the parent that breeds takes `Carrying`, both rest, and the
    /// species' behaviour (`lay_spawn`) does the rest.
    pub carries: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, range: 8.0, delay: 60, love: 600, rest: 6000, baby_age: 24000, foods: &[], xp: true, carries: false };
}
/// The parameters; `slot` is the goal's rank, `partner` what `can_use` chose for `start`.
#[derive(Component, Clone, Copy, Debug)]
pub struct BreedData {
    /// The species' parameters, referenced (read through `Deref`): never copied into the entity.
    p: &'static Params,
    pub slot: u8,
    partner: Option<Entity>,
}
impl std::ops::Deref for BreedData {
    type Target = Params;
    fn deref(&self) -> &Params {
        self.p
    }
}
/// `BreedData.slot` of an animal that breeds by its brain (`make_love`), not by a goal.
pub const NO_SLOT: u8 = u8::MAX;
/// In love until this tick; resting (age 6000) until this tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_remove = crate::behaviours::long_jump::wake_gated)]
pub struct Love(pub u64);
#[derive(Component, Clone, Copy, Debug)]
pub struct Resting(pub u64);
/// The parent carries what it will lay (`Params.carries`): erased by the behaviour that lays it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Carrying;
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Breeding {
    pub partner: Entity,
    pub since: u64,
    pub aim: (i32, i32),
}

pub static MODULE: Module = Module {
    name: "breed",
    flags: MOVE | LOOK,
    equip,
    fired,
    can_use,
    start,
    stop,
    systems,
    damaged,
    interact,
    requires: &[Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
/// `Animal.mobInteract`: a player's food puts an adult in love (the first module to answer true is the only one).
fn interact(w: &mut World, e: Entity, _: Inst, i: &Interaction) -> bool {
    let Some(b) = w.get::<BreedData>(e).copied() else { return false };
    if i.item == 0 || !b.foods.iter().any(|f| crate::items::is(f, i.item)) {
        return false;
    }
    feed(w, e)
}
/// `Animal.actuallyHurt`: damage taken ends love.
fn damaged(w: &mut World, e: Entity, _: Inst, _: &Damaged) {
    w.entity_mut(e).remove::<Love>();
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(BreedData { p, slot: inst.slot, partner: None });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(breed_run.in_set(slot));
}
/// `Animal.setInLove` by feeding: refused for a baby, a resting or a loving animal. Wakes the goal, and the loving
/// animals around (their partner just appeared).
pub fn feed(world: &mut World, e: Entity) -> bool {
    let now = crate::cow::now_of(world, e);
    if world.get::<GrowAt>(e).is_some()
        || world.get::<Resting>(e).is_some_and(|r| r.0 > now)
        || world.get::<Love>(e).is_some_and(|l| l.0 > now)
    {
        return false;
    }
    // An animal that died is not fed.
    let Some(b) = world.get::<BreedData>(e).copied() else { return false };
    world.entity_mut(e).insert(Love(now + b.love));
    let p = world.get::<Pos>(e).unwrap().0;
    let mut near = Vec::new();
    world.resource::<Index>().for_radius(p.x, p.z, b.range, |c| near.push(c));
    let mut reqs = Vec::new();
    let mut wake_brain = vec![e];
    if b.slot != NO_SLOT {
        reqs.push(Req::Start(e, b.slot));
    }
    for c in near {
        if c != e
            && world.get::<Love>(c).is_some_and(|l| l.0 > crate::cow::now_of(world, c))
            && let Some(d) = world.get::<BreedData>(c)
        {
            // A goal animal's selector is asked; a brain animal's brain is woken (its `make_love` looks for a partner).
            if d.slot == NO_SLOT {
                wake_brain.push(c);
            } else {
                reqs.push(Req::Start(c, d.slot));
            }
        }
    }
    world.resource_mut::<Requests>().0.extend(reqs);
    for c in wake_brain {
        crate::brain::wake(world, c);
    }
    true
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    !w.get::<Love>(e).is_none_or(|l| l.0 <= now)
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let Some(b) = w.get::<BreedData>(e).copied() else { return false };
    if w.get::<Love>(e).is_none_or(|l| l.0 <= now) {
        return false;
    }
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, b.range, |c| near.push(c));
    let mut best: Option<(Entity, f64)> = None;
    for c in near {
        let Some(cp) = w.get::<Pos>(c) else { continue };
        let d = d2(pos, cp.0);
        let lovers = w.get::<SpeciesId>(c) == w.get::<SpeciesId>(e) && w.get::<Love>(c).is_some_and(|l| l.0 > crate::cow::now_of(w, c)) && w.get::<GrowAt>(c).is_none() && w.get::<Panicking>(c).is_none();
        if c != e && lovers && d <= b.range * b.range && best.is_none_or(|x| d < x.1) {
            best = Some((c, d));
        }
    }
    let forced = w.get::<ForcedPartner>(e).copied();
    if forced.is_some() {
        w.entity_mut(e).remove::<ForcedPartner>();
    }
    if let Some(ForcedPartner(c)) = forced
        && w.get::<Love>(c).is_some_and(|l| l.0 > crate::cow::now_of(w, c))
    {
        best = Some((c, 0.0));
    }
    match best {
        Some((c, _)) => {
            w.get_mut::<BreedData>(e).unwrap().partner = Some(c);
            true
        }
        None => {
            // No partner yet: the next look is a deadline, not a poll of every tick.
            arm(w, e, inst.slot, 10);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let now = crate::cow::now_of(w, e);
    let Some(c) = w.get::<BreedData>(e).and_then(|b| b.partner) else { return };
    w.entity_mut(e).insert(Breeding { partner: c, since: now, aim: (i32::MIN, 0) });
    wake(w, c, &MODULE);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Breeding>();
}
/// `BreedGoal.canContinue` and `tick`: walk to the partner, and after the delay, if close, give birth.
#[allow(clippy::type_complexity)]
fn breed_run(
    clock: Res<Clock>,
    mut q: Query<(Aim, &mut Breeding, &BreedData, Option<&Love>, Option<&Phase>)>,
    partners: Query<(&Pos, Option<&Love>, Has<Panicking>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    let mut births: Vec<(Entity, Entity)> = Vec::new();
    for ((e, pos, mut look, looking, following, asked), mut b, data, love, phase) in &mut q {
        let now = clock.of(e);
        let partner_now = clock.of(b.partner);
        let partner = partners.get(b.partner).ok().filter(|(_, l, panicking)| l.is_some_and(|l| l.0 > partner_now) && !panicking);
        let (Some((pp, _, _)), true) = (partner, love.is_some_and(|l| l.0 > now)) else {
            reqs.0.push(Req::End(e, data.slot));
            continue;
        };
        let at = [pp.0.x, pp.0.y + look.eye, pp.0.z];
        let d = d2(pos.0, pp.0);
        aim(&mut cmd, e, &mut look, looking, at, None);
        let a = (pp.0.x.floor() as i32, pp.0.z.floor() as i32);
        if a != b.aim || (!following && !asked && d >= 9.0) {
            b.aim = a;
            go_to(&mut cmd, e, pp.0.x, pp.0.z, data.speed, 3);
        }
        // `BreedGoal` counts `loveTime` once per selector tick (its `tick` runs then): it breeds when the count reaches
        // `reducedTickDelay(60)` and the partner is within 3 blocks, and gives up at 60.
        let phase = phase.map_or(0, |p| p.0);
        let love_time = love_ticks(now, b.since, phase);
        if birth_due(is_selector_tick(now, phase), love_time, data.delay, d) {
            let partner = b.partner;
            births.push((e, partner));
        } else if gives_up(love_time, data.delay) {
            reqs.0.push(Req::End(e, data.slot));
        }
    }
    // The births in an order of the entities, not of the table rows (the entities of the babies depend on it, and the table rows differ between the sequential and the parallel run).
    births.sort_unstable_by(|a, b| b.cmp(a));
    for (e, partner) in births {
        cmd.queue(move |w: &mut World| birth(w, e, partner));
    }
}
/// `BreedGoal.tick`'s end: the goal checks that it still runs and that love lasts, makes the birth and ends both goals.
fn birth(w: &mut World, a: Entity, b: Entity) {
    let now = crate::cow::now_of(w, a);
    if w.get::<Breeding>(a).is_none() || w.get::<Love>(a).is_none_or(|l| l.0 <= now) {
        return;
    }
    if w.get::<BreedData>(a).is_some_and(|d| d.carries) {
        finish_birth(w, a, b);
        w.entity_mut(a).insert(Carrying);
        w.resource_mut::<Events>().carrying.push(a);
    } else {
        give_birth(w, a, b);
    }
    let reqs: Vec<Req> = [a, b].iter().filter_map(|&p| w.get::<BreedData>(p).filter(|d| d.slot != NO_SLOT).map(|d| Req::End(p, d.slot))).collect();
    w.resource_mut::<Requests>().0.extend(reqs);
}
/// Whether `b` is a partner for `a` (`BreedGoal`'s lovers, `AnimalMakeLove.canMateWith`): the same species, both in love, `b` not a baby.
pub fn can_mate(w: &World, a: Entity, b: Entity) -> bool {
    let in_love = |x: Entity| w.get::<Love>(x).is_some_and(|l| l.0 > crate::cow::now_of(w, x));
    a != b && w.get::<SpeciesId>(a) == w.get::<SpeciesId>(b) && in_love(a) && in_love(b) && w.get::<GrowAt>(b).is_none()
}
/// `Animal.spawnChildFromBreeding`, public and shared by the goal and by the brain's `make_love`: a baby of age -`baby_age` of
/// the parents' species at the parent's place, both parents rest and love ends, the experience of the birth, the baby takes
/// its mother and its first deadlines (and its brain). The caller checked that the two are in love and ends its own behaviour.
pub fn give_birth(w: &mut World, a: Entity, b: Entity) -> Option<Entity> {
    let now = crate::cow::now_of(w, a);
    let data = *w.get::<BreedData>(a)?;
    let (pos, active) = (w.get::<Pos>(a)?.0, w.get::<ActiveUntil>(a)?.0);
    rest_parents(w, a, b);
    let seed = with_rng(w, a, |r| u64::from(r.next_u32()) << 32 | u64::from(r.next_u32()));
    // The baby is of its parents' species (the mate was checked to be the same).
    let baby = spawn_like(w, a, pos.x, pos.z, seed);
    crate::species::goat::inherit_screaming(w, baby, a, b);
    // The parent's deadline is in its own time: the baby's own time is the world's.
    let active_baby = inherited_deadline(active, now, crate::cow::now_of(w, baby));
    w.get_mut::<ActiveUntil>(baby).unwrap().0 = active_baby;
    make_baby(w, baby, data.baby_age);
    birth_experience(w, a, pos);
    adopt(w, a, baby);
    if active > now {
        stroll::arm_now(w, baby);
    }
    arm_timers(w, baby);
    crate::brain::start(w, baby);
    Some(baby)
}
/// The two parents rest (`Resting`, age 6000 in the `Mob`) and love ends.
pub fn rest_parents(w: &mut World, a: Entity, b: Entity) {
    let Some(data) = w.get::<BreedData>(a).copied() else { return };
    for e in [a, b] {
        let own = crate::cow::now_of(w, e);
        w.entity_mut(e).insert(Resting(own + data.rest)).remove::<Love>();
    }
}
/// A birth that is not a baby (the egg of a species that lays one): the parents rest and the experience of the birth, which the
/// species' behaviour follows with what it lays.
pub fn finish_birth(w: &mut World, a: Entity, b: Entity) {
    let Some(pos) = w.get::<Pos>(a).map(|p| p.0) else { return };
    rest_parents(w, a, b);
    birth_experience(w, a, pos);
}
/// `finalizeSpawnChildFromBreeding`: the draw only inside the `mob_drops` check, then the orbs at the parents' place.
fn birth_experience(w: &mut World, a: Entity, pos: horizon_core::geometry::Vec3) {
    let Some(data) = w.get::<BreedData>(a).copied() else { return };
    if data.xp && w.get_resource::<crate::orb::NoXp>().is_none() && w.get_resource::<Level>().is_some_and(|l| l.mw.0.mob_drops) {
        let amount = birth_orbs(with_rng(w, a, |r| r.next_int(7)));
        let orb_seed = with_rng(w, a, |r| u64::from(r.next_u32()) << 32 | u64::from(r.next_u32()));
        crate::orb::award(w, pos, amount, &mut horizon_core::random::LegacyRandom::new(orb_seed as i64));
    }
}

/// The love of an animal that breeds by its brain: `BreedData` without a goal's slot, and the food that puts it in love
/// (`Animal.mobInteract`), both from the parameters; `make_love` is the brain's behaviour.
pub static LOVE: crate::rules::Rule = crate::rules::Rule {
    name: "love",
    equip: |w, e, p| {
        let p = crate::rules::params::<Params>(p);
        w.entity_mut(e).insert(BreedData { p, slot: NO_SLOT, partner: None });
    },
    attrs: crate::rules::no_attrs,
    interact: |w, e, p, i| {
        let p = crate::rules::params::<Params>(p);
        i.item != 0 && p.foods.iter().any(|f| crate::goals::item_id(f) == i.item) && feed(w, e)
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_armed};
    use crate::orb::Orb;
    use crate::species::Config;

    /// Two cows in love side by side, with this module (its `xp` as given). Runs until a baby is born or 600 ticks pass.
    fn breed(xp: bool) -> Option<(u64, u32, usize)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 2, Box::leak(Box::new(Params { xp, ..Params::DEFAULT })))]));
        sp.goals = Config { entries };
        let a = spawn_armed(&mut h.world, &sp, 0.5, 0.5, 71);
        let b = spawn_armed(&mut h.world, &sp, 2.5, 0.5, 72);
        for e in [a, b] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.tick();
        assert!(feed(&mut h.world, a) && feed(&mut h.world, b));
        let mut born_at = None;
        for t in 1..=600_u64 {
            h.tick();
            if born_at.is_none() && h.world.query::<&GrowAt>().iter(&h.world).count() > 0 {
                born_at = Some(t);
            }
        }
        let born = born_at?;
        let mut total = 0;
        let mut stacks = 0;
        for o in h.world.query::<&Orb>().iter(&h.world) {
            total += o.value.max(0) as u32 * o.count;
            stacks += 1;
        }
        Some((born, total, stacks))
    }

    /// A birth with the rule on gives 1 to 7 experience at the parents' place; with the module's `xp` off, none
    /// (the baby is born in both cases: the positive witness).
    #[test]
    fn a_birth_gives_one_to_seven_experience_and_none_without_the_draw() {
        let Some((born, total, stacks)) = breed(true) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(born > 0, "témoin : un bébé naît");
        assert!((1..=7).contains(&total) && stacks >= 1, "un montant de 1 à 7 : {total}");
        let Some((born_off, total_off, _)) = breed(false) else { return };
        assert!(born_off > 0, "témoin : un bébé naît sans l'expérience");
        assert_eq!(total_off, 0, "sans le tirage, aucune orbe");
    }
}
