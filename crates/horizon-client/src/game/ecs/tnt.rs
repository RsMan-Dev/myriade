//! The primed TNT's outputs from the ECS (`TntEvent`, merged into `HostEvent::Tnt`): the hiss of a lit fuse, the blast at the fuse's end,
//! the fall on the block. Each arm calls the host's effect in entities/tnt.rs, where the sound, the game events and the blast live.
use crate::game::Game;
use crate::mobs::Who;
use horizon_client::Result;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{host_out_tnt::TntEvent, targeting};

impl Game<'_> {
    /// The host side of a bridge `Who`: the player, or the mob of its ECS entity.
    fn host_who(&mut self, who: targeting::Who) -> Who {
        match who {
            targeting::Who::Player(_) => Who::Player,
            targeting::Who::Entity(e) => Who::Mob(self.ecs_id_of(e)),
        }
    }
}

impl Game<'_> {
    /// Applies one TNT event of P's: `Fuse` (the TNT's place is read from its view), `Explode`, `Landed`.
    pub(in crate::game) fn tnt_event(&mut self, ev: TntEvent) -> Result<()> {
        match ev {
            TntEvent::Fuse { entity } => {
                let id = self.ecs_id_of(entity);
                if let Some(v) = self.ecs_view(id) {
                    self.tnt_fuse_lit(v.position);
                }
            }
            TntEvent::Explode { at, power, owner } => {
                let owner = owner.map(|w| self.host_who(w));
                self.tnt_blast(at, power, owner)?
            }
            TntEvent::Landed { entity, at, fall } => {
                let id = self.ecs_id_of(entity);
                if let Some(bounds) = self.ecs.bounds(entity) {
                    self.tnt_landed(id, Vec3::new(at[0], at[1], at[2]), bounds, fall);
                }
            }
        }
        Ok(())
    }
}
