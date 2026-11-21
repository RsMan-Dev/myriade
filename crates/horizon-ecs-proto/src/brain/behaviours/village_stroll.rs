//! `VillageBoundStroll` (`villager_brain.rs:2006`): a land spot, leaning to the nearest village section.
use crate::brain::*;
use crate::cow::Pos;
use crate::poi::PoiRegistry;
use crate::spots::Pick;

pub struct Params {
    pub speed: f32,
    pub xz: i32,
    pub y: i32,
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:village_bound_random_stroll", mems: &[&WALK_TARGET], one_shot: true, can_start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let p = c.get::<Params>();
    let Some(at) = w.get::<Pos>(e).map(|q| [q.0.x.floor() as i32, q.0.y.floor() as i32, q.0.z.floor() as i32]) else { return false };
    let section = [at[0] >> 4, at[1] >> 4, at[2] >> 4];
    let (poi, best) = {
        let poi = w.resource::<PoiRegistry>();
        (poi.sections_to_village(section), poi.closest_section(section, 2))
    };
    let spot = if poi <= 1 || best == section {
        pick(w, e, Pick::Land { h: p.xz, v: p.y })
    } else {
        let to = [f64::from(best[0] * 16 + 8) + 0.5, f64::from(best[1] * 16 + 8), f64::from(best[2] * 16 + 8) + 0.5];
        pick(w, e, Pick::DefaultTowards { h: p.xz, v: p.y, to, max_rad: f64::from(std::f32::consts::FRAC_PI_2) })
    };
    match spot {
        Some(q) => set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(q), speed: p.speed, close: 0 }), None),
        None => erase(w, e, &WALK_TARGET),
    }
    true
}
