//! The `AreaEffectCloud` of a lingering potion (`items/potions.rs`, `tick_potion_cloud`): its radius grows by `radius_per_tick` once its
//! wait is over, and it ends when its duration or its radius is over. Every five ticks past the wait, the players and creatures in its
//! circle get its potion's brews, once per reapplication delay (`Victims`). `breath_cloud` is the dragon's cloud: its wait is fixed.
use crate::area;
use crate::cow::{Cause, DamageQueue, Health, Pos, SpeciesId, SpeciesTable};
use crate::effects::{self, Brew, Kind, PotionId};
use crate::goals::Events;
use crate::hostile::PlayerState;
use crate::rules::effect_cloud as rule;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// The radius, its growth a tick, what a victim adds to the radius and to the duration, the age, the wait, the duration (-1: none), the
/// delay before a victim is affected again, the scale of the timed brews' durations, the flags of the effects it shows hidden or ambient,
/// and the owner the damage is credited to (`None`: the cloud). Sparse: clouds are few.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct EffectCloud {
    pub radius: f32,
    pub radius_per_tick: f32,
    pub radius_on_use: f32,
    pub duration_on_use: i32,
    pub age: i32,
    pub wait: i32,
    pub duration: i32,
    pub reapplication_delay: i32,
    pub duration_scale: f32,
    pub hidden: &'static [Kind],
    pub ambient: &'static [Kind],
    pub owner: Option<Who>,
}

/// A player or a creature a cloud affected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Victim {
    Player(u32),
    Creature(Entity),
}

/// The victims with the tick each may be affected again at. Sparse, like the cloud.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Victims(pub Vec<(Victim, i32)>);

/// A cloud of `potion` at `at` (its floor). It emits one `HostEvent::Cloud`, as the breath clouds do.
pub fn spawn(w: &mut World, at: [f64; 3], potion: PotionId, c: EffectCloud) -> Entity {
    let e = w.spawn((Pos(Vec3::new(at[0], at[1], at[2])), c, potion, Victims::default()))
.id();
    crate::nbt::mark_saved(w, e, crate::species::lists::AREA_EFFECT_CLOUD);
    w.resource_mut::<Events>().host.cloud(at, c.radius, c.duration);
    e
}

/// One tick: ages the cloud, grows it once its wait is over (kept in 0..=32). False once it is gone.
pub fn step(c: &mut EffectCloud) -> bool {
    c.age = c.age.wrapping_add(1);
    if rule::over(c.age, c.wait, c.duration) {
        return false;
    }
    if c.age < c.wait {
        return true;
    }
    if c.radius_per_tick != 0.0 {
        let (radius, alive) = rule::grow(c.radius, c.radius_per_tick);
        c.radius = radius;
        return alive;
    }
    true
}

/// One tick of every cloud: only the clouds are visited, so none costs nothing. A cloud that is gone is despawned.
pub fn tick(w: &mut World) {
    let clouds: Vec<Entity> = w.query_filtered::<Entity, With<EffectCloud>>().iter(w).collect();
    for e in clouds {
        let mut c = *w.get::<EffectCloud>(e).expect("listed");
        let alive = step(&mut c) && (c.age < c.wait || c.age % 5 != 0 || dose(w, e, &mut c));
        w.entity_mut(e).insert(c);
        if !alive {
            w.despawn(e);
        }
    }
}

