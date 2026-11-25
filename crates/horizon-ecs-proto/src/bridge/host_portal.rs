//! The portals through the bridge (lot 73): the death the host forces (`kill`), the entries of the portals that moved. The calls that may run vanilla's rules take the
//! `MobWorld` the same way `mount` does.
use crate::bridge::host::EcsWorld;
use crate::cow::{Cell, DeathPolicy, Dying, GrowAt, Index, Moving, Pos, Sizes, Yaw};
use crate::goals::{Events, Head};
use crate::portal;
use bevy_ecs::{entity::Entity, prelude::*};
use myriade_engine_lib::mobs::MobWorld;

impl EcsWorld {
    /// `Entity.kill`: a lethal hit of cause `generic_kill`, which a totem does not stop (`#bypasses_invulnerability`). Under
    /// `DeathPolicy.vetoable` the entity stays `Dying` until the host calls `loot_dying`, its hook and `despawn`; otherwise its loot
    /// leaves on the ground (`loot::on_death`) and it goes. Returns the item names it dropped (none while held `Dying`).
    pub fn kill(&mut self, mw: &MobWorld<'_>, e: Entity) -> Vec<&'static str> {
        self.lent(mw, |w| {
            if w.get_entity(e).is_err() || crate::tear_down::is_dead(w, e) {
                return Vec::new();
            }
            if let Some(mut h) = w.get_mut::<crate::cow::Health>(e) {
                h.0 = 0.0;
            }
            // `LivingEntity.hurtServer` of the lethal blow: `makeSound(getDeathSound)` before `die`.
            crate::sound_slots::death(w, e);
            if let Some(c) = w.get::<Cell>(e).map(|c| c.0)
                && c != u32::MAX
            {
                w.resource_mut::<Index>().remove(e, c);
            }
            let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
            w.resource_mut::<Events>().host.death(e, "generic_kill", None, at);
            if let Some(id) = crate::bridge::ids::id_of(w, e) {
                w.resource_mut::<Events>().host.death_animation(id, at);
            }
            if w.get_resource::<DeathPolicy>().is_some_and(|p| p.vetoable) {
                w.entity_mut(e).insert(Dying { health: 0.0 });
                return Vec::new();
            }
            let names = crate::loot::on_death(w, e);
            crate::item::spill_destroyed(w, e);
            crate::spawn_trial::forget(w, e);
            w.entity_mut(e).insert(crate::tear_down::DyingPose);
            crate::tear_down::linger(w, e);
            names
        })
    }

    /// The entities to look at for a portal, as (entity, may take a portal with its vehicle, where it is): the `Moving` ones, those that
    /// entered the world since the last step, and `extra` (the host's own: an entity held by a portal, one next to a portal just lit).
    /// None of them: nothing is lent, no query is built (read from the archetypes, no entity visited).
    pub fn portal_entries(&mut self, mw: &MobWorld<'_>, extra: &[Entity]) -> Vec<(Entity, bool, [f64; 3])> {
        if !self.any_moving() && !self.any_entered() && extra.is_empty() {
            return Vec::new();
        }
        self.lent(mw, |w| {
            let mut seen = std::collections::HashSet::new();
            let mut rows: Vec<(Entity, [f64; 3])> = Vec::new();
            if w.component_id::<Moving>().is_some() {
                crate::queries::with::<(Entity, &'static Pos), With<Moving>, _>(w, |q, w| {
                    for (e, p) in q.iter(w) {
                        if seen.insert(e) {
                            rows.push((e, [p.0.x, p.0.y, p.0.z]));
                        }
                    }
                });
            }
            let entered: Vec<Entity> = w.get_resource::<crate::bridge::view::Entered>().map(|v| v.0.iter().chain(&v.1).copied().collect()).unwrap_or_default();
            for e in entered.into_iter().chain(extra.iter().copied()) {
                if let Some(p) = w.get::<Pos>(e)
                    && seen.insert(e)
                {
                    rows.push((e, [p.0.x, p.0.y, p.0.z]));
                }
            }
            rows.into_iter().map(|(e, at)| (e, portal::usable(w, e, true), at)).collect()
        })
    }

    /// Whether an entity entered the world since the last step (`Entered`).
    pub fn any_entered(&self) -> bool {
        self.world().get_resource::<crate::bridge::view::Entered>().is_some_and(|v| !v.0.is_empty() || !v.1.is_empty())
    }

    /// Whether an entity carries `Moving`: an archetype with it holds an entity.
    pub fn any_moving(&self) -> bool {
        let w = self.world();
        w.component_id::<Moving>().is_some_and(|id| w.archetypes().iter().any(|a| !a.is_empty() && a.contains(id)))
    }

    /// Whether an entity carries `C`: an archetype with it holds an entity (no walk of the entities).
    pub fn any_with<C: Component>(&self) -> bool {
        let w = self.world();
        w.component_id::<C>().is_some_and(|id| w.archetypes().iter().any(|a| !a.is_empty() && a.contains(id)))
    }
    pub fn any_ear(&self) -> bool {
        self.any_with::<crate::behaviours::event_listen::Ear>()
    }
    pub fn any_item(&self) -> bool {
        self.any_with::<crate::item::Item>()
    }
    pub fn any_orb(&self) -> bool {
        self.any_with::<crate::orb::Orb>()
    }

    /// The box of the creature by its species' data and its age: a baby (`GrowAt` still running) has the baby box.
    pub fn size(&self, e: Entity) -> Option<(f32, f32)> {
        let w = self.world();
        let s = w.get::<Sizes>(e)?;
        Some(if w.get::<GrowAt>(e).is_some() { s.baby } else { s.adult })
    }

    /// The body yaw and the head pitch of `e` (what `set_rotation` writes); None without a `Yaw`, a pitch of 0 without a `Head`.
    pub fn rotation(&self, e: Entity) -> Option<[f32; 2]> {
        let w = self.world();
        let yaw = w.get::<Yaw>(e)?.0;
        Some([yaw, w.get::<Head>(e).map_or(0.0, |h| h.pitch)])
    }

    /// `Entity.canUsePortal(false)`: the entity may take a portal on its own (not as a passenger).
    pub fn can_use_portal(&self, e: Entity) -> bool {
        portal::usable(self.world(), e, false)
    }

    /// `Entity.portal_after_move` of the kinds that apply portal effects before their base tick: projectiles and tridents.
    pub fn portal_after_move(&self, e: Entity) -> bool {
        let w = self.world();
        w.get::<crate::projectile::Projectile>(e).is_some() || w.get::<crate::trident::Trident>(e).is_some()
    }

    /// The box of the entity, width and height (`Body`).
    pub fn portal_size(&self, e: Entity) -> Option<(f32, f32)> {
        portal::size_of(self.world(), e)
    }
}
