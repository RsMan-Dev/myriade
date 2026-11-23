//! Helpers of the ECS tests: the herd world (`Herd`), its players and creatures, and the vanilla registries the tests read.
#![allow(dead_code)]
use bevy_ecs::prelude::*;
use horizon_client_lib::{items::Items, merchant::Trades, mobs::{MobWorld, PlayerView}};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    cow::*,
    goals::*,
};
use horizon_items::enchant::Enchantments;

pub fn player_view(at: Vec3, item: Option<&str>) -> PlayerView {
    PlayerView {
        position: at,
        bounds: Aabb::new(Vec3::new(at.x - 0.3, at.y, at.z - 0.3), Vec3::new(at.x + 0.3, at.y + 1.8, at.z + 0.3)),
        eye: [at.x, at.y + 1.62, at.z],
        alive: true,
        invulnerable: false,
        creative: false,
        sneaking: false,
        swimming: false,
        look: [0.0; 3],
        main_hand: item.map(Into::into),
        off_hand: None,
        head: None,
        chest: None,
        legs: None,
        feet: None,
        armor_cover: 0.0,
        invisible: false,
        spectator: false,
        gold_armor: false,
        hurt_by: None,
        last_hurt: None,
        velocity: Vec3::ZERO,
        yaw: 0.0,
        sprinting: false,
        riding: false,
        hero: false,
        effects: Vec::new(),
        uuid: [0; 4],
        allies: Default::default(),
        remote: false,
        sleeping: false,
        sleep_timer: 0,
        passenger: false,
        riding_vehicle: None,
        vehicle: None,
    }
}
/// The item that tempts the species of the run (`HORIZON_SPECIES`): its id and its name.
pub fn tempt_item() -> (u32, &'static str) {
    match species().name {
        "minecraft:pig" => (carrot(), "minecraft:carrot"),
        "minecraft:chicken" => (wheat_seeds(), "minecraft:wheat_seeds"),
        _ => (wheat(), "minecraft:wheat"),
    }
}
pub fn have_data() -> bool {
    // The tests inject their own configuration of the modules: every module is inscribed (the frame inscribes carried ones only).
    horizon_ecs_proto::species::inscribe_every_module();
    // A test without its data must fail, never pass empty.
    assert!(std::env::var_os("HORIZON_TEST_RESOURCES").is_some(), "données de test absentes : exporte HORIZON_TEST_RESOURCES");
    true
}
/// Whether the run's species (`HORIZON_SPECIES`, required) is none of `names`: the test is then skipped, and the skip is noted (printed and logged), never silent.
pub fn wrong_species(names: &[&str]) -> bool {
    let sp = horizon_ecs_proto::cow::species_var().expect("HORIZON_SPECIES absent : une espèce par passe, ex. HORIZON_SPECIES=minecraft:cow");
    let wrong = !names.contains(&sp.as_str());
    if wrong {
        let t = std::thread::current();
        horizon_ecs_proto::cow::skip_note(t.name().unwrap_or("?"), &format!("written for {names:?}, run is {sp}"));
    }
    wrong
}
/// Whether the run's species is one of `names`, whose own rule changes what the test asserts (the reason is noted with the skip, never silent).
pub fn unfit_species(names: &[&str], why: &str) -> bool {
    let sp = horizon_ecs_proto::cow::species_var().expect("HORIZON_SPECIES absent");
    let unfit = names.contains(&sp.as_str());
    if unfit {
        let t = std::thread::current();
        horizon_ecs_proto::cow::skip_note(t.name().unwrap_or("?"), &format!("{sp}: {why}"));
    }
    unfit
}
pub fn ecs_cow(h: &mut Herd, x: f64, y: f64, z: f64, seed: u64) -> Entity {
    let e = h.world.spawn(cow_bundle(x, z, seed)).id();
    horizon_ecs_proto::type_tags::mark(&mut h.world, e, species().id);
    horizon_ecs_proto::rules::equip_all(&mut h.world, e, species().rules);
    h.world.get_mut::<Pos>(e).unwrap().0.y = y;
    h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, y, z), species().width, species().height);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
    // A cow at rest has the resting vertical velocity (a fresh one is in the air for its first tick).
    if y <= 1.0 {
        h.world.get_mut::<Vel>(e).unwrap().0.y = -0.08 * f64::from(0.98_f32);
    }
    e
}
/// A creature of the water lives in a pool (out of it, it flops in its own tick): a block of water around the cell where it stands.
pub fn pool_if_aquatic(h: &mut Herd, x: i32, z: i32) {
    if horizon_ecs_proto::type_tags::lists("can_breathe_under_water", species().id) {
        for dx in -2..=2 {
            for dz in -3..=3 {
                for y in 1..=2 {
                    h.set_block(x + dx, y, z + dz, "minecraft:water");
                }
            }
        }
    }
}
/// Where a creature stands at rest: put back after each tick, so one that flops, hops or hovers (fish out of water, slimes, flyers) is where
/// the test aimed, as the cow is.
pub struct Pin(Vec3, Aabb, Vec3);
pub fn pin(h: &Herd, e: Entity) -> Pin {
    Pin(h.world.get::<Pos>(e).unwrap().0, h.world.get::<Bounds>(e).unwrap().0, h.world.get::<Vel>(e).unwrap().0)
}
pub fn tick_pinned(h: &mut Herd, e: Entity, at: &Pin) {
    h.tick();
    if h.world.get_entity(e).is_ok() {
        h.world.get_mut::<Pos>(e).unwrap().0 = at.0;
        h.world.get_mut::<Bounds>(e).unwrap().0 = at.1;
        h.world.get_mut::<Vel>(e).unwrap().0 = at.2;
    }
}
pub fn health(h: &Herd, e: Entity) -> f32 {
    h.world.get::<Health>(e).map_or(0.0, |x| x.0)
}

