//! The body and the identity of an entity, read from its components on demand (no cache). A sleeping entity (`Disabled`) reads as
//! absent. The free functions take the world, so the client's level reads the same components by id.
use super::host::EcsWorld;
use crate::cow::{Bounds, Contact, FluidState, Health, MaxHealth, Pos, PushFree, SpeciesTable, TYPES, TypeName, Vel, Yaw, now_of};
use myriade_engine_lib::mobs::MobTypes;
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

/// Every entity with a kind, asleep or not: its entity, box, whether it is alive, and whether it is a shulker's body (the
/// attached-face state, which only the shulker holds).
pub fn mob_boxes(w: &World) -> Vec<(Entity, Aabb, bool, bool)> {
    w.iter_entities()
        .filter(|r| r.contains::<TypeName>())
        .filter_map(|r| {
            let alive = r.get::<Health>().is_some_and(|h| h.0 > 0.0);
            let shulker = alive && r.contains::<crate::behaviours::attached_face::Attached>();
            Some((r.id(), r.get::<Bounds>()?.0, alive, shulker))
        })
        .collect()
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

/// `getYHeadRot` and `yBodyRot` of a creature with a head; the head yaw is NaN until it first moves (`Head.yaw`).
pub fn head_body_yaw(w: &World, e: Entity) -> Option<(f32, f32)> {
    awake(w, e)?.get::<crate::goals::Head>().map(|h| (h.yaw, h.body))
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
    let r = awake(w, e)?;
    r.get::<MaxHealth>().map(|h| h.0).or_else(|| r.contains::<crate::cow::SpeciesId>().then(|| crate::cow::max_health(w, e)))
}

pub fn fall_distance(w: &World, e: Entity) -> f64 {
    awake(w, e).and_then(|r| r.get::<Contact>().map(|c| c.fall_distance)).unwrap_or(0.0)
}

pub fn has_tag(w: &World, e: Entity, tag: &str) -> bool {
    awake(w, e).is_some() && type_tags::named(w, tag, e)
}

/// The entity sits in the pose that blocks the chest below it (`ChestBlock.isCatSittingOnChest`): it carries the sitting-on-blocks
/// state of the cat goals and is in the pose. No species name is read.
pub fn sits_on_chest(w: &World, e: Entity) -> bool {
    awake(w, e).is_some_and(|r| r.get::<crate::behaviours::cat_state::CatPose>().is_some_and(|p| p.sitting_pose))
}

pub fn species(w: &World, e: Entity) -> Option<&'static str> {
    awake(w, e)?.get::<TypeName>().map(|t| t.0)
}

/// Whether `e` stands in water (`FluidState.in_water`); false without the component.
pub fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}

/// The category name of the species of `e` (`EntityType.getCategory`), from the prepared mob types; None without a species or types.
pub fn category(w: &World, e: Entity) -> Option<&'static str> {
    let types: &'static MobTypes = *TYPES.get()?;
    types.types.get(species(w, e)?).map(|t| t.category.as_str())
}

/// `EntityType.fireImmune` of the species of `e`, from the prepared mob types.
pub fn fire_immune(w: &World, e: Entity) -> Option<bool> {
    let types: &'static MobTypes = *TYPES.get()?;
    types.types.get(species(w, e)?).map(|t| t.flags.fire_immune)
}

/// The category name of the species `name`, from the prepared mob types.
pub fn category_of(name: &str) -> Option<&'static str> {
    let types: &'static MobTypes = *TYPES.get()?;
    types.types.get(name).map(|t| t.category.as_str())
}

/// `Mob.isEnemy` (`instanceof Enemy`): a monster, but not one that carries `NotEnemy` (zombie horse, camel husk, zombie nautilus).
pub fn is_enemy(w: &World, e: Entity) -> bool {
    category(w, e) == Some("monster") && w.get::<crate::hostile::NotEnemy>(e).is_none()
}

/// The largest half width of the prepared mob types (0 before they are set); computed once they are.
pub fn max_half_width() -> f32 {
    static MAX: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    let types: &'static MobTypes = match TYPES.get() {
        Some(t) => *t,
        None => return 0.0,
    };
    *MAX.get_or_init(|| types.types.values().map(|t| (t.width * 0.5) as f32).fold(0.0, f32::max))
}

/// Whether `e` is in its first tick: its own time, which `now_of` reads, is still 0.
pub fn first_tick(w: &World, e: Entity) -> bool {
    now_of(w, e) == 0
}

/// The alive entities of the species `kind`, from the name index the `TypeName` hooks keep: no scan, and every entity has a name
/// (the census by `SpeciesId` misses the species without one: item, trident, tnt). 0 for an unknown species.
pub fn species_count(w: &World, kind: &str) -> usize {
    w.get_resource::<Carriers>().and_then(|c| c.1.1.of(kind)).map_or(0, |s| s.len())
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
