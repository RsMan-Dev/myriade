//! `Wolf.WolfAvoidEntityGoal` (`mobs/tame.rs:395-456`, `AvoidLlamas.start` and `tick`): while the wolf runs from a llama it has
//! no attack target. No flag and no start of its own: a state module next to the avoid goal (`avoid_entity`, instance `avoid`),
//! which clears the target of each animal whose avoid goal runs. Also the llama's draw of the avoid goal (`strength_roll`,
//! the `target_roll` of `avoid_entity::Params`).
use super::*;
use crate::behaviours::avoid_entity::Avoiding;
use crate::rules::forget_target::strength_passes;
use crate::targeting::clear_target;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The nth instance of `avoid_entity` of the animal whose run clears the target.
    pub avoid: u8,
}
impl Params {
    pub const DEFAULT: Self = Self { avoid: 0 };
}
/// The forget state: which avoid instance of the animal it follows.
#[derive(Component, Clone, Copy, Debug)]
pub struct Forgets {
    pub avoid: u8,
}
/// A llama's strength (`Llama.strength`, the secondary variant, 1 to 5). Not yet a component of the species: a test or a
/// species that has llamas inserts it.
#[derive(Component, Clone, Copy, Debug)]
pub struct LlamaStrength(pub i32);

pub static MODULE: Module = Module {
    name: "forget_target",
    flags: 0,
    equip,
    can_use: never,
    after_change: Some(after_change),
    systems,
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Forgets { avoid: p.avoid });
}
/// A state module: nothing starts it.
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
/// The start of the avoid run clears the target in the same tick (`AvoidLlamas.start`): the framework tells this module
/// after each goal starts, and the run is visible then.
fn after_change(w: &mut World, e: Entity, c: Change) {
    if c != Change::Started {
        return;
    }
    let Some(f) = w.get::<Forgets>(e).copied() else { return };
    if w.get::<Avoiding>(e).is_some_and(|a| a.0 == f.avoid) {
        clear_target(w, e);
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(forget.in_set(slot));
}
/// Each animal of this module whose avoid goal runs loses its target (the start of the run, then each tick of it). Only the animals
/// that run (`Avoiding`, a sparse marker) are visited: a herd of wolves that flee nothing costs nothing here.
fn forget(mine: Query<(Entity, &Forgets, &Avoiding)>, mut reqs: ResMut<Requests>) {
    for (e, f, a) in &mine {
        if a.0 == f.avoid {
            reqs.0.push(Req::Call(e, clear));
        }
    }
}
fn clear(w: &mut World, e: Entity) {
    clear_target(w, e);
}
/// `Wolf.WolfAvoidEntityGoal.canUse`: the llama's strength against `nextInt(5)`, drawn once the llama is found: the uniform
/// draw `u` in [0, 1) gives `floor(5 u)`, so `strength >= floor(5 u)` is `u < (strength + 1) / 5`. A target without a strength
/// never passes (the `Mob` returns false).
pub fn strength_roll(w: &World, target: Entity, u: f64) -> bool {
    let Some(s) = w.get::<LlamaStrength>(target) else { return false };
    strength_passes(s.0, u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::avoid_entity;
    use crate::species::Config;
    use crate::targeting::{Target, Who};

    /// The avoid instance 0 runs from the process's species (a stand-in for a llama, which is not in the registry), with
    /// the llama's draw; the forget state follows it.
    fn wolf_with(strength_of_others: Option<i32>) -> (Herd, Entity, Entity) {
        // The species of the process is set by the first world built: the world comes first.
        let mut h = Herd::with_slab_awake(20, false, false);
        let name = species().name;
        let names: &'static [&'static str] = Box::leak(vec![name].into_boxed_slice());
        let avoid: &'static avoid_entity::Params = Box::leak(Box::new(avoid_entity::Params {
            what: avoid_entity::Avoid::Named(names),
            max_dist: 8.0,
            walk: 1.5,
            sprint: 1.5,
            target_roll: Some(strength_roll),
            ..avoid_entity::Params::DEFAULT
        }));
        let forget: &'static Params = Box::leak(Box::new(Params::DEFAULT));
        let entries: &'static [Entry] = Box::leak(vec![Entry::new(&avoid_entity::MODULE, 3, avoid), Entry::new(&MODULE, 3, forget)].into_boxed_slice());
        let mut sp = *species();
        sp.goals = Config { entries };
        let wolf = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        let other = spawn_species(&mut h.world, &sp, 4.5, 0.5, 9);
        // Goals only: a species' brain would walk both elsewhere.
        h.world.entity_mut(wolf).remove::<crate::brain::Brain>();
        h.world.entity_mut(other).remove::<crate::brain::Brain>();
        if let Some(s) = strength_of_others {
            h.world.entity_mut(other).insert(LlamaStrength(s));
        }
        for _ in 0..20 {
            h.tick();
        }
        (h, wolf, other)
    }

    #[test]
    fn a_run_from_a_strong_llama_drops_the_target() {
    if crate::cow::skip_unless_walker("a_run_from_a_strong_llama_drops_the_target") {
        return;
    }
        let (mut h, wolf, other) = wolf_with(Some(4));
        h.world.entity_mut(wolf).insert(Target::new(Who::Entity(other), 60, u8::MAX));
        h.world.resource_mut::<Requests>().0.push(Req::Start(wolf, 0));
        for _ in 0..4 {
            h.tick();
        }
        assert!(h.world.get::<Avoiding>(wolf).is_some(), "no run from a strength-4 llama 4 blocks away");
        assert!(h.world.get::<Target>(wolf).is_none(), "the target survived the run from the llama");
    }

    #[test]
    fn no_run_from_a_llama_whose_strength_never_passes_the_draw() {
        if crate::cow::skip_unless_walker("no_run_from_a_llama_whose_strength_never_passes_the_draw") {
            return;
        }
        let (mut h, wolf, _) = wolf_with(Some(-1));
        h.world.resource_mut::<Requests>().0.push(Req::Start(wolf, 0));
        for _ in 0..10 {
            h.tick();
        }
        assert!(h.world.get::<Avoiding>(wolf).is_none(), "a run although the draw never passes");
    }

    #[test]
    fn strength_roll_is_the_law_of_the_mob() {
        if crate::cow::skip_unless_walker("strength_roll_is_the_law_of_the_mob") {
            return;
        }
        let mut w = World::new();
        let weak = w.spawn(LlamaStrength(1)).id();
        let none = w.spawn_empty().id();
        // strength 1: passes for floor(5 u) <= 1, that is u < 2/5.
        assert!(strength_roll(&w, weak, 0.39));
        assert!(!strength_roll(&w, weak, 0.41));
        assert!(!strength_roll(&w, none, 0.0));
    }
}
