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
    snapshot: Vec<Seen>,
    next_id: u32,
}

impl CowWorld {
    pub fn new() -> Self {
        let mut world = World::new();
        let query = world.query::<Cow>();
        Self { world, order: Vec::new(), query, pushes: PendingPushes::default(), mutate_order: false, snapshot: Vec::new(), next_id: 1 }
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
        let mut goals = CowGoalSelector::default();
        super::goals::register_cow(&mut goals, &m.kind);
        let e = self
            .world
            .spawn((
                Ident { id: m.id, kind: m.kind.clone() },
                Motion {
                    position: m.position,
                    previous: m.previous,
                    velocity: m.velocity,
                    yaw: m.yaw,
                    previous_yaw: m.previous_yaw,
                    body_yaw: m.body_yaw,
                    previous_body_yaw: m.previous_body_yaw,
                    head_yaw: m.head_yaw,
                    previous_head_yaw: m.previous_head_yaw,
                    head_pitch: m.head_pitch,
                    previous_head_pitch: m.previous_head_pitch,
                    on_ground: m.on_ground,
                    horizontal_collision: m.horizontal_collision,
                    vertical_collision_below: m.vertical_collision_below,
                    needs_sync: m.needs_sync,
                    sync_position: m.sync_position,
                    fall_distance: m.fall_distance,
                    stuck_multiplier: m.stuck_multiplier,
                    piston: m.piston.clone(),
                    supporting: m.supporting,
                    climbing: m.climbing,
                    on_climbable: m.on_climbable,
                    no_gravity: m.no_gravity,
                    no_physics: m.no_physics,
                    pushed: m.pushed.clone(),
                    limb_position: m.limb_position,
                    limb_speed: m.limb_speed,
                    previous_limb_speed: m.previous_limb_speed,
                },
                Vitals {
                    health: m.health,
                    hurt_time: m.hurt_time,
                    invulnerable: m.invulnerable,
                    invulnerable_time: m.invulnerable_time,
                    death_time: m.death_time,
                    impulse_grace_time: m.impulse_grace_time,
                    absorption: m.absorption,
                    last_hurt: m.last_hurt,
                    dealt_absorbed: m.dealt_absorbed,
                    last_damage: m.last_damage,
                    accepted_hit: None,
                    death_canceled: m.death_canceled,
                    last_direct: m.last_direct,
                    attacker_aim: None,
                    creative_attacker: m.creative_attacker,
                    projectile_motion: m.projectile_motion,
                    invulnerable_flag: m.invulnerable_flag,
                    combat: m.combat.clone(),
                    removed: m.removed,
                    despawn: m.despawn,
                    gear_knockback_resistance: m.gear_knockback_resistance,
                },
                Env {
                    fire_ticks: m.fire_ticks,
                    ticks_frozen: m.ticks_frozen,
                    in_powder_snow: m.in_powder_snow,
                    air: m.air,
                    first_tick: m.first_tick,
                    was_eye_in_water: m.was_eye_in_water,
                },
                Clk {
                    age: m.age,
                    ambient_timer: m.ambient_timer,
                    persistent: m.persistent,
                    no_ai: m.no_ai,
                    silent: m.silent,
                    swell: m.swell,
                    previous_swell: m.previous_swell,
                    max_swell: m.max_swell,
                },
                Rand(m.random.clone()),
                FluidsC(m.fluids),
                AiC(*m.ai.clone()),
                GoalsC(goals),
                ExtraC(super::extra::CowExtra {
                    age: m.extra.age,
                    in_love: m.extra.in_love,
                    forced_age_timer: m.extra.forced_age_timer,
                    age_locked: m.extra.age_locked,
                    variant: m.extra.variant,
                }),
                AttrsC(m.attrs.clone()),
                AnimC(m.anim.clone()),
            ))
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
        for slot in slots {
            let e = self.order[slot];
            let mut item = self.query.get_mut(&mut self.world, e).expect("a cow of the order");
            {
                // `apply_mob_pushes`: the pushes waiting for this cow join its velocity.
                let id = item.id.id;
                let mut add = |v: horizon_core::geometry::Vec3| {
                    let u = item.motion.velocity;
                    item.motion.velocity = horizon_core::geometry::Vec3::new(u.x + v.x, u.y + v.y, u.z + v.z);
                    item.motion.needs_sync = true;
                };
                if !self.pushes.list.is_empty() {
                    self.pushes.list.retain(|(target, v)| {
                        if *target != id {
                            return true;
                        }
                        add(*v);
                        false
                    });
                }
                if !self.pushes.by_mob.is_empty()
                    && let Some(pushes) = self.pushes.by_mob.remove(&id)
                {
                    pushes.into_iter().for_each(add);
                }
            }
            let from = events.len();
            let view = MobWorld {
                others: &self.snapshot,
                others_grid: Some(&grid),
                ..w.clone()
            };
            let kept = item.tick(&view, level, &mut events);
            // `take_mob_pushes`: the pushes of this cow's tick wait for their targets' turns.
            let mut kept_events = from;
            for i in from..events.len() {
                if let CowEvent::Push { target: Who::Mob(id), velocity } = events[i] {
                    self.pushes.push(id, horizon_core::geometry::Vec3::new(velocity[0], velocity[1], velocity[2]));
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
            self.snapshot[slot] = s;
            keep[slot] = kept;
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
