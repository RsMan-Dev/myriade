//! Experience orbs by network id: `OrbView`, a value built on demand from the orb's components, and the named writes the host makes to
//! an orb (its following player, its value and count, its fall). Nothing is kept on the host side.
use super::level::EcsLevel;
use bevy_ecs::{entity::Entity, world::World};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::{Air, Burning, Contact, Health, Pos, Vel, now_of},
    no_gravity,
    orb::Orb,
};

/// An experience orb as the host reads it: its value, the merged count, its age and its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbView {
    pub id: u32,
    pub value: i32,
    pub count: u32,
    /// `ExperienceOrb.age`: the ticks since it was made or last merged into.
    pub age: u32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub fall_distance: f64,
    /// `ExperienceOrb.followingPlayer`: the followed player's id.
    pub following: Option<u32>,
    pub health: f32,
    pub air: i32,
    pub no_gravity: bool,
    /// `Entity.remainingFireTicks`: 0 when it is not on fire.
    pub fire_ticks: i32,
}

/// The view of the orb `e`; None when it is not an orb.
fn view_of(w: &World, e: Entity) -> Option<OrbView> {
    let o = w.get::<Orb>(e)?;
    let now = now_of(w, e);
    let contact = w.get::<Contact>(e);
    Some(OrbView {
        id: o.id,
        value: o.value,
        count: o.count,
        age: now.saturating_sub(o.age_base) as u32,
        position: w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0),
        velocity: w.get::<Vel>(e).map_or(Vec3::ZERO, |v| v.0),
        on_ground: contact.is_some_and(|c| c.on_ground),
        fall_distance: contact.map_or(0.0, |c| c.fall_distance),
        following: o.following,
        health: w.get::<Health>(e).map_or(0.0, |h| h.0),
        air: w.get::<Air>(e).map_or(0, |a| a.0),
        no_gravity: no_gravity::get(w, e),
        fire_ticks: w.get::<Burning>(e).map_or(0, |b| (b.until - now as i64) as i32),
    })
}

impl EcsLevel {
    /// The view of the orb `id`; None when it is absent, asleep or the level is inert.
    pub fn orb(&self, id: u32) -> Option<OrbView> {
        let w = self.world()?;
        view_of(w, self.entity_of(id)?)
    }
    /// The views of every orb of the level, in the order of their ids.
    pub fn orb_views(&self) -> Vec<OrbView> {
        let ids = self.species_ids("minecraft:experience_orb");
        let mut views: Vec<OrbView> = ids.into_iter().filter_map(|id| self.orb(id)).collect();
        views.sort_by_key(|v| v.id);
        views
    }
    /// The orb `id`'s following player (`ExperienceOrb.followingPlayer`); false when it is not an orb.
    pub fn set_orb_following(&mut self, id: u32, player: Option<u32>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        w.get_mut::<Orb>(e).map(|mut o| o.following = player).is_some()
    }
    /// The value and the merged count of the orb `id`; false when it is not an orb.
    pub fn set_orb_stack(&mut self, id: u32, value: i32, count: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        w.get_mut::<Orb>(e).map(|mut o| {
            o.value = value;
            o.count = count;
        }).is_some()
    }
    /// The fall distance of the orb `id`; false when it is not an orb.
    pub fn set_orb_fall(&mut self, id: u32, fall: f64, on_ground: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        let Some(w) = self.world_mut() else { return false };
        w.get_mut::<Contact>(e).map(|mut c| {
            c.fall_distance = fall;
            c.on_ground = on_ground;
        }).is_some()
    }
}
