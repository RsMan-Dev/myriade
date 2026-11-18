//! The ECS world of the cows: the entities, their order (kept here, never taken from a query), the
//! pushes waiting for a cow's turn, and the pass over every cow of a tick.
//!
//! Order: `order` is the list of Horizon (a cow is added at the end, as `Game::insert_spawned_mob` does
//! for a child), every pass visits the cows by it, one cow's whole tick after the other's, and each
//! cow's steps (`tick.rs`) run in a fixed chain. No system is scheduled by `bevy_ecs`, no query
//! iterates a table: a cow is fetched by its `Entity`.
use super::{components::*, event::CowEvent, goals::CowGoalSelector, view::{Cow, CowItem}};
use crate::{
    health::DamageLevel,
    mobs::{Mob, MobWorld, PendingPushes, Seen, Who},
};
use bevy_ecs::{entity::Entity, query::QueryState, world::World};

pub struct CowWorld {
    pub world: World,
    /// The cows in Horizon's order.
    pub order: Vec<Entity>,
    query: QueryState<Cow>,
    pub pushes: PendingPushes,
    /// The mutation of the test of the order: the pass goes through the cows backwards.
    pub mutate_order: bool,
    /// Whether the cows spawned from now on keep and put back the outputs of their resting ticks.
    pub rest: bool,
    /// Dense unordered pass (a measurement, not vanilla order).
    pub dense: bool,
    /// The pass as systems (`lanes.rs`), and whether the free lane is used (else every cow is coupled).
    pub systems: bool,
    pub lanes: bool,
    pub all_free: bool,
    snapshot: Vec<Seen>,
    next_id: u32,
}

impl CowWorld {
    pub fn new() -> Self {
        let mut world = World::new();
        let query = world.query::<Cow>();
        world.init_resource::<super::lanes::LaneStats>();
        Self { world, order: Vec::new(), query, pushes: PendingPushes::default(), mutate_order: false, rest: false, dense: false, systems: false, lanes: true, all_free: false, snapshot: Vec::new(), next_id: 1 }
    }
    pub fn len(&self) -> usize {
        self.order.len()
    }
    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    /// A cow of the state `m` had at its creation (`Mob::new`: the constructor's draws, the paths'
    /// maluses and the species' goals are the existing code's, once, at the spawn). No `Mob` is kept.
    pub fn spawn_from(&mut self, m: Mob) -> Entity {
        let p = super::parts::Parts::from_mob(&m, self.rest);
        let e = self
            .world
            .spawn((p.id, p.motion, p.vitals, p.env, p.clk, p.random, p.fluids, p.ai, p.goals, p.extra, p.attrs, p.anim, p.rest))
            .insert(super::lanes::Turn::default())
            .id();
        self.order.push(e);
        e
    }

    /// Every cow's `Seen` in order, and the grid of their boxes (what the pass starts from).
    fn snapshot_all(&mut self) {
        self.snapshot.clear();
        for &e in &self.order {
            let item = self.query.get_mut(&mut self.world, e).expect("a cow of the order");
            self.snapshot.push(item.seen());
        }
    }

