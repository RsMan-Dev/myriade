//! `AbstractHorse.followMommy` (the last call of `aiStep`, each tick, no goal): a bred baby that is not eating keeps a path to the
//! nearest bred adult of the equine tag within the range, while it is more than 2 blocks away. The bred mark is the saved `Bred` key,
//! the eating one the saved `EatingHaystack` key (`nbt_fields::common`).
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub range: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { range: 16.0 };
}
/// `AbstractHorse.bred`: the mount was bred (the saved `Bred` byte).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Bred;
/// `AbstractHorse.isEating` (the `flag` of the old Mob): a baby eating its haystack keeps its place (the saved `EatingHaystack` byte).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct EatingHaystack;
/// The range of the entry, read at creation.
#[derive(Component, Clone, Copy, Debug)]
pub struct Mommy {
    pub range: f64,
}

pub static MODULE: Module = Module {
    name: "follow_mommy",
    flags: 0,
    equip,
    tick: Some(tick),
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Mommy { range: params::<Params>(p).range });
}
fn equine(w: &World, e: Entity) -> bool {
    w.get::<SpeciesId>(e).is_some_and(|id| crate::cow::tags_of_id(*id) & crate::cow::tag::EQUINE != 0)
}
/// `followMommy`: the nearest bred adult of the equine tag within the range (`distanceToSqr <= range²`), and a path to it while the baby
/// is more than 2 blocks away (`distanceToSqr > 4`), at the speed the navigation has.
pub fn tick(w: &mut World) {
    let babies: Vec<(Entity, _, f64)> = {
        let mut q = w.query_filtered::<(Entity, &Pos, &Mommy), (With<Bred>, With<GrowAt>, Without<EatingHaystack>)>();
        q.iter(w).map(|(e, p, m)| (e, p.0, m.range)).collect()
    };
    for (e, at, range) in babies {
        let mut near = Vec::new();
        w.resource::<Index>().for_radius(at.x, at.z, range, |c| near.push(c));
        let mut best = None;
        for c in near {
            if c == e || w.get::<GrowAt>(c).is_some() || w.get::<Bred>(c).is_none() || !equine(w, c) {
                continue;
            }
            let Some(p) = w.get::<Pos>(c).map(|p| p.0) else { continue };
            let d = d2(at, p);
            if d <= range * range && best.is_none_or(|b| d < b.1) {
                best = Some((p, d));
            }
        }
        let Some((mommy, _)) = best.filter(|b| b.1 > 4.0) else { continue };
        let speed = w.get::<Nav>(e).map_or(1.0, |n| n.speed);
        go_to_at(w, e, [mommy.x, mommy.y, mommy.z], speed, 0);
    }
}
