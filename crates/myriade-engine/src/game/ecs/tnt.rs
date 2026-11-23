//! The primed TNT's outputs from the ECS (`TntEvent`, merged into `HostEvent::Tnt`): the hiss of a lit fuse, the blast at the fuse's end,
//! the fall on the block. Each arm calls the host's effect in entities/tnt.rs, where the sound, the game events and the blast live.
use crate::game::Game;
use crate::mobs::Who;
use myriade_engine::Result;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{bridge::host::Select, host_out_tnt::TntEvent, targeting, tnt::Primed};

impl Game<'_> {
    /// The host side of a bridge `Who`: the player, or the mob of its ECS entity.
    pub(in crate::game) fn host_who(&mut self, who: targeting::Who) -> Who {
        match who {
            targeting::Who::Player(_) => Who::Player,
            targeting::Who::Entity(e) => Who::Mob(self.ecs_id_of(e)),
        }
    }

    /// The bridge `Who` of a command target: the acting player by its entity id, a mob by its ECS entity; others have none.
    pub(in crate::game) fn bridge_who_of(&self, by: Option<crate::game::commands::Target>) -> Option<targeting::Who> {
        use crate::game::commands::Target;
        match by? {
            Target::Player => u32::try_from(self.own_entity_id()).ok().map(targeting::Who::Player),
            Target::Mob(m) => self.ecs.entity_of(m).map(targeting::Who::Entity),
            _ => None,
        }
    }

    /// `host_who` of an optional bridge `Who` (an event whose cause may be absent).
    pub(in crate::game) fn host_who_opt(&mut self, who: Option<targeting::Who>) -> Option<Who> {
        who.map(|w| self.host_who(w))
    }
}

impl Game<'_> {
    /// The potent sulfur gas's push: each primed TNT in `area` whose vertical speed is under `limit` gains `push` up.
    pub(in crate::game) fn ecs_tnt_lift(&mut self, area: Aabb, limit: f64, push: f64) {
        let q = Select { kind: Some("minecraft:tnt".into()), tag: None, near: None, limit: usize::MAX };
        for e in self.ecs.select(&q) {
            let Some(id) = self.ecs.id_of(e) else { continue };
            let (Some(bounds), Some(v)) = (self.ecs.bounds(id), self.ecs.velocity(id)) else { continue };
            if bounds.intersects(area) && v.y < limit {
                self.ecs.set_velocity(e, Vec3::new(v.x, v.y + push, v.z));
            }
        }
    }

    /// Applies one TNT event of P's: `Fuse` (the TNT's place is read from its view), `Explode`, `Landed`.
    pub(in crate::game) fn tnt_event(&mut self, ev: TntEvent) -> Result<()> {
        match ev {
            TntEvent::Fuse { entity } => {
                let id = self.ecs_id_of(entity);
                // The one who lit it is kept on the TNT: the event is heard at the step after the click, when no actor stands any more.
                let owner = self.ecs.world().and_then(|w| w.get::<horizon_ecs_proto::tnt::Primed>(entity)).and_then(|p| p.owner);
                let owner = owner.map(|o| self.host_who(o));
                if let Some(v) = self.ecs_view(id) {
                    self.tnt_fuse_lit(v.position, owner);
                }
            }
            TntEvent::Explode { at, power, owner, used_portal } => {
                let owner = owner.map(|w| self.host_who(w));
                self.tnt_blast(at, power, owner, used_portal)?
            }
            TntEvent::Landed { entity, at, fall } => {
                let id = self.ecs_id_of(entity);
                if let Some(bounds) = self.ecs.bounds_of(entity) {
                    self.tnt_landed(id, Vec3::new(at[0], at[1], at[2]), bounds, fall);
                }
            }
        }
        Ok(())
    }
    /// Whether the primed TNT `id` went through a nether portal (`PrimedTnt.usedPortal`).
    pub(in crate::game) fn ecs_tnt_used_portal(&self, id: u32) -> bool {
        let (Some(w), Some(e)) = (self.ecs.world(), self.ecs.entity_of(id)) else { return false };
        w.get::<Primed>(e).is_some_and(|p| p.used_portal)
    }
    /// Sets the portal flag of the primed TNT `id` (`PrimedTnt.teleport`).
    pub(in crate::game) fn ecs_set_tnt_used_portal(&mut self, id: u32, used: bool) {
        let Some(e) = self.ecs.entity_of(id) else { return };
        if let Some(mut p) = self.ecs.world_mut().and_then(|w| w.get_mut::<Primed>(e)) {
            p.used_portal = used;
        }
    }
    /// The owner of the primed TNT `id`, named as the host names it (`host_who`).
    pub(in crate::game) fn ecs_tnt_owner(&mut self, id: u32) -> Option<Who> {
        let e = self.ecs.entity_of(id)?;
        let owner = self.ecs.world()?.get::<Primed>(e)?.owner?;
        Some(self.host_who(owner))
    }
}
