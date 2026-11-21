//! `FollowTemptation` (`AnimalBrain` `B::FollowTemptation`, the tadpole's idle activity, priority 1; `animal_brain.rs:1688-1730`,
//! `:1841-1846`): while a player tempts the entity it looks at him and walks to him, unless it is within `close` blocks. When it
//! stops, the temptation cooldown starts (100 ticks, counted down by `count_down`). The start conditions (the temptation is
//! present, no cooldown, no breed target, not panicking) are the entry's `needs`; its duration (60) is the entry's.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::d2;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:follow_temptation",
    mems: &[&TEMPTING_PLAYER, &TEMPTATION_COOLDOWN_TICKS, &WALK_TARGET, &LOOK_TARGET, &BREED_TARGET, &IS_PANICKING],
    requires: &[Cap::Body, Cap::Locomotion],
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// The walk's close-enough distance in blocks (the `WalkTarget` of the `Mob`: `close: 2`).
const WALK_CLOSE: i32 = 2;
/// The cooldown left when it stops (`temptation_cooldown_ticks.set(100)`).
const COOLDOWN_TICKS: i64 = 100;

/// The species' values: the walk speed (1.25 for the tadpole), how close is close enough in blocks (`follow_temptation_close`:
/// 2.5 for the tadpole, `animal_brain.rs:2027`), and whether the walk and the look aim at the player's eyes.
pub struct Params {
    pub speed: f32,
    pub close: f64,
    pub look_in_the_eyes: bool,
    /// The speed when the entity is out of water (the axolotl: 0.15, 0.5 in water); none: `speed` always.
    pub dry_speed: Option<f32>,
}
impl Params {
    pub const DEFAULT: Params = Params { speed: 1.25, close: 2.5, look_in_the_eyes: false, dry_speed: None };
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let player = match get(w, e, &TEMPTING_PLAYER) {
        Some(MemValue::Who(who)) => *who,
        _ => return false,
    };
    if present(w, e, &BREED_TARGET) || present(w, e, &IS_PANICKING) {
        return false;
    }
    let p = c.get::<Params>();
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(player, true)), None);
    let Some(me) = w.get::<Pos>(e).map(|q| q.0) else { return false };
    let near = crate::targeting::info(w, player).is_some_and(|i| d2(me, i.pos) < p.close * p.close);
    if near {
        erase(w, e, &WALK_TARGET);
    } else {
        let target = Tracker::Entity(player, p.look_in_the_eyes);
        let wet = w.get::<crate::cow::FluidState>(e).is_some_and(|f| f.in_water);
        let speed = if wet { p.speed } else { p.dry_speed.unwrap_or(p.speed) };
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target, speed, close: WALK_CLOSE }), None);
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    // The cooldown is a memory that expires (a deadline of the wheel), not a count written each tick.
    set(w, e, &TEMPTATION_COOLDOWN_TICKS, MemValue::Int(COOLDOWN_TICKS), Some(COOLDOWN_TICKS as u64));
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::goals::{Online, Player};
    use crate::targeting::Who;

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    /// The entry's conditions as the tadpole's configuration states them.
    static NEEDS: Needs = &[(&TEMPTING_PLAYER, Need::Present), (&TEMPTATION_COOLDOWN_TICKS, Need::Absent), (&BREED_TARGET, Need::Absent), (&IS_PANICKING, Need::Absent)];
    static ENTRIES: [BEntry; 1] = [BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 1, params: &Params::DEFAULT, needs: NEEDS, duration: Some((60, 60)), kids: &[] }];

    fn brained(h: &mut Herd) -> Entity {
        let def = BrainDef {
            sensors: &[],
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: &ENTRIES,
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        start(&mut h.world, e);
        e
    }
    fn player(h: &mut Herd, x: f64) {
        h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    }
    fn tempt(h: &mut Herd, e: Entity) {
        set(&mut h.world, e, &TEMPTING_PLAYER, MemValue::Who(Who::Player(1)), None);
    }
    fn step(h: &mut Herd, e: Entity) {
        wake(&mut h.world, e);
        brain_step(&mut h.world);
    }

    /// A tempting player at 8 blocks: the entity looks at his eyes and walks to his feet at 1.25, close 2 (the start is the
    /// step it is tempted in: the tick of the start runs).
    #[test]
    fn it_looks_at_a_tempting_player_and_walks_to_him() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        player(&mut h, 8.5);
        tempt(&mut h, e);
        step(&mut h, e);
        assert_eq!(look_target(&h.world, e), Some(Tracker::Entity(Who::Player(1), true)));
        assert_eq!(walk_target(&h.world, e), Some(WalkTarget { target: Tracker::Entity(Who::Player(1), false), speed: 1.25, close: 2 }));
    }

    /// Without a temptation, or with a cooldown, it does not start; without the cooldown, the same temptation starts it.
    #[test]
    fn it_does_not_start_without_a_temptation_or_during_a_cooldown() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        player(&mut h, 8.5);
        step(&mut h, e);
        assert_eq!(look_target(&h.world, e), None, "sans tentation, pas de départ");
        tempt(&mut h, e);
        set(&mut h.world, e, &TEMPTATION_COOLDOWN_TICKS, MemValue::Int(5), None);
        step(&mut h, e);
        assert_eq!(look_target(&h.world, e), None, "un délai de tentation interdit le départ");
        erase(&mut h.world, e, &TEMPTATION_COOLDOWN_TICKS);
        step(&mut h, e);
        assert!(look_target(&h.world, e).is_some(), "témoin : sans délai, le même joueur fait partir le but");
    }

    /// Near him (2.5 blocks, the tadpole's close) the walk target is cleared while the look stays; far again, it is set.
    #[test]
    fn it_stops_walking_when_close_and_resumes_when_he_moves_away() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        player(&mut h, 8.5);
        tempt(&mut h, e);
        step(&mut h, e);
        assert!(walk_target(&h.world, e).is_some(), "témoin : il marche vers le joueur loin");
        player(&mut h, 1.5);
        step(&mut h, e);
        assert_eq!(walk_target(&h.world, e), None, "à moins de 2,5 blocs, il ne marche plus");
        assert!(look_target(&h.world, e).is_some(), "il regarde toujours le joueur");
        player(&mut h, 8.5);
        step(&mut h, e);
        assert!(walk_target(&h.world, e).is_some(), "il repart quand le joueur s'éloigne");
    }

    /// When the temptation is gone, the behaviour stops: the cooldown is 100 ticks, the walk and the look are cleared.
    #[test]
    fn it_stops_when_the_temptation_ends_and_starts_the_cooldown() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        player(&mut h, 8.5);
        tempt(&mut h, e);
        step(&mut h, e);
        assert!(walk_target(&h.world, e).is_some() && look_target(&h.world, e).is_some(), "témoin : le but tourne");
        assert_eq!(int(&h.world, e, &TEMPTATION_COOLDOWN_TICKS), None);
        erase(&mut h.world, e, &TEMPTING_PLAYER);
        step(&mut h, e);
        assert_eq!(int(&h.world, e, &TEMPTATION_COOLDOWN_TICKS), Some(100));
        assert_eq!(walk_target(&h.world, e), None);
        assert_eq!(look_target(&h.world, e), None);
    }
}
