//! The witness of the bench: the very same source files of the cow (`physics.rs`, `tick.rs`, `goals.rs`,
//! `nav.rs`...) compiled a second time against `CowItem` as a plain struct of the same components, with
//! no `bevy_ecs`: a `Vec<CowItem>` in the list's order. What it costs against the ECS cow is the storage.
pub use super::{event, extra};
#[path = "climbing.rs"]
pub mod climbing;
#[path = "components.rs"]
pub mod components;
#[path = "compare.rs"]
pub mod compare;
#[path = "control.rs"]
pub mod control;
#[path = "dims.rs"]
pub mod dims;
#[path = "effects.rs"]
pub mod effects;
#[path = "goals.rs"]
pub mod goals;
#[path = "landing.rs"]
pub mod landing;
#[path = "life.rs"]
pub mod life;
#[path = "nav.rs"]
pub mod nav;
#[path = "parts.rs"]
pub mod parts;
#[path = "physics.rs"]
pub mod physics;
#[path = "rest.rs"]
pub mod rest;
#[path = "randpos.rs"]
pub mod randpos;
#[path = "seen.rs"]
pub mod seen;
#[path = "tick.rs"]
pub mod tick;

pub mod view {
    use super::components::*;
    /// One cow: its components side by side, owned.
    pub struct CowItem<'a, 'b> {
        pub id: Ident,
        pub motion: Motion,
        pub vitals: Vitals,
        pub env: Env,
        pub clk: Clk,
        pub random: Rand,
        pub fluids: FluidsC,
        pub ai: AiC,
        pub goals: GoalsC,
        pub extra: ExtraC,
        pub attrs: AttrsC,
        pub anim: AnimC,
        pub rest: RestC,
        pub _lifetimes: std::marker::PhantomData<(&'a (), &'b ())>,
    }
}

use crate::{
    health::DamageLevel,
    mobs::{Mob, MobWorld, PendingPushes, Seen, Who},
};
use event::CowEvent;
use view::CowItem;

/// The plain world: the cows in a vector, in order.
#[derive(Default)]
pub struct PlainWorld {
    pub cows: Vec<CowItem<'static, 'static>>,
    pub pushes: PendingPushes,
    pub rest: bool,
    snapshot: Vec<Seen>,
}

impl PlainWorld {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.cows.len()
    }
    pub fn spawn_from(&mut self, m: Mob) {
        let p = parts::Parts::from_mob(&m, self.rest);
        self.cows.push(CowItem {
            id: p.id,
            motion: p.motion,
            vitals: p.vitals,
            env: p.env,
            clk: p.clk,
            random: p.random,
            fluids: p.fluids,
            ai: p.ai,
            goals: p.goals,
            extra: p.extra,
            attrs: p.attrs,
            anim: p.anim,
            rest: p.rest,
            _lifetimes: std::marker::PhantomData,
        });
    }
    pub fn rest_counts(&self) -> (u64, u64) {
        self.cows.iter().fold((0, 0), |(a, h), c| (a + c.rest.asks, h + c.rest.hits))
    }
    pub fn clear_goals(&mut self, slot: usize) {
        self.cows[slot].goals.0.clear();
    }
    /// `CowWorld::pass` on the plain vector.
    pub fn pass(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>) -> Vec<CowEvent> {
        self.snapshot.clear();
        for c in &self.cows {
            self.snapshot.push(c.seen());
        }
        let mut grid = crate::mobs::seen_grid::SeenGrid::new(self.snapshot.iter().map(|s| s.bounds));
        let mut events: Vec<CowEvent> = Vec::new();
        let mut keep = vec![true; self.cows.len()];
        for slot in 0..self.cows.len() {
            let item = &mut self.cows[slot];
            {
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
            let view = MobWorld { others: &self.snapshot, others_grid: Some(&grid), others_solid: Some(&[]), ..w.clone() };
            let kept = item.tick(&view, level, &mut events);
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
        let mut slot = 0;
        self.cows.retain(|_| {
            let k = keep[slot];
            slot += 1;
            k
        });
        events
    }
    pub fn damage(&mut self, slot: usize, level: &mut DamageLevel<'_>, amount: f32, source: crate::mobs::Damage) -> bool {
        self.cows[slot].damage(level, amount, source)
    }
    pub fn set_in_love(&mut self, slot: usize, ticks: i32) {
        self.cows[slot].extra.in_love = ticks;
    }
    #[cfg(test)]
    pub fn differences(&self, slot: usize, m: &Mob) -> Vec<String> {
        let item = &self.cows[slot];
        let mut out = compare::differences(m, item);
        let (a, b) = (m.ai.goals.running_names(), item.goals.0.running_names());
        if a != b {
            out.push(format!("goals running: {a:?} / {b:?}"));
        }
        if format!("{:?}", m.random) != format!("{:?}", item.random.0) {
            out.push("random".into());
        }
        out
    }
    #[cfg(test)]
    pub fn seen_of(&self, slot: usize) -> Seen {
        self.cows[slot].seen()
    }
    /// The facts a child's creation reads of the parent `id`.
    pub fn facts(&self, id: u32) -> Option<(std::sync::Arc<crate::mobs::MobType>, horizon_core::geometry::Vec3, u8)> {
        self.cows.iter().find(|c| c.id.id == id).map(|c| (c.id.kind.clone(), c.motion.position, c.extra.variant))
    }
    pub fn set_partner(&mut self, id: u32) {
        if let Some(c) = self.cows.iter_mut().find(|c| c.id.id == id) {
            c.extra.age = 6000;
            c.extra.in_love = 0;
        }
    }
}
