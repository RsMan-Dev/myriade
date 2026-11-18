//! `GoalSelector` and the eight goals of a cow (`mobs/goals.rs`, `mobs/farm.rs`): Float, Panic, Breed,
//! Tempt, FollowParent, Stroll (water avoiding), LookAtPlayer and RandomLookAround. A goal that
//! reads a player (Tempt, LookAtPlayer) is ported without that branch: the ECS world has no player.
use super::{event::CowEvent, view::CowItem};
use crate::mobs::{MobWorld, Species, Who};
use horizon_core::{
    geometry::{Aabb, Vec3},
    jmath,
    random::RandomSource,
};

pub const MOVE: u8 = 1;
pub const LOOK: u8 = 2;
pub const JUMP: u8 = 4;

pub struct Cx<'a, 'w, 'l> {
    pub w: &'a MobWorld<'w>,
    pub level: &'a mut crate::health::DamageLevel<'l>,
    pub events: &'a mut Vec<CowEvent>,
}
pub trait CowGoal: std::fmt::Debug + Send + Sync {
    fn flags(&self) -> u8;
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool;
    fn can_continue(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool {
        self.can_use(m, cx)
    }
    fn interruptable(&self) -> bool {
        true
    }
    fn start(&mut self, _m: &mut CowItem<'_, '_>, _cx: &mut Cx) {}
    fn stop(&mut self, _m: &mut CowItem<'_, '_>, _cx: &mut Cx) {}
    fn every_tick(&self) -> bool {
        false
    }
    fn tick(&mut self, _m: &mut CowItem<'_, '_>, _cx: &mut Cx) {}
    fn box_clone(&self) -> Box<dyn CowGoal>;
    fn name(&self) -> &'static str;
}
impl Clone for Box<dyn CowGoal> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}
/// `Goal.reducedTickDelay`
pub fn reduced(ticks: i32) -> i32 {
    (ticks + 1) / 2
}

#[derive(Clone, Debug)]
struct Wrapped {
    priority: i32,
    goal: Box<dyn CowGoal>,
    running: bool,
}
#[derive(Clone, Debug, Default)]
pub struct CowGoalSelector {
    goals: Vec<Wrapped>,
    locked: [Option<usize>; 4],
    pub disabled: u8,
}
impl CowGoalSelector {
    pub fn add(&mut self, priority: i32, goal: impl CowGoal + 'static) {
        self.goals.push(Wrapped { priority, goal: Box::new(goal), running: false });
    }
    pub fn clear(&mut self) {
        self.goals.clear();
        self.locked = [None; 4];
    }
    pub fn is_running(&self, name: &str) -> bool {
        self.goals.iter().any(|g| g.running && g.goal.name() == name)
    }
    pub fn running_names(&self) -> Vec<&'static str> {
        self.goals.iter().filter(|g| g.running).map(|g| g.goal.name()).collect()
    }
    fn stop(&mut self, i: usize, m: &mut CowItem<'_, '_>, cx: &mut Cx) {
        if self.goals[i].running {
            self.goals[i].running = false;
            self.goals[i].goal.stop(m, cx);
        }
    }
    /// `GoalSelector.tick`
    pub fn tick(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) {
        for i in 0..self.goals.len() {
            if self.goals[i].running
                && (self.goals[i].goal.flags() & self.disabled != 0 || !self.goals[i].goal.can_continue(m, cx))
            {
                self.stop(i, m, cx);
            }
        }
        for slot in &mut self.locked {
            if slot.is_some_and(|i| !self.goals[i].running) {
                *slot = None;
            }
        }
        for i in 0..self.goals.len() {
            let flags = self.goals[i].goal.flags();
            if self.goals[i].running || flags & self.disabled != 0 {
                continue;
            }
            let replaceable = (0..4).filter(|b| flags & (1 << b) != 0).all(|b| match self.locked[b] {
                None => true,
                Some(j) => self.goals[j].goal.interruptable() && self.goals[i].priority < self.goals[j].priority,
            });
            if !replaceable || !self.goals[i].goal.can_use(m, cx) {
                continue;
            }
            for b in 0..4 {
                if flags & (1 << b) != 0 {
                    if let Some(j) = self.locked[b] {
                        self.stop(j, m, cx);
                    }
                    self.locked[b] = Some(i);
                }
            }
            if !self.goals[i].running {
                self.goals[i].running = true;
                self.goals[i].goal.start(m, cx);
            }
        }
        self.tick_running(m, cx, true);
    }
    pub fn tick_running(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx, force: bool) {
        for g in &mut self.goals {
            if g.running && (force || g.goal.every_tick()) {
                g.goal.tick(m, cx);
            }
        }
    }
}

macro_rules! clone_goal {
    ($name:expr) => {
        fn box_clone(&self) -> Box<dyn CowGoal> {
            Box::new(self.clone())
        }
        fn name(&self) -> &'static str {
            $name
        }
    };
}

