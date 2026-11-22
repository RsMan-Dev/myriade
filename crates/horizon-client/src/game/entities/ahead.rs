//! The dropped items' own moves, worked out ahead of the item loop on the tick's helper threads
//! while the parallel tick is on (`horizon_client::parallel`).
//!
//! An item's move (`ItemEntity.tick` up to its merge) reads only itself, the level's blocks, the
//! obstacle boxes and the border, and the item loop changes none of them: so an item's move worked
//! out on a copy before the loop is the move its turn would make. Its escape from inside a block
//! draws the item's own random, which the copy carries. A merge before its turn can change its
//! count, age and pickup delay: such an item moves at its turn instead. With a native mod loaded
//! (its hooks see every item) or a mod listening to the round, every item moves at its turn.
use super::*;
use super::{entity::Entity as _, native_ticks};
use horizon_client::{
    blocks::BlockFacts,
    entities::{ItemEntity, ItemSplash, ItemStep, ObstacleGrid},
    motion::MotionResources,
    world::World,
};
use horizon_core::geometry::{Aabb, Vec3, border::BorderGeometry};
use horizon_world::{block_collision::BlockCollisionRegistry, registry::BlockRegistry};

/// From this many items their moves go to the helper threads.
const MIN_ITEMS: usize = 256;

/// What an item's own move reads.
pub(in crate::game) struct ItemTables<'a> {
    world: &'a World,
    facts: &'a BlockFacts,
    collision: &'a BlockCollisionRegistry<'a>,
    motion: &'a MotionResources,
    registry: &'a BlockRegistry,
    border: &'a BorderGeometry,
    context: &'a horizon_world::native_load_context::LoopContext,
}

impl<'w> Game<'w> {
    pub(in crate::game) fn item_tables(&self) -> ItemTables<'_> {
        ItemTables {
            world: &self.world,
            facts: self.res.facts(),
            collision: self.collision,
            motion: self.res.motion(),
            registry: self.pack.assets.registry,
            border: self.border.geometry(),
            context: &self.loop_context,
        }
    }
}

impl ItemTables<'_> {
    /// `ItemEntity.tick` up to its move's vertical contact: the fluids, the water splash, the move
    /// (pushed out at `escape_speed` when inside a block) and its fall, with where it landed.
    pub(in crate::game) fn move_item(
        &self,
        item: &mut ItemEntity,
        escape_speed: Option<f32>,
        entity_boxes: &[Aabb],
        splash: &mut impl FnMut(ItemSplash),
    ) -> (ItemStep, Landed) {
        // Entity.baseTick splashes before moving, except on the first tick, at
        // the motion before the currents push it.
        let before = item.velocity;
        let heights = item.push_by_fluid(self.world, &self.motion.fluids, self.context);
        let mut splashed = 0;
        item.water_entry(heights.0 > 0.0, item.ticks == 0, before, &mut |s| {
            splashed += 1;
            splash(s);
        });
        item.lava_fall(heights.1 > 0.0);
        let mut step = item.tick_move_before_block_effects(
            self.world,
            self.facts,
            self.collision,
            escape_speed.is_some(),
            escape_speed,
            heights,
            entity_boxes,
            Some(self.border),
            Some(&self.motion.blocks),
            self.context,
        );
        let landed = item
            .check_fall(&mut step, |fall, from, moved| {
                if fall == 0.0 || moved.length_squared() < 1.0 {
                    return fall;
                }
                let reset = &self.motion.fall_reset;
                let hit = horizon_client::entities::fall_reset_hit(
                    self.context,
                    self.world,
                    &self.motion.fluids,
                    |p| reset.contains(&*self.facts.get(self.world.block(p.x, p.y, p.z)).name),
                    from,
                    moved,
                );
                if hit { 0.0 } else { fall }
            })
            .map(|fall| (fall, item.position, item.bounds(), splashed));
        (step, landed)
    }
    /// The rest of the move after its vertical contact: block effects, bubble columns, settling
    /// and the water entry. Whether the item lives on.
    pub(in crate::game) fn settle_item(
        &self,
        item: &mut ItemEntity,
        no_physics: bool,
        step: ItemStep,
        splash: &mut impl FnMut(ItemSplash),
    ) -> bool {
        item.finish_move_block_effects(
            self.world,
            self.facts,
            no_physics,
            Some(&self.motion.blocks),
            step,
            self.context,
        );
        item.push_by_bubble_column(self.world, self.registry, self.facts, self.collision);
        let alive = item.tick_settle(self.world, self.facts, no_physics, step, self.context);
        // ItemEntity.tick refreshes immersion and splashes on water entry.
        let before = item.velocity;
        let (water, _) = item.push_by_fluid(self.world, &self.motion.fluids, self.context);
        item.water_entry(water > 0.0, false, before, splash);
        alive
    }
}

/// `getEntityCollisions(entity, bounds.expandTowards(movement))`: the box around an item that its
/// move reads others' boxes in (a block of slack for the next velocity).
pub(super) fn reach(item: &ItemEntity) -> Aabb {
    let v = item.velocity;
    item.bounds()
        .inflate(Vec3::new(v.x.abs() + 1.0, v.y.abs() + 1.0, v.z.abs() + 1.0))
}

/// An item's move as its turn makes it: without physics (inside a block), alive, discarded, its
/// splashes, and where it landed.
pub(super) type Moved = (bool, bool, bool, Vec<ItemSplash>, Landed);

/// `Entity.checkFallDamage`'s landing: the fall, the item's position and box, and how many of the
/// tick's splashes came before it (`Entity.baseTick`'s; the water entry after the move comes
/// after). The block it lands on runs `fallOn` at the item's turn, as it can change the level
/// (its random, sounds): a move worked out ahead leaves the level alone.
pub(in crate::game) type Landed = Option<(f64, Vec3, Aabb, usize)>;

