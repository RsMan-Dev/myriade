//! The body and the identity of an entity, read from its components on demand (no cache). A sleeping entity (`Disabled`) reads as
//! absent. The free functions take the world, so the client's level reads the same components by id.
use super::host::EcsWorld;
use crate::cow::{Bounds, Contact, Health, MaxHealth, Pos, PushFree, SpeciesTable, TypeName, Vel, Yaw};
use crate::goals::Carriers;
use crate::targeting::Eye;
use crate::type_tags;
use bevy_ecs::{entity::Entity, entity_disabling::Disabled, prelude::*, world::EntityRef};
use horizon_core::geometry::{Aabb, Vec3};

/// Where an entity is, how it moves, which way it faces, and whether it stands on the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f32,
    pub on_ground: bool,
}

/// The entity `e`, unless it is gone or sleeps.
fn awake(w: &World, e: Entity) -> Option<EntityRef<'_>> {
    let r = w.get_entity(e).ok()?;
    (!r.contains::<Disabled>()).then_some(r)
}

pub fn bounds(w: &World, e: Entity) -> Option<Aabb> {
    awake(w, e)?.get::<Bounds>().map(|b| b.0)
}

pub fn height(w: &World, e: Entity) -> Option<f32> {
    awake(w, e)?.get::<Bounds>().map(|b| b.0.size().y as f32)
}

pub fn alive(w: &World, e: Entity) -> bool {
    awake(w, e).is_some_and(|r| r.get::<Health>().is_some_and(|h| h.0 > 0.0))
}

pub fn motion(w: &World, e: Entity) -> Option<Motion> {
    let r = awake(w, e)?;
    let (p, v) = (r.get::<Pos>()?.0, r.get::<Vel>()?.0);
    Some(Motion {
        position: [p.x, p.y, p.z],
        velocity: [v.x, v.y, v.z],
        yaw: r.get::<Yaw>().map_or(0.0, |y| y.0),
        on_ground: r.get::<Contact>().is_some_and(|c| c.on_ground),
    })
}

pub fn velocity(w: &World, e: Entity) -> Option<Vec3> {
    awake(w, e)?.get::<Vel>().map(|v| v.0)
}

pub fn yaw(w: &World, e: Entity) -> Option<f32> {
    awake(w, e)?.get::<Yaw>().map(|y| y.0)
}

pub fn eye_offset(w: &World, e: Entity) -> Option<f64> {
    awake(w, e)?.get::<Eye>().map(|h| h.0)
}

pub fn pushable(w: &World, e: Entity) -> bool {
    awake(w, e).is_some_and(|r| r.contains::<Health>() && !r.contains::<PushFree>())
}

pub fn health(w: &World, e: Entity) -> Option<f32> {
    awake(w, e)?.get::<Health>().map(|h| h.0)
}

pub fn max_health(w: &World, e: Entity) -> Option<f32> {
    awake(w, e)?.get::<MaxHealth>().map(|h| h.0)
}

pub fn fall_distance(w: &World, e: Entity) -> f64 {
    awake(w, e).and_then(|r| r.get::<Contact>().map(|c| c.fall_distance)).unwrap_or(0.0)
}

pub fn has_tag(w: &World, e: Entity, tag: &str) -> bool {
    awake(w, e).is_some() && type_tags::named(w, tag, e)
}

pub fn species(w: &World, e: Entity) -> Option<&'static str> {
    awake(w, e)?.get::<TypeName>().map(|t| t.0)
}

/// The alive entities of the species `kind`, from its census (kept by the hooks): no scan. 0 for an unknown species.
pub fn species_count(w: &World, kind: &str) -> usize {
    let Some(id) = w.get_resource::<SpeciesTable>().and_then(|t| t.0.iter().find(|s| s.name == kind)).map(|s| s.id) else {
        return 0;
    };
    w.get_resource::<Carriers>().and_then(|c| c.1.0.get(usize::from(id)).copied()).unwrap_or(0) as usize
}

impl EcsWorld {
    pub fn bounds(&self, e: Entity) -> Option<Aabb> {
        bounds(self.world(), e)
    }
    pub fn alive(&self, e: Entity) -> bool {
        alive(self.world(), e)
    }
    pub fn motion(&self, e: Entity) -> Option<Motion> {
        motion(self.world(), e)
    }
    pub fn velocity(&self, e: Entity) -> Option<Vec3> {
        velocity(self.world(), e)
    }
    pub fn yaw(&self, e: Entity) -> Option<f32> {
        yaw(self.world(), e)
    }
    pub fn eye_offset(&self, e: Entity) -> Option<f64> {
        eye_offset(self.world(), e)
    }
    pub fn height(&self, e: Entity) -> Option<f32> {
        height(self.world(), e)
    }
    pub fn pushable(&self, e: Entity) -> bool {
        pushable(self.world(), e)
    }
    pub fn health(&self, e: Entity) -> Option<f32> {
        health(self.world(), e)
    }
    pub fn max_health(&self, e: Entity) -> Option<f32> {
        max_health(self.world(), e)
    }
    pub fn fall_distance(&self, e: Entity) -> f64 {
        fall_distance(self.world(), e)
    }
    pub fn has_tag(&self, e: Entity, tag: &str) -> bool {
        has_tag(self.world(), e, tag)
    }
    pub fn species(&self, e: Entity) -> Option<&'static str> {
        species(self.world(), e)
    }
    pub fn species_count(&self, kind: &str) -> usize {
        species_count(self.world(), kind)
    }
}
