//! The area cloud of a potion (`AreaEffectCloud`, `items/potions.rs`): asleep for its wait (`after`), then `Live`. Its radius changes by
//! `radius_per_tick` each tick, it is removed under 0.5 blocks or past its duration, and every 5 ticks it gives its potion to the players and
//! creatures in its circle, once per reapplication delay (`Reached`); each new victim grows it by `radius_on_use` and lengthens it by
//! `duration_on_use`. Instant brews act here; a player's timed effects leave as `MiscEvent::CloudEffect`, a creature's are added here.
use crate::area;
use crate::colony_register::MiscOut;
use crate::cow::{after, now_of, Cause, DamageQueue, Health, OwnTime, Pos, SpeciesId, SpeciesTable};
use crate::effect_cloud::CloudSpec;
use crate::effects::{self, Brew, Kind};
use crate::goals::Events;
use crate::hostile::PlayerState;
use crate::host_out_misc::MiscEvent;
use crate::rules::effect_cloud as rule;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// `AreaEffectCloud` defaults from the server jar: a wait of 20 ticks, a reapplication delay of 20, no duration (`INFINITE`), no growth.
pub const WAIT: i32 = 20;
pub const REAPPLICATION: i32 = 20;
pub const INFINITE: i32 = -1;
pub const TIME_BETWEEN_APPLICATIONS: i32 = 5;
pub const HEIGHT: f64 = 0.5;

/// Marks a cloud past its wait: `tick` visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Live;

/// The players and creatures dosed, with the age each may be dosed again at.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Reached(pub Vec<(Who, i32)>);

/// A cloud at `at` (its floor), announced to the host as `HostEvent::Cloud`. Its wait is a deadline on the wheel; nothing visits it before.
pub fn spawn(w: &mut World, at: Vec3, c: CloudSpec) -> Entity {
    let (wait, radius, duration) = (c.wait, c.radius, c.duration);
    let e = w.spawn((Pos(at), c, Reached::default(), OwnTime)).id();
    crate::nbt::mark_saved(w, e, crate::species::lists::AREA_EFFECT_CLOUD);
    w.resource_mut::<Events>().host.cloud([at.x, at.y, at.z], radius, duration);
    after(w, e, wait, wake);
    e
}

/// The wait is over: the cloud is `Live`. Also armed by a load, for the age the save kept.
pub fn wake(w: &mut World, e: Entity) {
    if let Ok(mut m) = w.get_entity_mut(e) {
        m.insert(Live);
    }
}

/// Whether a cloud is live: the system runs only then.
pub fn any_live(q: Query<(), With<Live>>) -> bool {
    !q.is_empty()
}

/// One tick of every live cloud: it ends past its duration, grows, is removed under 0.5 blocks, and doses at every 5th tick of its age.
pub fn tick(w: &mut World) {
    let clouds: Vec<Entity> = w.query_filtered::<Entity, With<Live>>().iter(w).collect();
    for e in clouds {
        // Taken out and put back: the spec holds its particle compound, which a tick must not copy.
        let mut c = std::mem::take(&mut *w.get_mut::<CloudSpec>(e).expect("live clouds have a spec"));
        let age = now_of(w, e) as i32 + c.age_base;
        let mut alive = !rule::over(age, c.wait, c.duration);
        if alive && c.radius_per_tick != 0.0 {
            let (radius, live) = rule::grow(c.radius, c.radius_per_tick);
            c.radius = radius;
            alive = live;
        }
        if alive && age % TIME_BETWEEN_APPLICATIONS == 0 {
            alive = dose(w, e, &mut c, age);
        }
        if alive {
            w.entity_mut(e).insert(c);
        } else {
            w.despawn(e);
        }
    }
}