    /// One pass over the cows (`tick_borrowed_creatures` for a world of cows): each cow takes the pushes
    /// waiting for it, ticks, gives its pushes, and refreshes its row of the snapshot. Returns the
    /// events of the pass (the breedings), and removes the cows that left.
    pub fn pass(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>) -> Vec<CowEvent> {
        self.snapshot_all();
        let mut grid = crate::mobs::seen_grid::SeenGrid::new(self.snapshot.iter().map(|s| s.bounds));
        let mut events: Vec<CowEvent> = Vec::new();
        let mut keep = vec![true; self.order.len()];
        let slots: Vec<usize> = if self.mutate_order {
            (0..self.order.len()).rev().collect()
        } else {
            (0..self.order.len()).collect()
        };
        if self.systems {
            let order = self.order.clone();
            super::lanes::run_pass(
                &mut self.world,
                w,
                level,
                &mut events,
                &mut self.snapshot,
                &mut grid,
                &mut self.pushes,
                &mut keep,
                &order,
                self.lanes,
                self.all_free,
            );
        } else if self.dense {
            // Indicative only: the table's order, not Horizon's (equal while nothing is removed).
            for (slot, mut item) in self.query.iter_mut(&mut self.world).enumerate() {
                visit(&mut item, slot, &mut self.pushes, &mut self.snapshot, &mut grid, w, level, &mut events, &mut keep);
            }
        }
        for slot in slots.into_iter().filter(|_| !self.dense && !self.systems) {
            let e = self.order[slot];
            let mut item = self.query.get_mut(&mut self.world, e).expect("a cow of the order");
            visit(&mut item, slot, &mut self.pushes, &mut self.snapshot, &mut grid, w, level, &mut events, &mut keep);
        }
        // The cows that left the level.
        let mut slot = 0;
        let mut gone = Vec::new();
        self.order.retain(|e| {
            let k = keep[slot];
            slot += 1;
            if !k {
                gone.push(*e);
            }
            k
        });
        for e in gone {
            self.world.despawn(e);
        }
        events
    }

    /// The breedings of a pass, in the order the goals raised them: the child of each is a new cow
    /// at the end of the order (`Game::breed`, `insert_spawned_mob`). `make_child` builds the child
    /// as the game does (`create_mob_reason` and the inheritance); no experience orb is made.
    pub fn handle(&mut self, events: Vec<CowEvent>, mut make_child: impl FnMut(&CowItemFacts) -> Option<Mob>) {
        for event in events {
            let CowEvent::Breed { parent, partner, parent_variant, .. } = event else {
                continue;
            };
            let Some(p) = self.facts(parent) else { continue };
            let q = self.facts(partner);
            // `finalize_breeding`: the partner's age and love.
            if let Some(q) = &q {
                let mut item = self.query.get_mut(&mut self.world, q.entity).expect("the partner");
                item.extra.age = 6000;
                item.extra.in_love = 0;
            }
            let mut p = p;
            p.parent_variant = parent_variant;
            p.partner_variant = q.as_ref().map(|q| q.variant);
            if let Some(child) = make_child(&p) {
                self.spawn_from(child);
            }
        }
    }

    /// What the breeding reads of a cow.
    pub fn facts(&mut self, id: u32) -> Option<CowItemFacts> {
        let &e = self.order.iter().find(|&&e| self.world.get::<Ident>(e).is_some_and(|i| i.id == id))?;
        let item = self.query.get_mut(&mut self.world, e).ok()?;
        Some(CowItemFacts {
            entity: e,
            id,
            kind: item.id.kind.clone(),
            position: item.motion.position,
            variant: item.extra.variant,
            parent_variant: false,
            partner_variant: None,
        })
    }
}

/// The facts of a parent a child's creation reads.
#[derive(Clone)]
pub struct CowItemFacts {
    pub entity: Entity,
    pub id: u32,
    pub kind: std::sync::Arc<crate::mobs::MobType>,
    pub position: horizon_core::geometry::Vec3,
    pub variant: u8,
    pub parent_variant: bool,
    pub partner_variant: Option<u8>,
}
#[allow(unused)]
fn _use(_: &CowItem<'_, '_>) {}