/// A chest in the block entities of a world (`*h.world.resource::<Level>()`) with these slots: the block entity the
/// golems read. The block itself is placed by `Herd::set_block`.
pub fn set_chest(level: Level, x: i32, y: i32, z: i32, slots: Vec<Option<horizon_client_lib::items::ItemStack>>) {
    // SAFETY: the test thread owns the world between two steps; no reference to its block entities is alive.
    unsafe {
        let world = &mut *level.blocks.0;
        world.block_entities.insert((x, y, z), horizon_client_render::chunk::block_entity::BlockEntity::Chest(slots));
    }
}
pub fn xz(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}
pub fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}
pub fn is_creature(id: &str) -> bool {
    horizon_ecs_proto::cow::has_mob_kind(id)
}
/// The yaw (degrees, vanilla `lookAt`) a head at `from` takes to look at `to`, in the x/z plane.
pub fn look_yaw(from: Vec3, to: Vec3) -> f32 {
    ((to.z - from.z).atan2(to.x - from.x).to_degrees() - 90.0) as f32
}

/// The species under test walks on the ground with its own locomotion (a creature standing in water, on honey or in a bubble column):
/// objects, projectiles and flyers do not take these scenes (they have no ground mode, or no gravity to bring them to rest).
pub fn ground_walker() -> bool {
    let s = species();
    s.gravity.is_some_and(|g| g > 0.0) && s.modes.iter().any(|(m, _)| *m == horizon_ecs_proto::locomotion::Medium::Ground)
}

/// The items of the herd world, and the enchantments and trades read from the vanilla data (`HORIZON_TEST_RESOURCES`).
pub fn registries() -> (&'static Items, &'static Enchantments, Trades) {
    let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let items = Herd::with_slab_awake(2, false, false).world.resource::<Level>().mw.0.items.expect("items");
    let defaults = horizon_world::item_defaults::read(&root).expect("item defaults");
    let enchantments: &'static Enchantments = Box::leak(Box::new(Enchantments::load(&root, &defaults).expect("enchantments")));
    let trades = Trades::load(&root, &horizon_world::loot::PackOverlay::NONE);
    (items, enchantments, trades)
}