/// `FarmAnimal.registerGoals` of a cow (`farm::register`).
pub fn register_cow(goals: &mut CowGoalSelector, kind: &crate::mobs::MobType) {
    goals.add(0, Float);
    goals.add(1, Panic { speed: 2.0, pos: [0.0; 3] });
    goals.add(2, Breed { speed: 1.0, partner: None, love_time: 0 });
    goals.add(3, Tempt { calm_down: 0 });
    goals.add(4, FollowParent { speed: 1.25, parent: None, recalc: 0 });
    goals.add(5, Stroll { speed: 1.0, interval: 120, wanted: [0.0; 3] });
    goals.add(6, LookAtPlayer { probability: 0.02 });
    goals.add(7, RandomLookAround { rel: [0.0; 2], time: 0 });
    let _ = kind;
}

#[derive(Clone, Debug)]
pub struct Float;
impl CowGoal for Float {
    clone_goal!("Float");
    fn flags(&self) -> u8 {
        JUMP
    }
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) -> bool {
        m.floats()
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) {
        if m.random.next_float() < 0.8 {
            m.ai.jump = true;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Panic {
    speed: f64,
    pos: [f64; 3],
}
impl CowGoal for Panic {
    clone_goal!("Panic");
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool {
        let burning = m.env.fire_ticks > 0 && !m.id.kind.flags.fire_immune;
        let wanted = m
            .vitals
            .last_damage
            .is_some_and(|(d, t)| d.causes_panic() && i64::from(m.clk.age) - t <= 40)
            || burning;
        if !wanted {
            return false;
        }
        if let Some(range) = (m.env.fire_ticks > 0).then_some(5)
            && let Some(p) = m.water_pos(cx.w, range)
        {
            self.pos = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
            return true;
        }
        match m.default_random_pos(cx.w, 5, 4) {
            Some(p) => {
                self.pos = p;
                true
            }
            None => false,
        }
    }
    fn can_continue(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) -> bool {
        !m.ai.nav.is_done()
    }
    fn start(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) {
        m.nav_move_to(cx.w, self.pos, self.speed);
        m.ai.panicking = true;
    }
    fn stop(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) {
        m.ai.panicking = false;
    }
}

impl CowItem<'_, '_> {
    /// `PanicGoal.lookForWater`.
    fn water_pos(&self, w: &MobWorld, range: i32) -> Option<[i32; 3]> {
        let (bx, by, bz) = (
            self.motion.position.x.floor() as i32,
            self.motion.position.y.floor() as i32,
            self.motion.position.z.floor() as i32,
        );
        if w.data.state(w.world.block(bx, by, bz)).top != 0.0 {
            return None;
        }
        w.nearest_block([bx, by, bz], range, 1, |p, _| {
            w.fluid(p[0], p[1], p[2]) == crate::blocks::Fluid::Water
        })
    }
}

#[derive(Clone, Debug)]
pub struct Stroll {
    speed: f64,
    interval: i32,
    wanted: [f64; 3],
}
impl CowGoal for Stroll {
    clone_goal!("Stroll");
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool {
        if m.ai.no_action_time >= 100 {
            return false;
        }
        if m.random.next_int_bound(reduced(self.interval)).unwrap_or(1) != 0 {
            return false;
        }
        let prob = 0.001_f32;
        let pos = if m.fluids.in_water {
            m.land_random_pos(cx.w, 15, 7).or_else(|| m.default_random_pos(cx.w, 10, 7))
        } else if m.random.next_float() >= prob {
            m.land_random_pos(cx.w, 10, 7)
        } else {
            m.default_random_pos(cx.w, 10, 7)
        };
        let Some(p) = pos else { return false };
        self.wanted = p;
        true
    }
    fn can_continue(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) -> bool {
        !m.ai.nav.is_done()
    }
    fn start(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) {
        m.nav_move_to(cx.w, self.wanted, self.speed);
    }
    fn stop(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) {
        m.nav_stop();
    }
}

/// `LookAtPlayerGoal`: the draw of its probability, then no player to look at.
#[derive(Clone, Debug)]
pub struct LookAtPlayer {
    probability: f32,
}
impl CowGoal for LookAtPlayer {
    clone_goal!("LookAtPlayer");
    fn flags(&self) -> u8 {
        LOOK
    }
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) -> bool {
        if m.random.next_float() >= self.probability {
            return false;
        }
        false
    }
}

/// `TemptGoal`: counts its calm-down; there is no player holding a food item.
#[derive(Clone, Debug)]
pub struct Tempt {
    calm_down: i32,
}
impl CowGoal for Tempt {
    clone_goal!("Tempt");
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, _: &mut CowItem<'_, '_>, _: &mut Cx) -> bool {
        if self.calm_down > 0 {
            self.calm_down -= 1;
            return false;
        }
        false
    }
}

