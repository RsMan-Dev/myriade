//! `FollowAdult` (`BabyFollowAdult`, `animal_brain.rs:1476-1525`; one-shot, the `Idle` activity of the ghastling and the axolotl):
//! a baby that sees an adult (or a player, with `player`) between `min` and `max` blocks walks to it, close enough at `close`,
//! and looks at it. The walk follows the adult's position as it moves (a tracker, read when walked). The start conditions
//! (no walk target, the entity is a baby, the target in range) are checked here; the sensor's memory is the entry's source.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::{Pos, GrowAt};
use crate::goals::d2;
use crate::rules::follow_adult as rules;
use crate::targeting::info;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:baby_follow_adult",
    mems: &[&NEAREST_VISIBLE_ADULT, &NEAREST_VISIBLE_PLAYER, &WALK_TARGET, &LOOK_TARGET],
    requires: &[Cap::Brain, Cap::Locomotion],
    one_shot: true,
    can_start,
    ..Behaviour::DEFAULT
};

/// The species' values: the walk speed (1.1; the axolotl's `axolotl_speed`), the range in blocks (`min` included, `max`
/// excluded: `closerThan(17)` and not `closerThan(3)`), how close is close enough (2; the axolotl 4), whether the walk aims at
/// the eyes (not the axolotl), and whether it follows a player (`Idle` 2 of the ghastling) or an adult.
pub struct Params {
    pub speed: f32,
    pub min: f64,
    pub max: f64,
    pub close: i32,
    pub eyes: bool,
    pub player: bool,
    /// The speed when the entity is out of water (the axolotl: 0.15, 0.6 in water); none: `speed` always.
    pub dry_speed: Option<f32>,
}
impl Params {
    pub const DEFAULT: Params = Params { speed: 1.1, min: 3.0, max: 17.0, close: 2, eyes: true, player: false, dry_speed: None };
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let key = if p.player { &NEAREST_VISIBLE_PLAYER } else { &NEAREST_VISIBLE_ADULT };
    let who = match get(w, e, key) {
        Some(MemValue::Who(x)) => *x,
        _ => return false,
    };
    if present(w, e, &WALK_TARGET) || w.get::<GrowAt>(e).is_none() {
        return false;
    }
    let Some(at) = info(w, who).map(|i| i.pos) else { return false };
    let Some(me) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    if !rules::in_follow_range(d2(me, at), p.min, p.max) {
        return false;
    }
    let wet = w.get::<crate::cow::FluidState>(e).is_some_and(|f| f.in_water);
    let speed = rules::walk_speed(wet, p.speed, p.dry_speed);
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Entity(who, p.eyes), speed, close: p.close }), None);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{spawn_species, ActiveUntil, Herd, SpeciesTable};
    use crate::goals::{Online, Player};

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static NEEDS: Needs = &[];
    static ADULT: Params = Params { speed: 1.1, min: 3.0, max: 17.0, close: 2, eyes: true, player: false, dry_speed: None };
    static PLAYER: Params = Params { player: true, ..Params::DEFAULT };
    static ADULT_ENTRY: [BEntry; 1] = [BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 3, params: &ADULT, needs: NEEDS, duration: None, kids: &[] }];
    static PLAYER_ENTRY: [BEntry; 1] = [BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 2, params: &PLAYER, needs: NEEDS, duration: None, kids: &[] }];

    /// A cow (a baby with `baby`) with this entry's behaviour; the test sets its memories.
    fn brained(h: &mut Herd, entries: &'static [BEntry], baby: bool) -> Entity {
        let def = BrainDef {
            sensors: &[],
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: entries,
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let e = creature(h, 0.5, 0.5, baby);
        h.world.entity_mut(e).insert(bundle(plan));
        start(&mut h.world, e);
        e
    }
    fn creature(h: &mut Herd, x: f64, z: f64, baby: bool) -> Entity {
        let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").expect("cow");
        let e = spawn_species(&mut h.world, &sp, x, z, 0x77 + (x * 7.0) as u64);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        if baby {
            h.world.entity_mut(e).insert(GrowAt(u64::MAX));
        }
        e
    }
    fn step(h: &mut Herd, e: Entity) {
        wake(&mut h.world, e);
        brain_step(&mut h.world);
    }
    fn walk(h: &Herd, e: Entity) -> Option<WalkTarget> {
        walk_target(&h.world, e)
    }

    /// A baby with an adult cow 8 blocks away starts walking to its eyes at 1.1, close 2, and looks at it.
    #[test]
    fn a_baby_walks_to_an_adult_at_eight_blocks() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &ADULT_ENTRY, true);
        let adult = creature(&mut h, 8.5, 0.5, false);
        set(&mut h.world, e, &NEAREST_VISIBLE_ADULT, MemValue::Who(Who::Entity(adult)), None);
        step(&mut h, e);
        assert_eq!(walk(&h, e), Some(WalkTarget { target: Tracker::Entity(Who::Entity(adult), true), speed: 1.1, close: 2 }));
        assert_eq!(look_target(&h.world, e), Some(Tracker::Entity(Who::Entity(adult), true)));
    }

    /// It keeps following the adult: the walk's target is the adult, so it aims where the adult now is.
    #[test]
    fn the_walk_follows_the_adult_as_it_moves() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &ADULT_ENTRY, true);
        let adult = creature(&mut h, 8.5, 0.5, false);
        set(&mut h.world, e, &NEAREST_VISIBLE_ADULT, MemValue::Who(Who::Entity(adult)), None);
        step(&mut h, e);
        h.world.get_mut::<Pos>(adult).unwrap().0.x = 12.5;
        let at = walk(&h, e).unwrap().target.position(&h.world).unwrap();
        assert!((at[0] - 12.5).abs() < 1e-9, "témoin : la cible suit l'adulte, pas le point du départ");
    }

    /// Too close (2 blocks), too far (20 blocks), not a baby, or a walk already held: no start. The same baby at 8 blocks
    /// starts, so the refusals come from the distance and the state.
    #[test]
    fn it_does_not_start_too_close_too_far_or_when_grown_or_walking() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &ADULT_ENTRY, true);
        let near = creature(&mut h, 2.5, 0.5, false);
        set(&mut h.world, e, &NEAREST_VISIBLE_ADULT, MemValue::Who(Who::Entity(near)), None);
        step(&mut h, e);
        assert_eq!(walk(&h, e), None, "à 2 blocks, pas de départ");
        let far = creature(&mut h, 20.5, 0.5, false);
        set(&mut h.world, e, &NEAREST_VISIBLE_ADULT, MemValue::Who(Who::Entity(far)), None);
        step(&mut h, e);
        assert_eq!(walk(&h, e), None, "à 20 blocks, pas de départ");
        let mid = creature(&mut h, 8.5, 0.5, false);
        set(&mut h.world, e, &NEAREST_VISIBLE_ADULT, MemValue::Who(Who::Entity(mid)), None);
        h.world.entity_mut(e).remove::<GrowAt>();
        step(&mut h, e);
        assert_eq!(walk(&h, e), None, "un adulte ne suit personne");
        h.world.entity_mut(e).insert(GrowAt(u64::MAX));
        set(&mut h.world, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point([1.0, 1.0, 1.0]), speed: 1.0, close: 0 }), None);
        step(&mut h, e);
        assert_eq!(walk(&h, e).map(|w| w.target), Some(Tracker::Point([1.0, 1.0, 1.0])), "une marche en cours n'est pas remplacée");
    }

    /// The player variant follows the nearest visible player, not the adult.
    #[test]
    fn the_player_variant_follows_the_player() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h, &PLAYER_ENTRY, true);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 8.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        set(&mut h.world, e, &NEAREST_VISIBLE_PLAYER, MemValue::Who(Who::Player(1)), None);
        step(&mut h, e);
        assert_eq!(walk(&h, e).map(|w| w.target), Some(Tracker::Entity(Who::Player(1), true)), "témoin : il suit le joueur");
    }
}
