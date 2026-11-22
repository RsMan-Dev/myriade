//! `ValidatePlayDead` (`AxolotlAi`, core 2, after the move): while the play dead count is set, it takes one off each tick; at zero it
//! forgets the play dead and its culprit (`HURT_BY_ENTITY`), and the activity whose memory is gone falls back to the next one.
use crate::behaviours::Cap;
use crate::brain::behaviours::play_dead::PLAY_DEAD_TICKS;
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:validate_play_dead", mems: &[&PLAY_DEAD_TICKS, &HURT_BY_ENTITY], requires: &[Cap::Brain], can_start, tick, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    present(w, e, &PLAY_DEAD_TICKS)
}

fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    match int(w, e, &PLAY_DEAD_TICKS) {
        Some(v) if v > 0 => {
            set(w, e, &PLAY_DEAD_TICKS, MemValue::Int(v - 1), None);
            true
        }
        _ => {
            erase(w, e, &PLAY_DEAD_TICKS);
            erase(w, e, &HURT_BY_ENTITY);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::targeting::Who;

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static ENTRIES: [BEntry; 1] = [BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[], duration: None, kids: &[] }];

    fn brained(h: &mut Herd) -> Entity {
        let def = BrainDef { sensors: &[], activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }], pick: &[], initial: &[], behaviours: &ENTRIES };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        start(&mut h.world, e);
        e
    }
    fn ctx() -> Ctx {
        Ctx { id: 0, nth: 0, params: &() }
    }

    /// With a count, each tick takes one off and the behaviour runs on; the culprit stays.
    #[test]
    fn it_counts_the_play_dead_down_each_tick() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        set(&mut h.world, e, &PLAY_DEAD_TICKS, MemValue::Int(5), None);
        set(&mut h.world, e, &HURT_BY_ENTITY, MemValue::Who(Who::Player(1)), None);
        assert!((BEHAVIOUR.tick)(&mut h.world, e, &ctx()));
        assert_eq!(int(&h.world, e, &PLAY_DEAD_TICKS), Some(4));
        assert!(present(&h.world, e, &HURT_BY_ENTITY));
    }

    /// At zero, the play dead and the culprit are forgotten and the behaviour stops.
    #[test]
    fn it_forgets_the_play_dead_and_the_culprit_at_zero() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        set(&mut h.world, e, &PLAY_DEAD_TICKS, MemValue::Int(0), None);
        set(&mut h.world, e, &HURT_BY_ENTITY, MemValue::Who(Who::Player(1)), None);
        assert!(!(BEHAVIOUR.tick)(&mut h.world, e, &ctx()));
        assert!(!present(&h.world, e, &PLAY_DEAD_TICKS));
        assert!(!present(&h.world, e, &HURT_BY_ENTITY));
    }
}