impl CowWorld {
    pub fn damage(&mut self, e: Entity, level: &mut DamageLevel<'_>, amount: f32, source: crate::mobs::Damage) -> bool {
        let mut item = self.query.get_mut(&mut self.world, e).expect("a cow");
        item.damage(level, amount, source)
    }
    /// (asks, hits) of every cow's rest memo.
    pub fn rest_counts(&mut self) -> (u64, u64) {
        let mut q = self.world.query::<&RestC>();
        q.iter(&self.world).fold((0, 0), |(a, h), r| (a + r.asks, h + r.hits))
    }
    /// (free, coupled) cow-passes since the start.
    pub fn lane_counts(&self) -> (u64, u64) {
        let s = self.world.resource::<super::lanes::LaneStats>();
        (s.free, s.coupled)
    }
    pub fn clear_goals(&mut self, e: Entity) {
        let mut item = self.query.get_mut(&mut self.world, e).expect("a cow");
        item.goals.0.clear();
    }
    #[cfg(test)]
    pub fn seen_of(&mut self, slot: usize) -> Seen {
        let e = self.order[slot];
        self.query.get_mut(&mut self.world, e).expect("a cow").seen()
    }
    pub fn set_in_love(&mut self, e: Entity, ticks: i32) {
        let mut item = self.query.get_mut(&mut self.world, e).expect("a cow");
        item.extra.in_love = ticks;
    }
    /// The fields of the ECS cow in `slot` that differ from `m`, and the snapshot row.
    #[cfg(test)]
    pub fn differences(&mut self, slot: usize, m: &Mob) -> Vec<String> {
        let e = self.order[slot];
        let item = self.query.get_mut(&mut self.world, e).expect("a cow");
        let mut out = super::compare::differences(m, &item);
        // The goals, by their running names, and the snapshot the others read of it.
        let mob_goals = m.ai.goals.running_names();
        let ecs_goals = item.goals.running_names();
        if mob_goals != ecs_goals {
            out.push(format!("goals running: {mob_goals:?} / {ecs_goals:?}"));
        }
        if format!("{:?}", m.random) != format!("{:?}", item.random.0) {
            out.push(format!("random: {:?} / {:?}", m.random, item.random.0));
        }
        out
    }
}

/// One cow's turn of a pass (shared by the ordered pass and the dense one).
#[allow(clippy::too_many_arguments)]
pub(super) fn visit(
    item: &mut CowItem<'_, '_>,
    slot: usize,
    pushes: &mut PendingPushes,
    snapshot: &mut [Seen],
    grid: &mut crate::mobs::seen_grid::SeenGrid,
    w: &MobWorld,
    level: &mut DamageLevel<'_>,
    events: &mut Vec<CowEvent>,
    keep: &mut [bool],
) {
        {
            // `apply_mob_pushes`: the pushes waiting for this cow join its velocity.
            let id = item.id.id;
            let mut add = |v: horizon_core::geometry::Vec3| {
                let u = item.motion.velocity;
                item.motion.velocity = horizon_core::geometry::Vec3::new(u.x + v.x, u.y + v.y, u.z + v.z);
                item.motion.needs_sync = true;
            };
            if !pushes.list.is_empty() {
                pushes.list.retain(|(target, v)| {
                    if *target != id {
                        return true;
                    }
                    add(*v);
                    false
                });
            }
            if !pushes.by_mob.is_empty()
                && let Some(pushes) = pushes.by_mob.remove(&id)
            {
                pushes.into_iter().for_each(add);
            }
        }
        let from = events.len();
        let view = MobWorld {
            others: &*snapshot,
            others_grid: Some(&*grid),
            ..w.clone()
        };
        let kept = item.tick(&view, level, events);
        // `take_mob_pushes`: the pushes of this cow's tick wait for their targets' turns.
        let mut kept_events = from;
        for i in from..events.len() {
            if let CowEvent::Push { target: Who::Mob(id), velocity } = events[i] {
                pushes.push(id, horizon_core::geometry::Vec3::new(velocity[0], velocity[1], velocity[2]));
            } else {
                events.swap(kept_events, i);
                kept_events += 1;
            }
        }
        events.truncate(kept_events);
        let mut s = item.seen();
        s.alive &= kept;
        s.removed |= !kept;
        grid.update(slot, s.bounds);
        snapshot[slot] = s;
        keep[slot] = kept;
}
