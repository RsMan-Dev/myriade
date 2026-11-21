//! The blasts and the splashes the host asks of its ECS world. The blast runs over the lent mob world (`explosion`), and the blocks it
//! breaks are reported by the next step. The creatures in reach are queued for the tracker: a push or a hit changed them.
use crate::behaviours::ender_teleport::WaterSensitive;
use crate::bridge::{
    host::{EcsWorld, HurtSource, Select},
    view,
};
use crate::cow::Burning;
use crate::explosion;
use crate::goals::Events;
use crate::projectile::Blast;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

impl EcsWorld {
    /// `explosion::explode`: the creatures and the players in reach are hurt and pushed; the blocks break as the mob griefing rule says.
    pub fn explode(&mut self, mw: &MobWorld<'_>, b: Blast) {
        self.lent(mw, |w| explosion::explode(w, b));
        self.mark_near(b.at, b.radius);
    }

    /// `explosion::explode_with`: as `explode`, the blocks stay when `breaks_blocks` is false.
    pub fn explode_with(&mut self, mw: &MobWorld<'_>, b: Blast, breaks_blocks: bool) {
        self.lent(mw, |w| explosion::explode_with(w, b, breaks_blocks));
        self.mark_near(b.at, b.radius);
    }

    /// Water reaches `e` (`ThrownPotion.applyWater`): its fire goes out, and a water-sensitive kind takes 1 from it.
    pub fn splashed(&mut self, mw: &MobWorld<'_>, e: Entity) {
        self.lent(mw, |w| {
            if w.get::<Burning>(e).is_some() {
                w.entity_mut(e).remove::<Burning>();
                w.resource_mut::<Events>().extinguished.push(e);
            }
            view::mark(w, e);
        });
        if self.world().get::<WaterSensitive>(e).is_some() {
            self.hurt(mw, e, 1.0, HurtSource::IndirectMagic { owner: None });
        }
    }

    /// Queues the creatures within the blast's diameter for the tracker.
    fn mark_near(&mut self, at: Vec3, radius: f32) {
        let q = Select { near: Some(([at.x, at.y, at.z], f64::from(radius * 2.0))), ..Default::default() };
        for e in self.select(&q) {
            view::mark(self.world_mut(), e);
        }
    }
}
