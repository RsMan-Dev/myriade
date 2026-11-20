//! `NearestPlayers` (`AnimalBrain.sense_players`): every scan (the period of the species), the nearest player who is alive, not a
//! spectator and strictly inside the follow range is the player the brain knows (`nearest_players`). It is visible when the
//! animal's sight test passes (`nearest_visible_player`), and attackable when it is visible and the difficulty is not Peaceful
//! and the player is not creative (`nearest_visible_targetable_player(s)`). Each memory is written, or erased when empty; no
//! expiry: the next scan rewrites it.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::*;
use crate::goals::{d2, Online};
use crate::hostile::PlayerState;
use crate::targeting::{Eye, FollowRange, Who};
use horizon_core::geometry::Vec3;

pub static SENSOR: Sensor = Sensor {
    name: "minecraft:nearest_players",
    mems: &[&NEAREST_PLAYERS, &NEAREST_VISIBLE_PLAYER, &NEAREST_VISIBLE_TARGETABLE_PLAYERS, &NEAREST_VISIBLE_TARGETABLE_PLAYER],
    scan,
    requires: &[Cap::Targets],
    ..Sensor::DEFAULT
};

/// The nearest player within the follow range (`distance_sqr < range²`), alive and not a spectator.
fn near_player(w: &World, e: Entity) -> Option<u32> {
    let (p, r) = (w.get::<Pos>(e)?.0, w.get::<FollowRange>(e)?.0);
    let ps = w.resource::<PlayerState>();
    let mut best: Option<(u32, f64)> = None;
    w.resource::<Online>().for_near(p.x, p.z, r, |pl| {
        let d = d2(p, Vec3::new(pl.x, pl.y, pl.z));
        if ps.alive(pl.id) && !ps.spectator(pl.id) && d < r * r && best.is_none_or(|b| d < b.1) {
            best = Some((pl.id, d));
        }
    });
    best.map(|b| b.0)
}

/// `Mob.targetable` for a player (`TargetingConditions.forNonCombat`): alive, not a spectator, within the follow range scaled by
/// the player's visibility (at least 2 blocks), and in sight of the animal. Creative and Peaceful do not count here.
fn visible_to(w: &World, e: Entity, id: u32) -> bool {
    let ps = w.resource::<PlayerState>();
    if !ps.alive(id) || ps.spectator(id) {
        return false;
    }
    let (Some(pos), Some(eye), Some(range), Some(i)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Eye>(e), w.get::<FollowRange>(e), crate::targeting::info(w, Who::Player(id))) else {
        return false;
    };
    let reach = crate::targeting::sight_range(range.0, ps.visibility(id));
    if d2(pos, i.pos) > reach * reach {
        return false;
    }
    let mw = w.resource::<Level>().mw.0;
    mw.line_of_sight_for(Some(pos.y), Vec3::new(pos.x, pos.y + eye.0, pos.z), Vec3::new(i.pos.x, i.eye, i.pos.z))
}

fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let near = near_player(w, e);
    let visible = near.filter(|&id| visible_to(w, e, id));
    let attackable = visible.filter(|&id| {
        let ps = w.resource::<PlayerState>();
        !ps.peaceful && !ps.creative(id)
    });
    set(w, e, &NEAREST_PLAYERS, MemValue::Whos(near.map(Who::Player).into_iter().collect()), None);
    write_one(w, e, &NEAREST_VISIBLE_PLAYER, visible);
    set(w, e, &NEAREST_VISIBLE_TARGETABLE_PLAYERS, MemValue::Whos(attackable.map(Who::Player).into_iter().collect()), None);
    write_one(w, e, &NEAREST_VISIBLE_TARGETABLE_PLAYER, attackable);
}

/// A memory of one player: written, or erased when there is none.
fn write_one(w: &mut World, e: Entity, k: &MemKey, id: Option<u32>) {
    match id {
        Some(id) => set(w, e, k, MemValue::Who(Who::Player(id)), None),
        None => erase(w, e, k),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{spawn_species, Herd, SpeciesTable};
    use crate::goals::Player;

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];

    /// A cow with a brain holding this sensor only, its first scan armed by `start`.
    fn brained(h: &mut Herd, x: f64, z: f64) -> Entity {
        static ENTRIES: [SensorEntry; 1] = [SensorEntry::new(&SENSOR, 20)];
        let def = BrainDef {
            sensors: &ENTRIES,
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: &[],
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, x, z, 77);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        start(&mut h.world, e);
        e
    }
    fn player(h: &mut Herd, id: u32, x: f64, z: f64) {
        h.world.resource_mut::<Online>().set(Player { id, x, y: 1.0, z, main: 0, off: 0, ..Default::default() });
    }
    fn known(h: &Herd, e: Entity) -> Option<Vec<Who>> {
        match get(&h.world, e, &NEAREST_PLAYERS) {
            Some(MemValue::Whos(l)) => Some(l.clone()),
            _ => None,
        }
    }
    fn one(h: &Herd, e: Entity, k: &MemKey) -> Option<Who> {
        match get(&h.world, e, k) {
            Some(MemValue::Who(w)) => Some(*w),
            _ => None,
        }
    }

    /// A player at 4 blocks is known, visible and attackable; the same player far away is forgotten by the next scan.
    #[test]
    fn a_near_player_is_known_and_a_far_one_is_forgotten() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h, 0.5, 0.5);
        player(&mut h, 1, 4.5, 0.5);
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(known(&h, e), Some(vec![Who::Player(1)]));
        assert_eq!(one(&h, e, &NEAREST_VISIBLE_PLAYER), Some(Who::Player(1)));
        assert_eq!(one(&h, e, &NEAREST_VISIBLE_TARGETABLE_PLAYER), Some(Who::Player(1)));
        player(&mut h, 1, 30.5, 0.5);
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(known(&h, e), None);
        assert_eq!(one(&h, e, &NEAREST_VISIBLE_PLAYER), None);
    }

    /// A creative player is seen but not attacked: the attackable memories are absent while the others are present.
    #[test]
    fn a_creative_player_is_visible_but_not_attackable() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h, 0.5, 0.5);
        player(&mut h, 1, 4.5, 0.5);
        h.world.resource_mut::<PlayerState>().creative = vec![false, true];
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(one(&h, e, &NEAREST_VISIBLE_PLAYER), Some(Who::Player(1)));
        assert_eq!(one(&h, e, &NEAREST_VISIBLE_TARGETABLE_PLAYER), None);
        assert_eq!(known(&h, e), Some(vec![Who::Player(1)]));
    }

    /// A spectator is not known at all (`!p.spectator`): the witness is the same player, known once he stops spectating.
    #[test]
    fn a_spectator_is_not_known() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h, 0.5, 0.5);
        player(&mut h, 1, 4.5, 0.5);
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert!(known(&h, e).is_some());
        h.world.resource_mut::<PlayerState>().spectator = vec![false, true];
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(known(&h, e), None);
    }
}