/// The dose: the reached whose delay is over are forgotten, then each player and creature in the circle gets the potion once. A new
/// victim grows the cloud by `radius_on_use` (gone under 0.5 blocks: false) and lengthens it by `duration_on_use` when it is finite.
fn dose(w: &mut World, e: Entity, c: &mut CloudSpec, age: i32) -> bool {
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return true };
    let owned: Vec<Brew>;
    let brews: &[Brew] = if c.custom.is_empty() {
        effects::potion_brews(c.potion).unwrap_or(&[])
    } else {
        owned = effects::potion_brews(c.potion).unwrap_or(&[]).iter().chain(&c.custom).copied().collect();
        &owned
    };
    let r = f64::from(c.radius);
    let area = Aabb::new(Vec3::new(at.x - r, at.y, at.z - r), Vec3::new(at.x + r, at.y + HEIGHT, at.z + r));
    let mut inside: Vec<Who> = Vec::new();
    if !brews.is_empty() {
        inside.extend(area::players_in(w, area).into_iter().map(Who::Player));
        inside.extend(area::creatures_in(w, area, e).into_iter().map(Who::Entity));
    }
    let mut reached = std::mem::take(&mut w.get_mut::<Reached>(e).expect("spawned with it").0);
    reached.retain(|(_, until)| age < *until);
    let mut alive = true;
    for who in inside {
        let (x, z) = match who {
            Who::Player(id) => area::player_xz(w, id),
            Who::Entity(k) => area::creature_xz(w, k),
        };
        if reached.iter().any(|(k, _)| *k == who) || rule::outside(x - at.x, z - at.z, c.radius) || !affect(w, e, at, who, brews, c) {
            continue;
        }
        reached.push((who, age.wrapping_add(c.reapplication)));
        if c.radius_on_use != 0.0 {
            let (radius, live) = rule::grow(c.radius, c.radius_on_use);
            c.radius = radius;
            if !live {
                alive = false;
                break;
            }
        }
        if c.duration_on_use != 0 && c.duration != INFINITE {
            let (duration, live) = rule::extend(c.duration, c.duration_on_use);
            c.duration = duration;
            if !live {
                alive = false;
                break;
            }
        }
    }
    w.get_mut::<Reached>(e).expect("spawned with it").0 = reached;
    alive
}

/// Gives `brews` to `who`, as the jar's `AreaEffectCloud` does: instant brews act here, a player's timed effects leave as `CloudEffect` for the
/// host, a creature's are added here. False when `who` cannot be affected (a creature without health).
fn affect(w: &mut World, cloud: Entity, at: Vec3, who: Who, brews: &[Brew], c: &CloudSpec) -> bool {
    if let Who::Entity(k) = who {
        if w.get::<Health>(k).is_none() {
            return false;
        }
    }
    for &brew in brews {
        // A custom effect's flags are the cloud's own lists for now: its brew is timed like a named one.
        let (harm, level) = match brew {
            Brew::Timed(kind, amp, ticks) | Brew::Flagged(kind, amp, ticks, _) => {
                let (ticks, flags) = (rule::scaled(ticks, c.duration_scale), flags(c, kind));
                match who {
                    Who::Player(_) => {
                        let out = &mut w.get_resource_or_insert_with(MiscOut::default).0;
                        out.push(MiscEvent::CloudEffect {
                            target: who,
                            effect: kind,
                            amplifier: amp,
                            ticks,
                            hidden: flags & effects::HIDDEN != 0,
                            ambient: flags & effects::AMBIENT != 0,
                            source: c.owner,
                            owner: c.owner,
                        });
                    }
                    Who::Entity(k) => {
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
        match who {
            Who::Player(id) if harm => area::hurt_player(w, id, amount, source_entity(c, cloud)),
            Who::Player(id) => heal_player(w, id, amount),
            Who::Entity(k) if harm != crate::type_tags::is::<effects::InvertedHealingAndHarm>(w, k) => match c.owner {
                Some(Who::Entity(o)) => w.resource_mut::<DamageQueue>().add_by_entity(k, amount, (at.x, at.z), o),
                _ => w.resource_mut::<DamageQueue>().add(k, amount, Cause::Magic),
            },
            Who::Entity(k) => heal(w, k, amount),
        }
    }
    true
}

/// The entity a cloud's damage is credited to: its owner when it is an entity, else the cloud itself.
fn source_entity(c: &CloudSpec, cloud: Entity) -> Entity {
    match c.owner {
        Some(Who::Entity(o)) => o,
        _ => cloud,
    }
}

/// The `HIDDEN` and `AMBIENT` flags of `kind` in the cloud.
fn flags(c: &CloudSpec, kind: Kind) -> u8 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(radius: f32, per_tick: f32) -> CloudSpec {
        CloudSpec { radius_per_tick: per_tick, ..CloudSpec::new(radius, "minecraft:poison", None) }
    }

    /// A cloud shrinking under half a block is removed on the tick it gets there.
    #[test]
    fn a_cloud_under_half_a_block_is_removed() {
        let mut herd = crate::cow::Herd::new(0, 4.0, false, false, 7);
        let e = spawn(&mut herd.world, Vec3::new(0.5, 64.0, 0.5), cloud(0.6, -0.2));
        herd.world.entity_mut(e).insert(Live);
        tick(&mut herd.world);
        assert!(herd.world.get_entity(e).is_err());
    }

    /// A cloud spawned with a wait is not `Live` at once: nothing visits it before its wake-up.
    #[test]
    fn a_cloud_is_asleep_until_its_wait_is_over() {
        let mut herd = crate::cow::Herd::new(0, 4.0, false, false, 7);
        let e = spawn(&mut herd.world, Vec3::new(0.5, 64.0, 0.5), CloudSpec { wait: WAIT, ..cloud(3.0, -0.1) });
        assert!(herd.world.get::<Live>(e).is_none());
    }
}
