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
            crate::tear_down::linger(w, e);
            names
        })
    }

    /// The entities that moved this tick, as (entity, may take a portal with its vehicle, where it is). Only the `Moving` ones are visited.
    pub fn portal_entries(&mut self, mw: &MobWorld<'_>) -> Vec<(Entity, bool, [f64; 3])> {
        self.lent(mw, |w| {
            let movers: Vec<(Entity, [f64; 3])> = w
                .query_filtered::<(Entity, &Pos), With<Moving>>()
                .iter(w)
                .map(|(e, p)| (e, [p.0.x, p.0.y, p.0.z]))
                .collect();
            movers
                .into_iter()
                .map(|(e, at)| (e, portal::usable(w, e, true), at))
                .collect()
        })
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
