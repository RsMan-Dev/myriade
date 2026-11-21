//! The block world the herd sees and its change events.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// The block changes waiting for the next step: (x, y, z, block name).
#[derive(Resource, Default)]
pub struct BlockChanges(pub Vec<(i32, i32, i32, &'static str)>);

/// The block changes WITH properties waiting for the next step: (x, y, z, block name, properties): `blocks::set_state`.
#[derive(Resource, Default)]
pub struct StateChanges(pub Vec<(i32, i32, i32, &'static str, &'static [(&'static str, &'static str)])>, pub Vec<(i32, i32, i32, horizon_world::registry::BlockStateId)>);

pub(crate) const EGG: &str = "minecraft:turtle_egg";

/// The top of what the bench worlds generate (the patches reach y = 3).
pub(crate) const BUILD_TOP: i32 = 6;

/// The mob world of the bench (the blocks, the registries, the data), built once and kept for the process:
/// shared read-only by the systems (the kernels it holds keep their caches in thread locals).
#[derive(Clone, Copy)]
pub struct Mw(pub &'static MobWorld<'static>);

// SAFETY of the prototype: the world is only read once built, and every cache behind it is thread local.
unsafe impl Send for Mw {}

unsafe impl Sync for Mw {}

/// The leaked `World` of the blocks, for the one system that changes them (`block_changes`, which runs alone in the
/// chain: no other system holds a reference then).
#[derive(Clone, Copy)]
pub struct Blocks(pub *mut horizon_client_render::world::World);

// SAFETY: one system at a time dereferences it (`block_changes`, alone in the chain), and the pointer comes from the host, which outlives the Level.
unsafe impl Send for Blocks {}

unsafe impl Sync for Blocks {}

#[derive(Resource, Clone, Copy)]
pub struct Level {
    pub mw: Mw,
    pub blocks: Blocks,
    /// The occupancy masks of the world's sections (`occupancy.rs`): the block kernel's fast path.
    pub occ: &'static crate::occupancy::Occupancy,
    /// The walls are at |x| or |z| = `half`.
    pub half: i32,
}

impl Level {
    /// `MobWorld::no_collision` through the occupancy masks.
    pub fn no_collision(&self, b: Aabb) -> bool {
        let mw = self.mw.0;
        self.occ.no_collision(SceneCollisions { world: mw.world, binding: mw.collision, subject: None }, b)
    }
}

/// A walled stone slab of `half` blocks each way from the origin, on the prepared vanilla data.
pub fn build_level(half: i32, patches: bool) -> (Level, Species, Arc<horizon_client_lib::mobs::MobType>) {
    build_level_with(half, move |blocks: &mut horizon_client_render::world::World, registry: &horizon_world::registry::BlockRegistry| {
        let stone = registry.default_state("minecraft:stone").unwrap();
        let chunks = (half + 15) / 16 + 1;
        for cx in -chunks..chunks {
            for cz in -chunks..chunks {
                blocks.chunks.insert((cx, cz), Arc::new(Chunk::empty(cx, cz)));
            }
        }
        for x in -half..=half {
            for z in -half..=half {
                blocks.set(x, -2, z, stone).unwrap();
                blocks.set(x, -1, z, stone).unwrap();
                blocks.set(x, 0, z, stone).unwrap();
                if x.abs() == half || z.abs() == half {
                    blocks.set(x, 1, z, stone).unwrap();
                    blocks.set(x, 2, z, stone).unwrap();
                }
            }
        }
        if patches {
            // 3 x 3 hazards every 48 blocks: lava, a pond, powder snow (in the floor), fire and cactus (on it).
            let put = |blocks: &mut horizon_client_render::world::World, name: &str, x0: i32, z0: i32, y: i32| {
                let state = registry.default_state(name).unwrap();
                for x in x0..x0 + 3 {
                    for z in z0..z0 + 3 {
                        blocks.set(x, y, z, state).unwrap();
                    }
                }
            };
            // A low ceiling (stone at y = 3, 3 x 3) for the jump delay: a cow under it jumps for less than 10 ticks.
            put(blocks, "minecraft:stone", -20, -20, 3);
            let mut gx = -half + 8;
            while gx + 40 < half {
                let mut gz = -half + 8;
                while gz + 40 < half {
                    put(blocks, "minecraft:lava", gx, gz, 0);
                    put(blocks, "minecraft:water", gx + 8, gz, 0);
                    put(blocks, "minecraft:water", gx + 8, gz, -1);
                    put(blocks, "minecraft:powder_snow", gx + 16, gz, 0);
                    put(blocks, "minecraft:fire", gx, gz + 8, 1);
                    put(blocks, "minecraft:cactus", gx + 8, gz + 8, 1);
                    gz += 48;
                }
                gx += 48;
            }
        }
    })
}

/// `build_level` over a world of the caller's making (`build` fills the blocks).
pub fn build_level_with(half: i32, build: impl FnOnce(&mut horizon_client_render::world::World, &horizon_world::registry::BlockRegistry)) -> (Level, Species, Arc<horizon_client_lib::mobs::MobType>) {
    let (mw, types, blocks_ptr) =
        horizon_client_lib::mobs::world::leaked_test_world_mut(true, build).expect("HORIZON_TEST_RESOURCES");
    let chosen: &'static str = Box::leak(
        process_species_name().into_boxed_str(),
    );
    // Every species the prototype knows, by id: the same table for the whole process.
    let names: Vec<&'static str> = crate::species::names();
    let mut table: Vec<Species> = Vec::new();
    let mut chosen_kind = None;
    for (i, name) in names.into_iter().enumerate() {
        // An entity that is not a `Mob` (a display, a TNT, a projectile) has no `MobType`: its row is a stand-in (the cow's), never read.
        let kind = Arc::new(types.types.get(name).unwrap_or(&types.types[crate::species::lists::STAND_IN]).clone());
        let is_creature = types.types.contains_key(name);
        let mob = Mob::new(1, kind.clone(), Vec3::new(0.5, 1.0, 0.5), 0.0);
        let screaming = crate::species::registry().by_name(name).unwrap().screaming;
        table.push(Species {
            id: i as u8,
            malus: mob.ai.malus,
            can_float: mob.ai.nav.can_float,
            can_open_doors: mob.ai.nav.can_open_doors,
            can_pass_doors: mob.ai.nav.can_pass_doors,
            can_walk_over_fences: mob.ai.nav.can_walk_over_fences,
            follow_range: mob.attrs.follow_range as f32,
            max_health: mob.health,
            water_slowdown: kind.water_slowdown,
            lava_walker: kind.flags.lava_walker,
            undead: kind.flags.undead,
            water_efficiency: kind.water_efficiency as f32,
            eye_height: kind.eye_height,
            breathes_water: kind.flags.breathes_water,
            name,
            saved: crate::species::registry().by_name(name).unwrap().saved,
            width: kind.width as f32,
            height: kind.height as f32,
            step_height: kind.step_height as f32,
            gravity: kind.gravity,
            speed: mob.attrs.speed,
            flying_speed: horizon_client_lib::mobs::finalize::value(&mob, horizon_client_lib::mobs::finalize::Attr::FlyingSpeed),
            air_speed: kind.air_speed,
            omnidirectional: kind.flags.omnidirectional,
            baby: ((kind.width * kind.baby_scale) as f32, (kind.height * kind.baby_scale) as f32),
            goals: crate::species::config(name),
            rules: crate::species::registry().by_name(name).unwrap().rules,
            knockback_resistance: mob.attrs.knockback_resistance,
            armor: (kind.armor as f32, kind.armor_toughness as f32),
            attack_damage: mob.attrs.attack_damage,
            walk: walk_value_of(&mob, &kind),
            caps: crate::species::registry().by_name(name).unwrap().caps,
            brain: crate::species::registry().by_name(name).unwrap().brain,
            modes: crate::species::registry().by_name(name).unwrap().modes,
            tags: if is_creature { crate::cow::species::tags_of(&mob, &kind) | if screaming.is_some() { crate::cow::species::tag::SCREAMER } else { 0 } } else { 0 },
            max_air: if is_creature { mob.max_air_supply() } else { 300 },
            persistent: is_creature && mob.persistent,
            screaming,
            llama_strength: crate::species::registry().by_name(name).unwrap().llama_strength,
        });
        if name == chosen {
            chosen_kind = Some(kind);
        }
    }
    if let Some(items) = mw.items {
        crate::items::install(items);
        crate::equipment_items::install(items);
    }
    let _ = TYPES.set(types);
    let _ = TABLE.set(table.clone());
    // LEVER en attente : la racine est lue ici comme par `leaked_test_world_mut` ; elle devrait venir de `TestResources`.
    crate::type_tags::install(std::path::Path::new(&std::env::var("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES")), &table);
    let species = *table.iter().find(|s| s.name == chosen).expect("a known species");
    let _ = SPECIES.set(species);
    (Level { mw: Mw(mw), blocks: Blocks(blocks_ptr), occ: Box::leak(Box::default()), half }, species, chosen_kind.expect("a known species"))
}

/// A path that a block change asked to make again (`PathNavigation.pendingRecompute` and `delayedRecompute`).
#[derive(Component, Default)]
pub struct Recompute;

/// `PathNavigation.recomputePath`: made at once if the last recompute is more than 20 ticks old and the cow can update
/// a path (on the ground or in a liquid); else the old path is kept and the ask is made again every tick.
#[allow(clippy::type_complexity)]
pub(crate) fn recompute(
    clock: Res<Clock>,
    mut q: Query<(Entity, &mut Nav, &Contact, &FluidState), With<Recompute>>,
    mut commands: Commands,
) {
    for (e, mut nav, c, fs) in &mut q {
        if nav.path.as_ref().is_none_or(|p| p.is_done()) {
            nav.delayed = false;
            commands.entity(e).remove::<Recompute>();
        } else if clock.of(e).saturating_sub(nav.last_recompute) <= 20 || !(c.on_ground || fs.in_water || fs.in_lava) {
            nav.delayed = true;
        } else {
            nav.last_recompute = clock.of(e);
            nav.path = None;
            nav.delayed = false;
            commands.entity(e).remove::<(Following, Recompute)>().insert(NeedsPath);
        }
    }
}

/// A block was placed or removed (`ServerLevel.sendBlockUpdated`): the world takes it, the hazard column is read again,
/// and only what it can change is woken: the cows whose box, widened by a block, meets the block get their physics
/// (a floor that left), their suffocation and their hazard and fluid modules looked at again (their box counts as
/// changed, so `reindex` re-evaluates them); the cows that follow a path that the change is near the middle of
/// (`PathNavigation.shouldRecomputePath`) ask for a new path to the same spot; nobody else is touched.
#[allow(clippy::type_complexity)]
pub(crate) fn block_changes(
    mut changes: ResMut<BlockChanges>,
    mut state_changes: ResMut<StateChanges>,
    level: Res<Level>,
    mut hazards: ResMut<Hazards>,
    idx: Res<Index>,
    mut sun: ResMut<crate::hostile::SunDirty>,
    mut seen: ResMut<crate::hostile::ChangedBlocks>,
    mut dirty: Option<ResMut<crate::spawning::DirtyColumns>>,
    mut block_events: Option<ResMut<crate::block_events::BlockEvents>>,
    mut q: Query<(&mut Bounds, &Pos, &mut Contact)>,
    mut paths: Query<(Entity, &Pos, &Stroll, &mut Nav), With<Following>>,
    mut commands: Commands,
) {
    if changes.0.is_empty() && state_changes.0.is_empty() && state_changes.1.is_empty() {
        return;
    }
    let mw = level.mw.0;
    let named = std::mem::take(&mut changes.0).into_iter().map(|(x, y, z, n)| (x, y, z, n, &[][..], None));
    let with_state = std::mem::take(&mut state_changes.0).into_iter().map(|(x, y, z, n, p)| (x, y, z, n, p, None));
    let by_id = std::mem::take(&mut state_changes.1).into_iter().map(|(x, y, z, id)| (x, y, z, "", &[][..], Some(id)));
    for (x, y, z, name, props, id) in named.chain(with_state).chain(by_id).collect::<Vec<_>>() {
        let Some(state) = id.or_else(|| crate::blocks::resolve(mw, name, props)) else { continue };
        let name: &str = if id.is_some() { mw.registry.state(state).map_or("", |s| s.name.as_str()) } else { name };
        // SAFETY: this system runs alone in the chain, no reference to the blocks is alive meanwhile.
        unsafe {
            let world = &mut *level.blocks.0;
            let _ = world.set(x, y, z, state);
            level.occ.changed(&*world, level.mw.0.collision, x, y, z);
            world.path_type_block_updated(horizon_core::position::BlockPos::new(x, y, z));
            crate::pathsearch::layer_bump();
            // The real light engine: the sky above a column changed with the block.
            horizon_client_lib::mobs::world::relight_test_world(world, &[(x, y, z)]);
        }
        hazards.set(x, y, z, hazard_bit(name));
        hazards.eggs.remove(&[x, y, z]);
        if name == EGG {
            hazards.eggs.insert([x, y, z]);
        }
        seen.0.push((x, y, z));
        if let Some(b) = block_events.as_mut() {
            b.changed.push([x, y, z]);
        }
        if let Some(d) = dirty.as_mut() {
            d.0.push((x, z));
        }
        let cell = Aabb::new(Vec3::new(f64::from(x), f64::from(y), f64::from(z)), Vec3::new(f64::from(x + 1), f64::from(y + 1), f64::from(z + 1)));
        let reach = cell.inflate(Vec3::new(1.0, 1.0, 1.0));
        let mut near: Vec<Entity> = Vec::new();
        idx.for_near(f64::from(x) + 0.5, f64::from(z) + 0.5, |e| near.push(e));
        // The sky above a column changed: whoever stands in it looks again at the sun.
        sun.0.extend(near.iter().copied());
        for e in near {
            if let Ok((mut b, _, mut c)) = q.get_mut(e)
                && b.0.intersects(reach)
            {
                b.set_changed();
                // The ground it stood on is no longer known: the next step sweeps for real instead of resting.
                c.on_ground = false;
                commands.entity(e).insert((Moving, Recheck));
            }
        }
        // The paths that run near the change.
        for (e, pos, s, mut nav) in &mut paths {
            let Some(path) = nav.path.as_ref().filter(|p| !p.is_done()) else { continue };
            let Some(end) = path.nodes.last() else { continue };
            let middle = Vec3::new(
                (f64::from(end.x) + pos.0.x) / 2.0,
                (f64::from(end.y) + pos.0.y) / 2.0,
                (f64::from(end.z) + pos.0.z) / 2.0,
            );
            let remaining = (path.nodes.len() - path.next) as f64;
            let c = Vec3::new(f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5);
            if c.distance_squared(middle) < remaining * remaining && s.walking && !nav.delayed {
                commands.entity(e).insert(Recompute);
            }
        }
    }
}