/// The five-tick dose of a cloud past its wait: the victims whose delay is over are dropped, then the players and creatures in its
/// circle are affected. False when `radius_on_use` or `duration_on_use` ends the cloud.
fn dose(w: &mut World, e: Entity, c: &mut EffectCloud) -> bool {
    let brews: &'static [Brew] = w.get::<PotionId>(e).and_then(|p| effects::potion_brews(p.0)).unwrap_or(&[]);
    let mut victims = std::mem::take(&mut w.get_mut::<Victims>(e).expect("spawned with it").0);
    victims.retain(|(_, until)| c.age < *until);
    let mut alive = true;
    if brews.is_empty() {
        victims.clear();
    } else {
        let at = w.get::<Pos>(e).expect("spawned with it").0;
        let r = f64::from(c.radius);
        let area = Aabb::new(Vec3::new(at.x - r, at.y, at.z - r), Vec3::new(at.x + r, at.y + 0.5, at.z + r));
        let mut inside: Vec<Victim> = area::players_in(w, area).into_iter().map(Victim::Player).collect();
        inside.extend(area::creatures_in(w, area, e).into_iter().map(Victim::Creature));
        for v in inside {
            let (x, z) = match v {
                Victim::Player(id) => area::player_xz(w, id),
                Victim::Creature(k) => area::creature_xz(w, k),
            };
            let (dx, dz) = (x - at.x, z - at.z);
            if victims.iter().any(|(k, _)| *k == v) || rule::outside(dx, dz, c.radius) || !affect(w, e, at, v, brews, c) {
                continue;
            }
            victims.push((v, c.age.wrapping_add(c.reapplication_delay)));
            if c.radius_on_use != 0.0 {
                let (radius, live) = rule::grow(c.radius, c.radius_on_use);
                c.radius = radius;
                if !live {
                    alive = false;
                    break;
                }
            }
            if c.duration_on_use != 0 && c.duration != -1 {
                let (duration, live) = rule::extend(c.duration, c.duration_on_use);
                c.duration = duration;
                if !live {
                    alive = false;
                    break;
                }
            }
        }
    }
    w.get_mut::<Victims>(e).expect("spawned with it").0 = victims;
    alive
}

/// Gives `brews` to `v`, as `cloud_affect` does: `cloud` and `at` are the source. False when `v` cannot be affected (a creature without
/// health).
fn affect(w: &mut World, cloud: Entity, at: Vec3, v: Victim, brews: &[Brew], c: &EffectCloud) -> bool {
    if let Victim::Creature(k) = v {
        if w.get::<Health>(k).is_none() {
            return false;
        }
    }
    for &brew in brews {
        let (harm, level) = match brew {
            Brew::Timed(kind, amp, ticks) => {
                let (ticks, flags) = (rule::scaled(ticks, c.duration_scale), flags(c, kind));
                match v {
                    Victim::Player(id) => effects::add_player_flagged(w, id, kind, amp, ticks, flags),
                    Victim::Creature(k) => {
                        effects::add_flagged(w, k, kind, amp, ticks, flags);
                    }
                }
                continue;
            }
            Brew::Harm(level) => (true, level),
            Brew::Heal(level) => (false, level),
        };
        // `HealOrHarmMobEffect.applyInstantaneousEffect`: `6 << level` harms, `4 << level` heals, at half strength.
        let amount = rule::instant(if harm { 6 } else { 4 }, level);
        match v {
            Victim::Player(id) if harm => area::hurt_player(w, id, amount, source_entity(c, cloud)),
            Victim::Player(id) => heal_player(w, id, amount),
            Victim::Creature(k) if harm != crate::type_tags::is::<effects::InvertedHealingAndHarm>(w, k) => match c.owner {
                Some(Who::Entity(o)) => w.resource_mut::<DamageQueue>().add_by_entity(k, amount, (at.x, at.z), o),
                _ => w.resource_mut::<DamageQueue>().add(k, amount, Cause::Magic),
            },
            Victim::Creature(k) => heal(w, k, amount),
        }
    }
    true
}

/// The entity a cloud's damage is credited to: its owner when it is an entity, else the cloud itself.
fn source_entity(c: &EffectCloud, cloud: Entity) -> Entity {
    match c.owner {
        Some(Who::Entity(o)) => o,
        _ => cloud,
    }
}

/// The `HIDDEN` and `AMBIENT` flags of `kind` in the cloud.
fn flags(c: &EffectCloud, kind: Kind) -> u8 {
    (if c.hidden.contains(&kind) { effects::HIDDEN } else { 0 }) | (if c.ambient.contains(&kind) { effects::AMBIENT } else { 0 })
}

/// A player healed by `amount`: its health as the host reads it (`PlayerState`), at most 20.
fn heal_player(w: &mut World, id: u32, amount: f32) {
    let mut ps = w.resource_mut::<PlayerState>();
    let i = id as usize;
    if ps.health.len() <= i {
        ps.health.resize(i + 1, 20.0);
    }
    ps.health[i] = (ps.health[i] + amount).min(20.0);
}

/// A creature healed by `amount`, up to its species' maximum.
fn heal(w: &mut World, k: Entity, amount: f32) {
    let max = w.get::<SpeciesId>(k).map_or(f32::MAX, |s| w.resource::<SpeciesTable>().0[s.0 as usize].max_health);
    if let Some(mut h) = w.get_mut::<Health>(k) {
        h.0 = (h.0 + amount).min(max);
    }
}
