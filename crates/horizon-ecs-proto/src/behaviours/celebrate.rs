//! Celebration after a kill of the creature this one targets (`mobs/piglin.rs:1874-1905`): the kill is an event
//! (`Events.killed`, from `cow::apply_damage`); the targeters of the victim walk to the spot it fell and play the sound.
use super::*;
use crate::targeting::{Target, Who};
use crate::species::registry;
use crate::{cow::*, goals::*};
use horizon_core::random::{LegacyRandom, RandomSource};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub sound: &'static str,
    pub duration: u64,
    /// The victim species that makes the celebrant dance, and the chance of it (`PiglinAi.wantsToDance`: one time in ten).
    pub dance_of: &'static str,
    pub dance_chance: f32,
    /// The walk to the kill spot: within `close` blocks at `speed`, or within `dance_close` at `dance_speed` while dancing.
    pub close: i32,
    pub speed: f64,
    pub dance_close: i32,
    pub dance_speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self {
        sound: "minecraft:entity.piglin.celebrate",
        duration: 300,
        dance_of: "",
        dance_chance: 0.1,
        close: 2,
        speed: 1.0,
        dance_close: 4,
        dance_speed: 0.6,
    };
}
/// A creature killed by a damage hit: the victim, its killer when the hit has one, its species id (`u8::MAX`: none), and where it fell.
#[derive(Clone, Copy, Debug)]
pub struct Kill {
    pub victim: Entity,
    pub killer: Option<crate::targeting::Who>,
    pub species: u8,
    pub at: [f64; 3],
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Party {
    pub p: &'static Params,
    pub slot: u8,
}
/// The celebration runs until `until`, from the kill spot `spot` (the centre of its block), dancing or not.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Celebrating {
    pub until: u64,
    pub spot: [f64; 3],
    pub dance: bool,
}

pub static MODULE: Module = Module {
    name: "celebrate",
    flags: MOVE,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Party { p: params::<Params>(p), slot: inst.slot });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(on_kill.in_set(slot));
}
/// A kill wakes the celebrants whose target was the victim: their party starts at the spot it fell.
fn on_kill(mut commands: Commands, ev: Res<Events>, q: Query<(Entity, &Target), With<Party>>) {
    for k in ev.killed.iter() {
        for (e, t) in &q {
            if matches!(t.who, Who::Entity(v) if v == k.victim) {
                commands.queue(move |w: &mut World| start_party(w, e, k));
            }
        }
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Celebrating>(e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let (c, p) = (*w.get::<Celebrating>(e).unwrap(), w.get::<Party>(e).unwrap().p);
    crate::behaviours::act_phase::sound(w, e, p.sound, 1.0, 1.0);
    let (reach, speed) = if c.dance { (p.dance_close, p.dance_speed) } else { (p.close, p.speed) };
    go_to_at(w, e, c.spot, speed, reach);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Celebrating>();
}
/// The dance is drawn only for the victim species `dance_of`, from a random seeded with the time, as the Mob does.
pub fn start_party(w: &mut World, e: Entity, k: Kill) {
    let Some(p) = w.get::<Party>(e).copied() else { return };
    let now = now_of(w, e);
    let hunted = registry().id(p.p.dance_of).is_some_and(|i| i as u8 == k.species);
    let dance = hunted && LegacyRandom::new(now as i64).next_float() < p.p.dance_chance;
    let spot = [k.at[0].floor() + 0.5, k.at[1].floor(), k.at[2].floor() + 0.5];
    w.entity_mut(e).insert(Celebrating { until: now + p.p.duration, spot, dance });
    after(w, e, p.p.duration as i32, time_up);
    w.resource_mut::<Requests>().0.push(Req::Start(e, p.slot));
}
fn time_up(w: &mut World, e: Entity) {
    let (Some(c), Some(p)) = (w.get::<Celebrating>(e).copied(), w.get::<Party>(e).copied()) else { return };
    if now_of(w, e) >= c.until {
        w.resource_mut::<Requests>().0.push(Req::End(e, p.slot));
    }
}
