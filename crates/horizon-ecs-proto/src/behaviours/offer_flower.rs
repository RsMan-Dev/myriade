//! `IronGolem.OfferFlowerGoal`: by day (`Daylight`), one chance in 8000 per ask (every other tick, a geometric deadline of the wheel), the golem
//! picks the nearest villager or copper golem within 6 blocks in sight (box inflated 6 / 2 / 6) and holds a poppy out to it for 400 ticks (200 asks),
//! looking at it. Move + look flags. Not done: the entity events 11 and 34 (the arm raised: no channel to the client yet), the poppy given to a
//! copper golem at the end (`MobEvent::Gift`); a villager is a candidate only once its species exists in the table (it is listed by name).
use super::*;
use crate::species::SpeciesSet;
use horizon_core::geometry::Vec3;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub chance: i32,
    pub ticks: i32,
    /// The species the flower is offered to (configuration: the species file names them).
    pub candidates: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { chance: 8000, ticks: 400, candidates: &[] };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Offer {
    p: Params,
    slot: u8,
    mask: SpeciesSet,
    who: Option<Entity>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Offering {
    left: i32,
}

pub static MODULE: Module = Module { name: "offer_flower", flags: MOVE | LOOK, equip, arm, fired, blocked: Some(blocked), can_use, start, stop, tick: Some(tick), ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let ids = crate::species::registry().species.iter().enumerate().filter(|(_, s)| params::<Params>(p).candidates.contains(&s.name)).map(|(i, _)| i);
    w.entity_mut(e).insert(Offer { p: *params::<Params>(p), slot: inst.slot, mask: SpeciesSet::of(ids), who: None });
}
fn next(w: &mut World, e: Entity) {
    let Some(o) = w.get::<Offer>(e).copied() else { return };
    if o.mask.is_empty() {
        return;
    }
    let wait = with_rng(w, e, |r| 2 * geometric(r, o.p.chance));
    crate::goals::arm(w, e, o.slot, wait);
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    next(w, e);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    next(w, e);
    true
}
/// A goal of the golem that keeps the move flag holds the ask off: the `Mob` makes no draw then, the draw is lost and made again.
fn blocked(w: &mut World, e: Entity, _: Inst) {
    next(w, e);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !w.get_resource::<crate::hostile::Daylight>().is_some_and(|d| d.0) {
        return false;
    }
    let (Some(o), Some(pos)) = (w.get::<Offer>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { return false };
    let eye = w.get::<crate::targeting::Eye>(e).map_or(1.0, |l| l.0);
    let from = Vec3::new(pos.x, pos.y + eye, pos.z);
    let mw = w.resource::<Level>().mw.0;
    let mut best: Option<(f64, Entity)> = None;
    w.resource::<Index>().for_radius(pos.x, pos.z, 6.0, |c| {
        let (Some(cp), Some(cs)) = (w.get::<Pos>(c), w.get::<SpeciesId>(c)) else { return };
        if c == e || !o.mask.contains(cs.0) || (cp.0.y - pos.y).abs() > 2.0 {
            return;
        }
        let d = d2(pos, cp.0);
        if d <= 36.0 && best.is_none_or(|b| d < b.0 || (d == b.0 && c < b.1)) && mw.line_of_sight_for(Some(pos.y), from, Vec3::new(cp.0.x, cp.0.y + w.get::<crate::targeting::Eye>(c).map_or(1.0, |l| l.0), cp.0.z)) {
            best = Some((d, c));
        }
    });
    w.get_mut::<Offer>(e).unwrap().who = best.map(|b| b.1);
    best.is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    // The `Mob`'s goal ticks every other game tick: `reducedTickDelay(400)` of them last 400 game ticks.
    let n = w.get::<Offer>(e).unwrap().p.ticks;
    w.entity_mut(e).insert(Offering { left: n });
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Offering>();
    if let Some(mut o) = w.get_mut::<Offer>(e) {
        o.who = None;
    }
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, run);
}
fn run(mut q: Query<(Entity, &Offer, &mut Offering, &mut Look, Has<Looking>)>, pos: Query<&Pos>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for (e, o, mut s, mut look, looking) in &mut q {
        if let Some(p) = o.who.and_then(|c| pos.get(c).ok()) {
            aim(&mut cmd, e, &mut look, looking, [p.0.x, p.0.y + 1.0, p.0.z], Some(30.0));
        }
        s.left -= 1;
        if s.left <= 0 {
            reqs.0.push(Req::End(e, o.slot));
        }
    }
}
