//! Horizon's item on the ground, run the naive way: every item, every tick, in the order the game runs `ItemEntity.tick` (the fluid push, the
//! move, the bubble column, the drags, the burning and the lava, the merge). The control of `item.rs` and the cost it is measured against.
use horizon_client_lib::entities::{merge_with_neighbours, update_entity_fluids, ItemCells, ItemEntity};
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;

pub fn original(id: u32, stack: ItemStack, at: Vec3, velocity: Vec3, delay: i64) -> ItemEntity {
    ItemEntity {
        random: horizon_core::random::EntityRandom::own(horizon_core::random::LegacyRandom::new(i64::from(id))),
        attachment_lifetime: Default::default(),
        rotation: [0.0, 0.0],
        join: Default::default(),
        health: 5,
        fire_ticks: 0,
        id,
        stack,
        thrower: None,
        target: None,
        position: at,
        previous: at,
        velocity,
        on_ground: false,
        fall_distance: 0.0,
        age: 0,
        pickup_delay: delay,
        bob_offset: 0.0,
        ticks: 0,
        needs_sync: false,
        stuck: Vec3::ZERO,
        was_touching_water: false,
        no_gravity: false,
        vertical_collision: false,
        horizontal_collision: false,
    }
}
/// `ItemEntity.tick` as the game runs it for one item (a server without mods): false when it is gone.
pub fn tick_original(mw: &MobWorld<'static>, it: &mut ItemEntity) -> bool {
    let (world, facts, ctx) = (mw.world, mw.facts, mw.loop_context);
    let can_hurt = |_: &ItemStack, _: &str| true;
    let start = it.position;
    let start_in_lava = it.in_lava(world, facts);
    let no_physics = it.inside_block(world, mw.collision, &[], None);
    let escape = no_physics.then(|| it.random.next_float() * 0.2 + 0.1);
    let before = it.velocity;
    let (b, p, e) = (it.bounds(), it.position, it.fluid_eye_y());
    let (water, lava, _) = update_entity_fluids(b, p, e, &mut it.velocity, world, mw.fluid_tables, ctx);
    it.water_entry(water > 0.0, it.ticks == 0, before, &mut |_| {});
    it.lava_fall(lava > 0.0);
    let mut step = it.tick_move_before_block_effects(world, facts, mw.collision, no_physics, escape, (water, lava), &[], None, Some(mw.movement), ctx);
    let _ = it.check_fall(&mut step, |fall, _, _| fall);
    it.finish_move_block_effects(world, facts, no_physics, Some(mw.movement), step, ctx);
    horizon_client_lib::motion::bubble_column_push_over(world, mw.registry, facts, mw.collision, it.bounds(), &mut it.velocity, |_| {});
    let alive = it.tick_settle(world, facts, no_physics, step, ctx);
    let before = it.velocity;
    let (b, p, e) = (it.bounds(), it.position, it.fluid_eye_y());
    let (water2, _, _) = update_entity_fluids(b, p, e, &mut it.velocity, world, mw.fluid_tables, ctx);
    it.water_entry(water2 > 0.0, false, before, &mut |_| {});
    let _ = it.burning_tick(start_in_lava, &can_hurt);
    let void = f64::from(world.shape.min_y - 64);
    if it.health <= 0 {
        return false;
    }
    if start.y >= void && !it.environment_tick(world, facts, mw.registry, &can_hurt).alive {
        return false;
    }
    alive
}
/// The original's list of items: each ticks in the order of the list, then merges.
pub struct Original {
    pub mw: &'static MobWorld<'static>,
    pub items: Vec<ItemEntity>,
}
impl Original {
    pub fn tick(&mut self) {
        let mw = self.mw;
        let max = |_: &ItemStack| 64;
        let n = self.items.len();
        let mut removed = vec![false; n];
        let mut cells = ItemCells::new(&self.items);
        for i in 0..n {
            if removed[i] {
                continue;
            }
            let alive = tick_original(mw, &mut self.items[i]);
            let it = &self.items[i];
            if !alive {
                removed[i] = true;
                cells.remove(i, it.previous);
                continue;
            }
            cells.moved(i, it.previous, it.position);
            let block = |v: Vec3| (v.x.floor(), v.y.floor(), v.z.floor());
            let rate = if block(it.previous) == block(it.position) { 40 } else { 2 };
            if it.ticks % rate == 0 {
                merge_with_neighbours(&mut self.items, &mut removed, &mut cells, i, true, &max);
            }
        }
        let mut r = removed.into_iter();
        self.items.retain(|_| !r.next().unwrap());
    }
}

