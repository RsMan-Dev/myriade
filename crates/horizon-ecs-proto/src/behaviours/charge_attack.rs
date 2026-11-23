//! `ChargeAttack` of the nautilus (`nautilus.rs:471-611`, fiche 113): with a target, out of rest and not in love, the animal
//! swims in a STRAIGHT line toward the position the target had at the start (`velocity` fixed, `speed` long); the charge ends
//! when it is 12 blocks from its start, when the target is 11 blocks away or out of sight, or at contact: then the blow, the
//! horizontal speed times `hit_slow`, the target dropped, and 80 ticks of rest. Woken by the target being acquired.
//! The blow pushes its target twice as vanilla does: the default knockback, then `causeExtraKnockback` along the animal's facing
//! (`Hit.extra` for an entity, `PlayerState.pushes` for a player, applied by the host through the player's knockback).
//! REPLIS (fiche 113 §5): no head turn toward the target (`turn_to`, `aim_clamped` not called: the module keeps LOOK only as the flag of the Mob);
//! the target is chosen by `nearest_target` (the nautilus's parameters, `species/nautilus.rs`); the taming and the love are not read here.
use super::*;
use crate::cow::*;
use crate::effects::{Effects, Kind};
use crate::goals::*;
use crate::hostile::PlayerState;
use crate::rules::charge_attack::{charge_ends, charge_velocity};
use crate::targeting::{self, Target, TargetWatchers, Who};
use horizon_core::geometry::{Vec3, trig};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed of the charge (`ChargeAttack(.., speed, ..)`: 0,6 for the nautilus, 0,5 for the zombie one).
    pub speed: f64,
    /// The ticks of rest after a charge (`counter2 = 80`, `stop`).
    pub pause: i32,
    /// The squared distance from the start (12 blocks) and from the target (11 blocks) that end the charge.
    pub max_run_sqr: f64,
    pub max_target_sqr: f64,
    /// The horizontal speed kept after the blow (`velocity.x, z *= 0.6`).
    pub hit_slow: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.6, pause: 80, max_run_sqr: 144.0, max_target_sqr: 121.0, hit_slow: 0.6 };
}
/// The state of the goal: its parameters, its slot, the start of the charge, the fixed velocity, and the end of the rest.
#[derive(Component, Clone, Copy)]
pub struct Charger {
    p: &'static Params,
    slot: u8,
    start: Vec3,
    velocity: Vec3,
    rest_until: u64,
}
/// The goal runs: the tick visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Charging;