/// Inserts the trade data of the vanilla registries into the herd world, as the host does when a villager's trades are read.
pub fn insert_trade_data(h: &mut Herd) {
    let (items, enchantments, trades) = registries();
    h.world.insert_resource(horizon_ecs_proto::behaviours::trade_offers::TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
}

/// A host world whose zone ticks around the origin: a player there and two steps, so what is spawned next is awake (a
/// creature outside a ticking chunk sleeps and reads as absent).
pub fn awake_host_with(setup: &horizon_ecs_proto::bridge::host::HostSetup, build: impl FnOnce(&mut horizon_client_lib::world::World, &horizon_world::registry::BlockRegistry)) -> (&'static MobWorld<'static>, horizon_ecs_proto::bridge::host::EcsWorld) {
    use horizon_ecs_proto::bridge::host::{EcsWorld, PlayerFact};
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = horizon_client_lib::mobs::world::leaked_test_world_mut(true, build).expect("a prepared world");
    let mut h = EcsWorld::new(setup, mw, types, &tags);
    h.set_player(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, ..Default::default() }, PlayerFact { alive: true, targetable: true, hero: None });
    // The players' chunks reach the zone one step after the player is set.
    h.step(mw);
    h.step(mw);
    (mw, h)
}

/// `awake_host_on_stone` inside a world border (`border`: its geometry, the walls a mob meets at its edge).
pub fn awake_host_in_border(setup: &horizon_ecs_proto::bridge::host::HostSetup, border: horizon_core::geometry::border::BorderGeometry) -> (&'static MobWorld<'static>, horizon_ecs_proto::bridge::host::EcsWorld) {
    use horizon_ecs_proto::bridge::host::{EcsWorld, PlayerFact};
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let build = |blocks: &mut horizon_client_lib::world::World, registry: &horizon_world::registry::BlockRegistry| {
        for cx in -2..2 {
            for cz in -2..2 {
                blocks.chunks.insert((cx, cz), std::sync::Arc::new(horizon_client_render::chunk::Chunk::empty(cx, cz)));
            }
        }
        let stone = registry.default_state("minecraft:stone").unwrap();
        for x in -8..8 {
            for z in -8..8 {
                blocks.set(x, 0, z, stone).unwrap();
            }
        }
    };
    let (mw, types, _) = horizon_client_lib::mobs::world::leaked_test_world_bordered(true, Some(border), build).expect("a prepared world");
    let mut h = EcsWorld::new(setup, mw, types, &tags);
    h.set_player(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, ..Default::default() }, PlayerFact { alive: true, targetable: true, hero: None });
    h.step(mw);
    h.step(mw);
    (mw, h)
}

pub fn awake_host(setup: &horizon_ecs_proto::bridge::host::HostSetup) -> (&'static MobWorld<'static>, horizon_ecs_proto::bridge::host::EcsWorld) {
    awake_host_with(setup, |_, _| {})
}

/// `awake_host` over a stone floor at y = 0 (x and z from -8 to 8), for what walks or paths.
pub fn awake_host_on_stone(setup: &horizon_ecs_proto::bridge::host::HostSetup) -> (&'static MobWorld<'static>, horizon_ecs_proto::bridge::host::EcsWorld) {
    awake_host_with(setup, |blocks, registry| {
        for cx in -2..2 {
            for cz in -2..2 {
                blocks.chunks.insert((cx, cz), std::sync::Arc::new(horizon_client_render::chunk::Chunk::empty(cx, cz)));
            }
        }
        let stone = registry.default_state("minecraft:stone").unwrap();
        for x in -8..8 {
            for z in -8..8 {
                blocks.set(x, 0, z, stone).unwrap();
            }
        }
    })
}
