//! `ElderGuardian.customServerAiStep`: when `(tickCount + id) % 1200 == 0`, mining fatigue III for 6000 ticks to the survival players
//! closer than 50 blocks (`MobEffectUtil.addEffectToPlayersAround`, last argument 1200), and the GUARDIAN_ELDER_EFFECT game event
//! (its parameter 1 unless silent: the client plays the curse sound for 1 only). At creation, when it has no restriction, a home
//! of radius 16 at its block (`setHomeTo`). Allied players are not checked: no team model yet.
//! DOUTE : le son est émis par `HostOut::sound` à tous, alors que le paquet vanille ne va qu'aux joueurs touchés.
//! DOUTE : `id` est l'index de l'entité ECS, pas l'identifiant du `Mob` : la phase diffère, la période et la portée non.
use super::*;
use crate::cow::*;
use crate::effects::{self, Kind, PlayerEffects};
use crate::goals::*;
use crate::behaviours::move_towards_restriction::Restriction;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub period: i64,
    pub range: f64,
    pub amp: i32,
    pub ticks: i32,
    /// A player already under the effect at its level is hit again when this many ticks or fewer are left.
    pub refresh: u64,
}
impl Params {
    pub const ELDER: Self = Self { period: 1200, range: 50.0, amp: 2, ticks: 6000, refresh: 1199 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Curse {
    p: Params,
}
/// Marker of a creature born silent (`isSilent`): no curse sound.
#[derive(Component, Clone, Copy, Debug)]
pub struct Silent;

pub static MODULE: Module = Module {
    name: "elder_curse",
    flags: 0,
    equip,
    arm,
    start,
    stop,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Curse { p: *params::<Params>(p) });
    if w.get::<Restriction>(e).is_none() {
        let at = w.get::<Pos>(e).unwrap().0;
        let center = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
        w.entity_mut(e).insert(Restriction { center, radius: 16, wanted: None });
    }
}
/// The next tick at which `(now + id) % period == 0` (at least one tick away).
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let p = w.get::<Curse>(e).unwrap().p;
    let now = w.resource::<Clock>().of(e) as i64;
    let phase = (now + i64::from(e.index().index())).rem_euclid(p.period);
    let wait = (p.period - phase) % p.period;
    crate::goals::arm(w, e, inst.slot, wait.max(1) as i32);
}
/// The curse: the players within range that the effect reaches (`MobEffectUtil`'s test), then the sound.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let c = *w.get::<Curse>(e).unwrap();
    let at = w.get::<Pos>(e).unwrap().0;
    let now = w.resource::<Clock>().0;
    let mut hit = Vec::new();
    let ps = w.get_resource::<crate::hostile::PlayerState>();
    let alive = |id: u32| ps.is_none_or(|p| !p.creative(id) && !p.spectator(id) && p.health.get(id as usize).is_none_or(|h| *h > 0.0));
    let r2 = c.p.range * c.p.range;
    w.resource::<Online>().for_near(at.x, at.z, c.p.range, |q| {
        let d2 = (q.x - at.x).powi(2) + (q.y - at.y).powi(2) + (q.z - at.z).powi(2);
        if alive(q.id) && d2 < r2 && takes(w, q.id, now, &c.p) {
            hit.push(q.id);
        }
    });
    for id in hit {
        effects::add_player(w, id, Kind::MiningFatigue, c.p.amp, c.p.ticks);
    }
    if w.get::<Silent>(e).is_none() {
        w.resource_mut::<Events>().host.sound("minecraft:entity.elder_guardian.curse", [at.x, at.y, at.z], 1.0, 1.0);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    arm(w, e, inst);
}
/// `hasEffect` and the level test: not under the effect, or under a weaker one, or its time nearly out.
fn takes(w: &World, id: u32, now: u64, p: &Params) -> bool {
    let left = w.get_resource::<PlayerEffects>().and_then(|r| r.0.get(&id)).and_then(|l| l.iter().find(|x| x.0 == Kind::MiningFatigue && x.2 > now)).copied();
    match left {
        None => true,
        Some((_, amp, ends)) => amp < p.amp || ends - now <= p.refresh,
    }
}