pub static MODULE: Module = Module {
    name: "charge_attack",
    flags: MOVE | LOOK,
    resources,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Living, Cap::Goals, Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Charger { p, slot: inst.slot, start: Vec3::new(0.0, 0.0, 0.0), velocity: Vec3::new(0.0, 0.0, 0.0), rest_until: 0 });
}
/// A target acquired by an animal that has the goal: the goal is asked (the selector waits for its phase).
fn on_target(w: &mut World, e: Entity) {
    let Some(slot) = w.get::<Charger>(e).map(|c| c.slot) else { return };
    w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
}
/// The only deadline is the end of the rest: `can_use` decides whether there is still a target.
fn fired(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
/// `canUse`: a target, and the rest over (`counter2 <= 0`); during the rest the deadline is re-armed at its end.
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(rest_until) = w.get::<Charger>(e).map(|c| c.rest_until) else { return false };
    if w.get::<Target>(e).is_none() {
        return false;
    }
    let now = now_of(w, e);
    if now < rest_until {
        crate::goals::arm(w, e, inst.slot, (rest_until - now) as i32);
        return false;
    }
    true
}
/// `start`: the start is here, the velocity points at the target's position now, its length is the speed (zero when the
/// target is on the animal). The tick that runs in the same tick applies it.
fn start(w: &mut World, e: Entity, _: Inst) {
    let (Some(p), Some(who)) = (w.get::<Pos>(e).map(|x| x.0), w.get::<Target>(e).map(|t| t.who)) else { return };
    let Some(i) = targeting::info(w, who) else { return };
    let Some(mut c) = w.get_mut::<Charger>(e) else { return };
    c.start = p;
    c.velocity = charge_velocity(p, i.pos, c.p.speed);
    drop(c);
    w.entity_mut(e).insert(Charging);
}
/// `stop`: the rest of 80 ticks is armed (`counter2 = 80`), the charge is over.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Charging>();
    let now = now_of(w, e);
    let Some(pause) = w.get::<Charger>(e).map(|c| c.p.pause) else { return };
    if let Some(mut c) = w.get_mut::<Charger>(e) {
        c.rest_until = now + pause as u64;
    }
    crate::goals::arm(w, e, inst.slot, pause);
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, charge_run);
}
/// The level of an effect on the animal (`amplifier + 1`, 0 without it).
fn level(fx: Option<&Effects>, kind: Kind) -> i32 {
    fx.and_then(|f| f.list.iter().find(|x| x.kind == kind)).map_or(0, |x| x.amp + 1)
}
/// The charge of every charging animal: velocity kept, then the end tests (target gone or unseen, 12 blocks, 11 blocks)
/// and the contact (the blow, the horizontal speed cut, the target dropped, the goal ends).
fn charge_run(
    mut set: ParamSet<(targeting::Targets, ResMut<PlayerState>)>,
    clock: Res<Clock>,
    mut dmg: ResMut<DamageQueue>,
    mut reqs: ResMut<Requests>,
    mut q: Query<(Entity, &Pos, &Bounds, &Charger, &mut Vel, &AttackDamage, Option<&Target>, &Body, &Yaw, Option<&Effects>), With<Charging>>,
    mut cmd: Commands,
) {
    let tick = clock.0;
    for (e, pos, bounds, c, mut vel, attack, target, body, yaw, fx) in &mut q {
        let end = |reqs: &mut Requests| reqs.0.push(Req::End(e, c.slot));
        let Some(t) = target.copied() else {
            end(&mut reqs);
            continue;
        };
        let Some(i) = set.p0().info(t.who).filter(|i| !i.exempt) else {
            end(&mut reqs);
            continue;
        };
        let p = pos.0;
        if charge_ends(c.start, p, i.pos, t.seen, c.p.max_run_sqr, c.p.max_target_sqr) {
            end(&mut reqs);
            continue;
        }
        vel.0 = c.velocity;
        cmd.entity(e).insert(Moving);
        if !bounds.0.intersects(i.bounds) {
            continue;
        }
        let amount = attack.0;
        // `causeExtraKnockback` (nautilus.rs:760): along (sin yaw, -cos yaw), power `(clamp(speed * movement speed, 0.2, 2) + boost) * 2`,
        // the boost being 0.25 per level of speed less 0.25 per level of slowness (`Body.speed` already carries the effects' speed).
        let angle = f64::from(yaw.0 * trig::DEG_TO_RAD);
        // The factor is a float in the game; a slowness deep enough leaves no power, and then no second knockback.
        let boost = 0.25 * (level(fx, Kind::Speed) - level(fx, Kind::Slowness)) as f32;
        let power = f64::from((((c.p.speed * body.speed) as f32).clamp(0.2, 2.0) + boost) * 2.0);
        let extra = (power > 0.0).then(|| (f64::from(trig::sin(angle)), -f64::from(trig::cos(angle)), power));
        match t.who {
            Who::Entity(victim) => {
                dmg.add_by_entity(victim, amount, (p.x, p.z), e);
                if let Some(hit) = dmg.0.last_mut() {
                    hit.extra = extra;
                }
            }
            Who::Player(id) => {
                set.p1().hurt(tick, id, amount, (p.x, p.z), Some(Who::Entity(e)));
                if let Some(extra) = extra {
                    set.p1().push(id, extra, e);
                }
            }
        }
        vel.0.x *= c.p.hit_slow;
        vel.0.z *= c.p.hit_slow;
        cmd.queue(move |w: &mut World| targeting::clear_target(w, e));
        end(&mut reqs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::{Who, clear_target, set_target};
    use horizon_core::geometry::Vec3 as V;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 3, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A nautilus with this goal alone, and a cow (no goals) `dist` blocks away on x, its target acquired.
    fn world(dist: f64) -> Option<(Herd, Entity, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:nautilus").unwrap();
        sp.goals = config();
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let nautilus = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
        let prey = spawn_species(&mut h.world, &cow, 0.5 + dist, 0.5, 62);
        for e in [nautilus, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        for _ in 0..5 {
            h.tick();
        }
        let y = h.world.get::<Pos>(nautilus).unwrap().0.y;
        h.teleport(prey, V::new(0.5 + dist, y, 0.5));
        set_target(&mut h.world, nautilus, Target::new(Who::Entity(prey), 200, u8::MAX));
        Some((h, nautilus, prey))
    }

    /// A charge starts with the target seen, and flies straight at it: the fixed velocity points at the target, its length
    /// is the speed (witness: the goal runs; a nautilus with no target never runs).
    #[test]
    fn it_charges_straight_at_a_seen_target() {
        let Some((mut h, nautilus, _)) = world(6.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let mut started = false;
        for _ in 0..40 {
            h.tick();
            started |= running(&h, nautilus);
        }
        assert!(started, "témoin : la charge part dans 40 ticks");
        let c = *h.world.get::<Charger>(nautilus).unwrap();
        assert!((c.velocity.x - 0.6).abs() < 1e-9, "vitesse vers la cible : {:?}", c.velocity);
        assert!(c.velocity.z.abs() < 1e-9 && c.velocity.y.abs() < 1e-9, "trajectoire droite : {:?}", c.velocity);
    }

    /// Losing the target ends the charge at once; the rest lasts 80 ticks, then a target held starts a charge again.
    #[test]
    fn it_rests_80_ticks_after_a_charge() {
        let Some((mut h, nautilus, prey)) = world(6.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        for _ in 0..40 {
            h.tick();
            if running(&h, nautilus) {
                break;
            }
        }
        assert!(running(&h, nautilus), "témoin : la charge part");
        clear_target(&mut h.world, nautilus);
        h.tick();
        assert!(!running(&h, nautilus), "la perte de la cible finit la charge");
        set_target(&mut h.world, nautilus, Target::new(Who::Entity(prey), 200, u8::MAX));
        let mut again = None;
        for t in 1..=200 {
            h.tick();
            if running(&h, nautilus) {
                again = Some(t);
                break;
            }
        }
        let t = again.expect("témoin : la charge repart après le repos");
        assert!((70..=90).contains(&t), "le repos dure 80 ticks, repart après {t}");
    }

    /// Without a target nothing starts; the same animal starts as soon as a target is held (the witness).
    #[test]
    fn without_a_target_it_does_not_charge() {
        let Some((mut h, nautilus, prey)) = world(6.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        clear_target(&mut h.world, nautilus);
        for _ in 0..60 {
            h.tick();
            assert!(!running(&h, nautilus), "sans cible, pas de charge");
        }
        set_target(&mut h.world, nautilus, Target::new(Who::Entity(prey), 200, u8::MAX));
        let mut started = false;
        for _ in 0..40 {
            h.tick();
            started |= running(&h, nautilus);
        }
        assert!(started, "témoin : la même bête charge dès qu'elle tient une cible");
    }
}