/// One item's move, worked out on a copy.
pub(super) struct Ahead {
    moved: ItemEntity,
    result: Moved,
    /// What a merge before its turn can change: its count, age and pickup delay.
    started: (u32, i32, i64),
}

fn started(item: &ItemEntity) -> (u32, i32, i64) {
    (item.stack.count, item.age, item.pickup_delay)
}

/// `item`'s whole move on a server with no native mod, from the tables alone.
fn move_alone(
    tables: &ItemTables,
    obstacles: &ObstacleGrid,
    item: &mut ItemEntity,
    found: &mut Vec<u32>,
    near: &mut Vec<Aabb>,
) -> Moved {
    obstacles.near_shared(reach(item), found, near);
    let no_physics = item.inside_block(tables.world, tables.collision, near, Some(tables.border));
    // `moveTowardsClosestSpace` draws the speed from the item's own random.
    let escape_speed = no_physics.then(|| item.random.next_float() * 0.2 + 0.1);
    let mut splashes = Vec::new();
    let (step, landed) = tables.move_item(item, escape_speed, near, &mut |s| splashes.push(s));
    if step.vertical_contact.is_some() && item.stack.count == 0 {
        return (no_physics, false, true, splashes, landed);
    }
    let alive = tables.settle_item(item, no_physics, step, &mut |s| splashes.push(s));
    (no_physics, alive, false, splashes, landed)
}

#[cfg(test)]
thread_local! {
    /// A test's choice of the path, over the parallel tick's switch.
    static AHEAD: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    /// The moves a test's tick put in place, and those a merge made it refuse.
    static TAKEN: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

/// Whether the moves are worked out ahead: while the parallel tick is on.
fn ahead() -> bool {
    #[cfg(test)]
    if let Some(ahead) = AHEAD.with(std::cell::Cell::get) {
        return ahead;
    }
    horizon_client::parallel::lanes() > 1
}

/// The moves worked out ahead, by the item's place in the loop.
pub(super) struct Moves(Vec<Option<Ahead>>);

impl Moves {
    /// The moves of the items that will tick (`ticks`), on the helper threads; none while the
    /// parallel tick is off, for few items, or with a mod that could see an item's move.
    pub(super) fn work_out(
        game: &Game,
        items: &[ItemEntity],
        obstacles: &ObstacleGrid,
        round: &native_ticks::TickRound,
        ticks: impl Fn(&ItemEntity) -> bool,
    ) -> Self {
        if !ahead()
            || items.len() < MIN_ITEMS

            || round.listening()
            || !mod_api::__private::mods().is_empty()
        {
            return Self(Vec::new());
        }
        let todo: Vec<usize> = (0..items.len()).filter(|&i| ticks(&items[i])).collect();
        let tables = game.item_tables();
        let done = horizon_client::parallel::par_map(&todo, MIN_ITEMS / 4, |part, out| {
            let (mut found, mut near) = (Vec::new(), Vec::new());
            for &i in part {
                let mut moved = items[i].clone();
                let result = move_alone(&tables, obstacles, &mut moved, &mut found, &mut near);
                let started = started(&items[i]);
                out.push((
                    i,
                    Ahead {
                        moved,
                        result,
                        started,
                    },
                ));
            }
        });
        let mut moves: Vec<Option<Ahead>> = (0..items.len()).map(|_| None).collect();
        for (i, a) in done {
            moves[i] = Some(a);
        }
        Self(moves)
    }
    /// Item `i`'s move, put in place, when it was worked out and no merge changed the item since.
    /// Under `HORIZON_PARALLEL_TICK=verify` the move is made again at its turn, on a copy, and a
    /// difference is logged.
    pub(super) fn take(
        &mut self,
        i: usize,
        item: &mut ItemEntity,
        again: impl FnOnce(&mut ItemEntity) -> Moved,
    ) -> Option<Moved> {
        let a = self.0.get_mut(i)?.take()?;
        let fits = a.started == started(item);
        #[cfg(test)]
        TAKEN.with(|t| {
            let (taken, refused) = t.get();
            t.set((taken + usize::from(fits), refused + usize::from(!fits)));
        });
        if !fits {
            return None;
        }
        if horizon_client::parallel::parallel_tick() == 2 {
            let mut serial = item.clone();
            let expected = again(&mut serial);
            check(item.id, &serial, &expected, &a.moved, &a.result);
        }
        *item = a.moved;
        Some(a.result)
    }
}

/// `HORIZON_PARALLEL_TICK=verify`: an item's move worked out ahead against the one at its turn.
fn check(id: u32, serial: &ItemEntity, expected: &Moved, ahead: &ItemEntity, got: &Moved) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CHECKED: AtomicU64 = AtomicU64::new(0);
    static DIFFERED: AtomicU64 = AtomicU64::new(0);
    if format!("{serial:?} {expected:?}") != format!("{ahead:?} {got:?}") {
        DIFFERED.fetch_add(1, Ordering::Relaxed);
        log::error!("parallel tick: item {id}'s move worked out ahead differs from its turn's");
    }
    let n = CHECKED.fetch_add(1, Ordering::Relaxed) + 1;
    if n == 1 || n.is_multiple_of(10_000) {
        log::info!(
            "parallel tick: {n} item moves worked out ahead checked, {} differed",
            DIFFERED.load(Ordering::Relaxed)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{AHEAD, TAKEN};
    use crate::game::game_tests::tests::server_game;
    use horizon_client::{chunk::Chunk, items::ItemStack};
    use horizon_core::{geometry::Vec3, random::RandomSource};
    use std::sync::Arc;

}