#[derive(Clone, Debug)]
pub struct RandomLookAround {
    rel: [f64; 2],
    time: i32,
}
impl CowGoal for RandomLookAround {
    clone_goal!("RandomLookAround");
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) -> bool {
        m.random.next_float() < 0.02
    }
    fn can_continue(&mut self, _: &mut CowItem<'_, '_>, _: &mut Cx) -> bool {
        self.time >= 0
    }
    fn start(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) {
        let r = std::f64::consts::TAU * m.random.next_double();
        self.rel = [jmath::cos(r), jmath::sin(r)];
        self.time = 20 + m.random.next_int_bound(20).unwrap_or(0);
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut CowItem<'_, '_>, _: &mut Cx) {
        self.time -= 1;
        let at = [m.motion.position.x + self.rel[0], m.eye_y(), m.motion.position.z + self.rel[1]];
        m.look_at(at);
    }
}

#[derive(Clone, Debug)]
pub struct FollowParent {
    speed: f64,
    parent: Option<u32>,
    recalc: i32,
}
impl CowGoal for FollowParent {
    clone_goal!("FollowParent");
    fn flags(&self) -> u8 {
        0
    }
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool {
        if m.extra.age >= 0 {
            return false;
        }
        let species = m.species();
        let pos = m.motion.position;
        let mut best: Option<(u32, f64)> = None;
        let area = Aabb::new(
            Vec3::new(pos.x - 9.0, pos.y - 5.0, pos.z - 9.0),
            Vec3::new(pos.x + 9.0, pos.y + 5.0, pos.z + 9.0),
        );
        for s in cx.w.others_near(area).filter(|s| {
            s.alive
                && s.species == species
                && !s.baby
                && (s.position.x - pos.x).abs() <= 8.0
                && (s.position.y - pos.y).abs() <= 4.0
                && (s.position.z - pos.z).abs() <= 8.0
        }) {
            let d = m.distance_sqr(s.position);
            if best.is_none_or(|b| d <= b.1) {
                best = Some((s.id, d));
            }
        }
        match best {
            Some((id, d)) if d >= 9.0 => {
                self.parent = Some(id);
                true
            }
            _ => false,
        }
    }
    fn can_continue(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool {
        if m.extra.age >= 0 {
            return false;
        }
        let Some(p) = self.parent.and_then(|p| cx.w.seen(p)) else {
            return false;
        };
        let d = m.distance_sqr(p.position);
        p.alive && (9.0..=256.0).contains(&d)
    }
    fn start(&mut self, _: &mut CowItem<'_, '_>, _: &mut Cx) {
        self.recalc = 0;
    }
    fn stop(&mut self, _: &mut CowItem<'_, '_>, _: &mut Cx) {
        self.parent = None;
    }
    fn tick(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) {
        self.recalc -= 1;
        if self.recalc <= 0 {
            self.recalc = reduced(10);
            if let Some(p) = self.parent.and_then(|p| cx.w.seen(p)) {
                let at = [p.position.x, p.position.y, p.position.z];
                m.nav_move_to_entity(cx.w, at, 1, self.speed);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Breed {
    speed: f64,
    partner: Option<u32>,
    love_time: i32,
}
impl CowGoal for Breed {
    clone_goal!("Breed");
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool {
        if m.extra.in_love <= 0 {
            return false;
        }
        let species = m.species();
        let id = m.id.id;
        self.partner = cx
            .w
            .nearby(m.motion.position, 8.0, move |s| {
                s.species == species && s.id != id && s.in_love && !s.baby && !s.panicking
            })
            .find(|s| m.distance_sqr(s.position) <= 64.0)
            .map(|s| s.id);
        self.partner.is_some()
    }
    fn can_continue(&mut self, _: &mut CowItem<'_, '_>, cx: &mut Cx) -> bool {
        self.partner
            .and_then(|p| cx.w.seen(p))
            .is_some_and(|s| s.alive && s.in_love && !s.panicking)
            && self.love_time < 60
    }
    fn stop(&mut self, _: &mut CowItem<'_, '_>, _: &mut Cx) {
        self.partner = None;
        self.love_time = 0;
    }
    fn tick(&mut self, m: &mut CowItem<'_, '_>, cx: &mut Cx) {
        let Some(p) = self.partner.and_then(|p| cx.w.seen(p)).cloned() else {
            return;
        };
        let x = m.max_head_x_rot();
        m.look_at_speed([p.position.x, p.eye_y, p.position.z], 10.0, x);
        m.nav_move_to_entity(cx.w, [p.position.x, p.position.y, p.position.z], 1, self.speed);
        self.love_time += 1;
        if self.love_time >= reduced(60) && m.distance_sqr(p.position) < 9.0 {
            // `BreedGoal.breed`: the parent's coin for the child's variant (a cow draws one), then
            // the experience (`breeding::voice_and_experience`): both from the parent's random.
            let parent_variant = m.random.next_boolean();
            let experience = m.breeding_experience(cx.w.mob_drops);
            m.extra.in_love = 0;
            m.extra.age = 6000;
            cx.events.push(CowEvent::Breed { parent: m.id.id, partner: p.id, parent_variant, experience });
        }
    }
}
const _: Option<Who> = None;
const _: Species = Species::Cow;
