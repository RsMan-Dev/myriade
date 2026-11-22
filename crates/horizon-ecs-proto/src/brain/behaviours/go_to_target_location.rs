//! `GoToTargetLocation` (`PiglinAi` CELEBRATE, two instances: 2 blocks at speed 1.0 and 4 blocks at 0.6): with a block memory set and no
//! attack target or walk target, a piglin farther than `distance` from that block walks to a spot within one block of it
//! (`getNearbyPos`: the block offset by -1, 0 or 1 on x and z), and looks at it.
use crate::brain::*;
use crate::cow::Pos;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:go_to_target_location",
    mems: &[&WALK_TARGET, &LOOK_TARGET, &ATTACK_TARGET, &CELEBRATE_LOCATION],
    requires: &[Cap::Brain],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// The block memory the piglin goes to (`CELEBRATE_LOCATION`).
    pub memory: &'static MemKey,
    /// The walk ends within this many blocks (`closerThan`).
    pub distance: i32,
    pub speed: f32,
}

fn target(w: &World, e: Entity, p: &Params) -> Option<[i32; 3]> {
    match get(w, e, p.memory) {
        Some(MemValue::Block(b)) => Some(*b),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(at) = target(w, e, p) else { return false };
    let me = w.get::<Pos>(e).map(|q| q.0);
    let far = me.is_some_and(|m| {
        let d = [m.x.floor() - f64::from(at[0]), m.y.floor() - f64::from(at[1]), m.z.floor() - f64::from(at[2])];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= f64::from(p.distance) * f64::from(p.distance)
    });
    far && !present(w, e, &ATTACK_TARGET) && walk_target(w, e).is_none()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(at) = target(w, e, p) else { return };
    let dx = with_rng(w, e, |r| r.next_int(3)) - 1;
    let dz = with_rng(w, e, |r| r.next_int(3)) - 1;
    let spot = [at[0] + dx, at[1], at[2] + dz];
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Block(spot)), None);
    let walk = WalkTarget { target: Tracker::Block(spot), speed: p.speed, close: p.distance };
    set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_spot_is_within_one_block() {
        let offsets: Vec<i32> = (0..3).map(|draw| draw - 1).collect();
        assert_eq!(offsets, vec![-1, 0, 1]);
    }
}
