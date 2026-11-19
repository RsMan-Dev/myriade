//! `ChargeAttack` of the nautilus (`nautilus.rs:471-611`, fiche 113): with a target, out of rest and not in love, the animal
//! swims in a STRAIGHT line toward the position the target had at the start (`velocity` fixed, `speed` long); the charge ends
//! when it is 12 blocks from its start, when the target is 11 blocks away or out of sight, or at contact: then the blow, the
//! horizontal speed times `hit_slow`, the target dropped, and 80 ticks of rest. Woken by the target being acquired.
//! REPLIS (fiche 113 §5): no blow with a given push (`causeExtraKnockback` absent: the standard knockback of `add_by_entity`);
//! no head turn toward the target (`turn_to`, `aim_clamped` not called: the module keeps LOOK only as the flag of the Mob);
//! the target-choosing rule of the nautilus is not here (the test sets the target); the taming and the love are not read.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::hostile::PlayerState;
use crate::targeting::{self, Target, TargetWatchers, Who};
use horizon_core::geometry::Vec3;

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
    p: Params,
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
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
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
    let (dx, dy, dz) = (i.pos.x - p.x, i.pos.y - p.y, i.pos.z - p.z);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    let v = if len < 1.0e-4 { Vec3::new(0.0, 0.0, 0.0) } else { Vec3::new(dx / len * c.p.speed, dy / len * c.p.speed, dz / len * c.p.speed) };
    c.start = p;
    c.velocity = v;
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
/// The charge of every charging animal: velocity kept, then the end tests (target gone or unseen, 12 blocks, 11 blocks)
/// and the contact (the blow, the horizontal speed cut, the target dropped, the goal ends).
fn charge_run(
    mut set: ParamSet<(targeting::Targets, ResMut<PlayerState>)>,
    clock: Res<Clock>,
    mut dmg: ResMut<DamageQueue>,
    mut events: ResMut<Events>,
    mut reqs: ResMut<Requests>,
    mut q: Query<(Entity, &Pos, &Bounds, &Charger, &mut Vel, &AttackDamage, Option<&Target>), With<Charging>>,
    mut cmd: Commands,
) {
    let tick = clock.0;
    for (e, pos, bounds, c, mut vel, attack, target) in &mut q {
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
        if d2(c.start, p) >= c.p.max_run_sqr || d2(i.pos, p) >= c.p.max_target_sqr || !t.seen {
            end(&mut reqs);
            continue;
        }
        vel.0 = c.velocity;
        cmd.entity(e).insert(Moving);
        if !bounds.0.intersects(i.bounds) {
            continue;
        }
        let amount = attack.0;
        match t.who {
            Who::Entity(victim) => dmg.add_by_entity(victim, amount, (p.x, p.z), e),
            Who::Player(id) => set.p1().hurt(tick, id, amount, (p.x, p.z), Some(Who::Entity(e))),
        }
        vel.0.x *= c.p.hit_slow;
        vel.0.z *= c.p.hit_slow;
        cmd.entity(e).remove::<Target>();
        events.target_lost.push(e);
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
        let mut h = Herd::with_slab(60, true, false);
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
