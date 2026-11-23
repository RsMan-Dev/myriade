//! Frogs (`Frog`, `FrogAi`): the `Croak`, `ShootTongue` and long jump
//! behaviours as goals, the poses they set and the client's animation states.
//! AnimState states: jump, croak, tongue, swim idle.
//!
//! The brain's idle `RunOne` picks a croak three times as often as a stroll
//! (weights 3 and 1; it also holds a do-nothing option that only repeats
//! the pick), so a frog that is not walking croaks in three of four idle
//! picks: `Croak` takes those, and the frog's stroll then follows at once.
use super::{
    DamageLevel, MobEvent, MobWorld, Species, Who,
    goals::{
        AnimalPanic, Breed, Cx, Float, Goal, LOOK, LookAtPlayer, MOVE, RandomLookAround, Stroll,
        Tempt, goal_clone,
    },
    sync,
};
use crate::blocks::Fluid;
use horizon_core::{
    geometry::Vec3,
    random::{LevelRandom, RandomSource},
};
use horizon_world::move_gate::MoveGate;
use myriade_types::SoundSource;
use std::fmt::Debug;

const JUMP: usize = 0;
const CROAK: usize = 1;
const TONGUE: usize = 2;
const SWIM_IDLE: usize = 3;

/// `Croak.CROAK_TICKS`.
const CROAK_TICKS: i32 = 60;
/// `ShootTongue`'s duration and eating distance.
const TONGUE_TIME_OUT: i32 = 100;
const EATING_DISTANCE: f64 = 1.75;
/// `FrogAttackablesSensor.TARGET_DETECTION_DISTANCE` and the sensor's scan
/// rate.
const DETECTION_DISTANCE: f64 = 10.0;
const SCAN_RATE: i32 = 20;

/// `FrogAi.TIME_BETWEEN_LONG_JUMPS` and the brain memory
/// `LONG_JUMP_COOLDOWN_TICKS` (saved with the others, as `finalize` sets it).
const TIME_BETWEEN_LONG_JUMPS: (i32, i32) = (100, 140);
/// `GoatAi.TIME_BETWEEN_LONG_JUMPS`.
const GOAT_TIME_BETWEEN_LONG_JUMPS: (i32, i32) = (600, 1200);
const LONG_JUMP_COOLDOWN: &str = "long_jump_cooling_down";
/// `FrogAi`'s `LongJumpToPreferredBlock`: `MAX_LONG_JUMP_WIDTH`,
/// `MAX_LONG_JUMP_HEIGHT`, `MAX_JUMP_VELOCITY_MULTIPLIER` and the chance to
/// look for a lily pad or big dripleaf first.
const JUMP_WIDTH: i32 = 4;
const JUMP_HEIGHT: i32 = 2;
/// The goat's `MAX_JUMP_VELOCITY_MULTIPLIER` is the same number.
const JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;
const PREFERRED_BLOCKS_CHANCE: f32 = 0.5;
/// `GoatAi`'s `LongJumpToRandomPos`: `MAX_LONG_JUMP_WIDTH` and
/// `MAX_LONG_JUMP_HEIGHT`.
const GOAT_JUMP_WIDTH: i32 = 5;
const GOAT_JUMP_HEIGHT: i32 = 5;
/// `LongJumpToRandomPos.ALLOWED_ANGLES`, `FIND_JUMP_TRIES` and
/// `PREPARE_JUMP_DURATION`.
const ALLOWED_ANGLES: [i32; 4] = [65, 70, 75, 80];
const FIND_JUMP_TRIES: i32 = 20;
const PREPARE_JUMP_DURATION: i32 = 40;
/// `LongJumpMidJump`'s duration (`TIME_OUT_DURATION`).
const MID_JUMP_DURATION: i32 = 100;

/// `ShootTongue.State`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    MoveToTarget,
    CatchAnimation,
    EatAnimation,
    #[default]
    Done,
}

/// The frog's brain memories and `DATA_TONGUE_TARGET_ID`.
#[derive(Clone, Debug, Default)]
pub struct State {
    croak: i32,
    /// The land beside water `LaySpawn` walks to (its `WALK_TARGET`).
    lay_target: Option<[i32; 3]>,
    /// Ticks to the attackables sensor's next scan, and `NEAREST_ATTACKABLE`.
    scan: i32,
    nearest: Option<u32>,
    /// `UNREACHABLE_TONGUE_TARGETS`: the mob and the ticks it stays in mind.
    unreachable: Vec<(u32, i32)>,
    /// `ATTACK_TARGET` while `ShootTongue` runs, and the synched tongue target.
    target: Option<u32>,
    pub tongue_target: Option<u32>,
    phase: Phase,
    eat_timer: i32,
    path_counter: i32,
    time: i32,
    /// `LivingEntity.discardFriction`, set through the long jump.
    pub discard_friction: bool,
}





fn can_eat(species: Species, size: i32) -> bool {
    matches!(species, Species::Slime | Species::MagmaCube) && size == 1
}


/// `Croak`: the pose sync::CROAKING for 60 ticks (the croak is the frog's ambient
/// sound).
#[derive(Clone, Debug)]
struct Croak;
impl Goal for Croak {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn every_tick(&self) -> bool {
        true
    }
}

/// `IS_PREGNANT`, set by `Frog.spawnChildFromBreeding` and erased once the
/// frog has laid its frogspawn.
const PREGNANT: &str = "is_pregnant";

impl<G: Debug> Debug for Unless<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.1.fmt(f)
    }
}
impl<G: Goal + Clone + 'static> Goal for Unless<G> {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.1.flags()
    }
    fn name(&self) -> &'static str {
        self.1.name()
    }
    fn interruptable(&self) -> bool {
        self.1.interruptable()
    }
    fn every_tick(&self) -> bool {
        self.1.every_tick()
    }
}

/// `FrogAi.initLaySpawnActivity` for a pregnant frog: `TryFindLandNearLiquid(8, 1.0)`
/// looks for land beside water and walks there, and `TryLaySpawnOnFluidNearLand`
/// puts frogspawn on the water next to it. Both run every tick (the brain's
/// walk target is `lay_target` here); the wandering `RunOne` is the goals below it.
#[derive(Clone, Debug, Default)]
struct LaySpawn {
    /// `nextOkStartTime`: the search runs again once it has passed; every tick the
    /// behaviour is tried before that pushes it 40 ticks on.
    next_ok: i64,
}
impl LaySpawn {
    fn is_air(w: &MobWorld, p: [i32; 3]) -> bool {
        w.facts.is_air(w.world.block(p[0], p[1], p[2]))
    }
    fn empty(w: &MobWorld, p: [i32; 3]) -> bool {
        w.collision_bounds(p).is_none()
    }
    /// `TryFindLandNearLiquid`'s `findFirst` test.
    fn land_near_water(w: &MobWorld, p: [i32; 3]) -> bool {
        if !Self::empty(w, p) || Self::empty(w, [p[0], p[1] - 1, p[2]]) {
            return false;
        }
        HORIZONTAL.iter().any(|d| {
            let n = [p[0] + d[0], p[1], p[2] + d[1]];
            Self::is_air(w, n) && w.fluid(n[0], n[1] - 1, n[2]) == Fluid::Water
        })
    }
}
impl LaySpawn {
}
impl Goal for LaySpawn {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// `Direction.Plane.HORIZONTAL` as x and z steps: north, east, south, west.
const HORIZONTAL: [[i32; 2]; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
/// `BlockPos.withinManhattan`: nearest first, then x, y, and +z before -z.
fn manhattan_box(o: [i32; 3], range: i32) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for depth in 0..=range * 3 {
        let max_x = range.min(depth);
        for x in -max_x..=max_x {
            let max_y = range.min(depth - x.abs());
            for y in -max_y..=max_y {
                let z = depth - x.abs() - y.abs();
                if z <= range {
                    out.push([o[0] + x, o[1] + y, o[2] + z]);
                    if z != 0 {
                        out.push([o[0] + x, o[1] + y, o[2] - z]);
                    }
                }
            }
        }
    }
    out
}

/// `ShootTongue` with the tongue activity's `StartAttacking` and
/// `StopAttackingIfTargetInvalid`: walk up to a small slime or magma cube,
/// shoot the tongue, pull it in and eat it.
#[derive(Clone, Debug)]
struct ShootTongue;
impl Goal for ShootTongue {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}





/// `LongJumpToRandomPos.PossibleJump`: a block and its weight.
type Candidate = ([i32; 3], i32);

/// `FrogAi`'s long jump activity: `LongJumpToPreferredBlock` looks for a spot
/// it cannot walk to, faces it for 40 ticks and leaps; `LongJumpMidJump`
/// holds the pose until it lands. One goal here, like the brain activity
/// that keeps the idle croak and stroll from starting meanwhile.
#[derive(Clone, Debug, Default)]
pub(super) struct LongJump {
    candidates: Vec<Candidate>,
    not_preferred: Vec<Candidate>,
    wanting_preferred: bool,
    initial: Option<Vec3>,
    chosen: Option<Vec3>,
    /// The chosen spot, looked at while preparing.
    look: [f64; 3],
    tries: i32,
    /// Ticks since the jump was chosen.
    prepare: i32,
    /// `LONG_JUMP_MID_JUMP`, and the ticks `LongJumpMidJump` has run.
    mid_jump: bool,
    in_air: Option<i32>,
}
impl LongJump {
    /// `WeightedRandom.getRandomItem` with the level's random over the
    /// candidates, removed once drawn.
    fn draw(&mut self, lr: &mut LevelRandom) -> Option<Candidate> {
        let total: i32 = self.candidates.iter().map(|c| c.1).sum();
        if total == 0 {
            return None;
        }
        let mut pick = lr.next_int_bound(total).unwrap_or(0);
        let i = self.candidates.iter().position(|c| {
            pick -= c.1;
            pick < 0
        })?;
        Some(self.candidates.remove(i))
    }
    /// `LongJumpToPreferredBlock.getJumpCandidate`: while it wants them, the
    /// first drawn spot over a preferred block, else the first one passed over.
    fn candidate(&mut self, w: &MobWorld, lr: &mut LevelRandom) -> Option<Candidate> {
        if !self.wanting_preferred {
            return self.draw(lr);
        }
        while !self.candidates.is_empty() {
            if let Some(c) = self.draw(lr) {
                let [x, y, z] = c.0;
                if w.has_tag("minecraft:frog_prefer_jump_to", w.block_name(x, y - 1, z)) {
                    return Some(c);
                }
                self.not_preferred.push(c);
            }
        }
        (!self.not_preferred.is_empty()).then(|| self.not_preferred.remove(0))
    }
}
/// `LongJumpToRandomPos.start`'s candidates: every block of the box but the
/// mob's own, weighted by the squared distance, in `betweenClosed` order.
fn candidates([x, y, z]: [i32; 3], width: i32, height: i32) -> Vec<Candidate> {
    let mut out = Vec::new();
    for dz in -width..=width {
        for dy in -height..=height {
            for dx in -width..=width {
                if (dx, dy, dz) != (0, 0, 0) {
                    out.push(([x + dx, y + dy, z + dz], dx * dx + dy * dy + dz * dz));
                }
            }
        }
    }
    out
}
impl Goal for LongJump {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn interruptable(&self) -> bool {
        !self.mid_jump
    }
    fn every_tick(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::random::LegacyRandom;



    /// `Frog.travelInWater`: the move uses `getSpeed` (the speed the move
    /// control stored, here 0.25, not the movement attribute 1.0 and not the
    /// turtle's 0.1), then the whole motion is scaled by 0.9. Out of water
    /// this path does not run.
    #[test]
    #[ignore = "daily"]
    fn water_travel_uses_get_speed_and_scales_by_nine_tenths() {
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            let mut m = frog();
            m.attrs.speed = 1.0;
            m.ai.speed = 0.25;
            m.yaw = 0.0;
            m.velocity = Vec3::new(1.0, -0.5, 0.125);
            let kept = m.velocity;
            let mut test_level = crate::health::TestLevel::new(0);
            assert!(!travel(
                &mut m,
                w,
                &mut test_level.level(),
                Vec3::new(0.0, 0.0, 1.0),
                &mut Vec::new()
            ));
            assert_eq!(m.velocity, kept);

            m.fluids.in_water = true;
            m.position = Vec3::new(0.5, 80.0, 0.5);
            m.travel(
                w,
                &mut test_level.level(),
                Vec3::new(0.0, 0.0, 1.0),
                &mut Vec::new(),
            );
            // (1, -0.5, 0.125) + (0, 0, getSpeed 0.25) = (1, -0.5, 0.375),
            // then * 0.9 = (0.9, -0.45, 0.3375).
            assert!((m.velocity.x - 0.9).abs() < 1.0e-9, "{}", m.velocity.x);
            assert!((m.velocity.y - -0.45).abs() < 1.0e-9, "{}", m.velocity.y);
            assert!((m.velocity.z - 0.3375).abs() < 1.0e-9, "{}", m.velocity.z);
        });
        // Without a test world the closure does not run; there is nothing else to check.
        assert!(ran, "vanilla data is missing");
    }

    /// `Frog.updateWalkAnimation`: target `min(distance * 25, 1)`, 0 while
    /// the jump animation is started, eased by 0.4 (`WalkAnimationState.update`).
    #[test]
    fn a_frog_walks_its_legs_at_the_vanilla_rate() {
        let mut m = frog();
        m.previous = m.position;
        m.position.x += 0.02;
        m.update_walk_animation();
        assert!((m.limb_speed - 0.5 * 0.4).abs() < 1e-6, "{}", m.limb_speed);
        assert!((m.limb_position - m.limb_speed).abs() < 1e-6);

        let mut m = frog();
        m.previous = m.position;
        m.position.x += 0.1;
        m.update_walk_animation();
        assert!((m.limb_speed - 0.4).abs() < 1e-6, "{}", m.limb_speed);

        let mut m = frog();
        m.anim.states[JUMP].start(0.0);
        m.previous = m.position;
        m.position.x += 0.02;
        m.update_walk_animation();
        assert_eq!(m.limb_speed, 0.0);
    }

    /// `LongJumpToRandomPos.start`: the 9 x 5 x 9 box without the frog's own
    /// block, x fastest, then y, then z, each weighted by its squared distance.
    #[test]
    fn the_candidates_cover_the_jump_box_in_vanilla_order() {
        let c = candidates([10, 64, -3], JUMP_WIDTH, JUMP_HEIGHT);
        assert_eq!(c.len(), 9 * 5 * 9 - 1);
        assert_eq!(c[0], ([6, 62, -7], 16 + 4 + 16));
        assert_eq!(c[1], ([7, 62, -7], 9 + 4 + 16));
        assert_eq!(c[9], ([6, 63, -7], 16 + 1 + 16));
        assert_eq!(c[45], ([6, 62, -6], 16 + 4 + 9));
        assert!(!c.iter().any(|(p, _)| *p == [10, 64, -3]));
        assert_eq!(c.last().unwrap().0, [14, 66, 1]);
    }

    /// `WeightedRandom.getRandomItem`: `nextInt(total)` walked down the
    /// list, and the drawn candidate is removed.
    #[test]
    fn a_draw_takes_the_weighted_candidate_out() {
        let mut j = LongJump {
            candidates: vec![([0, 0, 0], 1), ([1, 0, 0], 4), ([2, 0, 0], 9)],
            ..LongJump::default()
        };
        let mut r = LegacyRandom::new(7);
        let mut lr = LevelRandom::for_level(r.clone());
        let pick = r.next_int_bound(14).unwrap();
        let want = if pick < 1 {
            0
        } else if pick < 5 {
            1
        } else {
            2
        };
        assert_eq!(j.draw(&mut lr).unwrap().0, [want, 0, 0]);
        assert_eq!(j.candidates.len(), 2);
        assert!(!j.candidates.iter().any(|c| c.0 == [want, 0, 0]));
    }

    /// `CountDownCooldownTicks`: the memory falls by one a tick and is gone
    /// the tick after it reached 0; the sample is 100..=140, halved after a
    /// failed search, drawn from the level's random and not the frog's.
    #[test]
    fn pregnancy_is_a_memory_that_the_laying_erases() {
        let mut m = frog();
        assert!(!is_pregnant(&m));
        set_pregnant(&mut m, true);
        set_pregnant(&mut m, true);
        assert!(is_pregnant(&m));
        assert_eq!(
            m.extra.memories.iter().filter(|x| x.0 == PREGNANT).count(),
            1
        );
        set_pregnant(&mut m, false);
        assert!(!is_pregnant(&m));
    }

    #[test]
    fn the_land_search_goes_out_by_manhattan_distance() {
        let all = manhattan_box([0, 0, 0], 8);
        assert_eq!(all[0], [0, 0, 0]);
        assert_eq!(all.len(), 17 * 17 * 17);
        // x, then y, and +z before -z, at distance 1.
        assert_eq!(
            all[1..7],
            [
                [-1, 0, 0],
                [0, -1, 0],
                [0, 0, 1],
                [0, 0, -1],
                [0, 1, 0],
                [1, 0, 0]
            ]
        );
    }

    #[test]
    fn the_long_jump_cooldown_counts_down_and_is_erased() {
        let mut m = frog();
        let own = m.random.state();
        sample_cooldown(&mut m, &mut crate::mobs::world::level_random(3), 2);
        let t = jump_cooldown(&m).unwrap();
        assert!((50..=70).contains(&t));
        assert_eq!(m.random.state(), own);
        set_jump_cooldown(&mut m, Some(1));
        set_jump_cooldown(&mut m, Some(0));
        assert_eq!(jump_cooldown(&m), Some(0));
        set_jump_cooldown(&mut m, None);
        assert_eq!(jump_cooldown(&m), None);
        assert!(!m.extra.memories.iter().any(|x| x.0 == LONG_JUMP_COOLDOWN));
    }


    /// `Frog` keeps its pregnancy as the `IS_PREGNANT` brain memory, and
    /// `Brain.Packed` saves every memory: `mob_to_nbt`/`mob_from_nbt` are the
    /// pair `Game::mob_tree_nbt` and `Game::load_mob` go through. Horizon read
    /// the memory back but wrote no `Brain` for a frog, so a frog made
    /// pregnant in play that unloaded with its chunk came back unpregnant and
    /// never laid its frogspawn.
    #[test]
    fn a_pregnant_frog_keeps_its_pregnancy_through_a_save() {
        use crate::entity_nbt::{mob_from_nbt, mob_to_nbt};
        use horizon_core::nbt::Tag;
        let memory = |tag: &Tag| {
            tag.get("Brain")
                .and_then(|b| b.get("memories"))
                .and_then(|b| b.get("minecraft:is_pregnant"))
                .cloned()
        };
        let mut m = frog();
        set_pregnant(&mut m, true);
        let saved = mob_to_nbt(&m, [1, 2, 3, 4], None, None, |_| None);
        assert_eq!(
            memory(&saved),
            Some(Tag::compound([(
                "value",
                Tag::Compound(Default::default())
            )])),
            "the save names the pregnancy"
        );
        let types = std::collections::HashMap::from([(m.kind.id.clone(), m.kind.clone())]);
        let back = mob_from_nbt(&saved, &types, 2, None).unwrap();
        assert!(is_pregnant(&back), "the reloaded frog is pregnant");
        // A frog that laid its spawn saves no memory, even over a compound
        // that still names one.
        set_pregnant(&mut m, false);
        let saved = mob_to_nbt(&m, [1, 2, 3, 4], Some(&saved), None, |_| None);
        assert_eq!(memory(&saved), None, "a laid frog saves no pregnancy");
        let back = mob_from_nbt(&saved, &types, 3, None).unwrap();
        assert!(!is_pregnant(&back), "and does not come back pregnant");
    }

    /// `TryLaySpawnOnFluidNearLand`: the frogspawn is set, then
    /// `block_place` is posted from the frog at the new block, then the lay
    /// sound, and the pregnancy is erased.
    #[test]
    #[ignore = "daily"]
    fn laying_frogspawn_posts_block_place_from_the_frog() {
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            |world, registry| {
                world.chunks.insert(
                    (0, 0),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
                );
                let water = registry.default_state("minecraft:water").unwrap();
                world.set(8, 63, 7, water).unwrap();
            },
            |w, _| {
                let mut m = frog();
                m.position = Vec3::new(8.5, 64.0, 8.5);
                m.on_ground = true;
                set_pregnant(&mut m, true);
                let mut lr = crate::health::TestLevel::new(3);
                let mut events = Vec::new();
                let mut cx = Cx {
                    w,
                    level: &mut lr.level(),
                    events: &mut events,
                };
                LaySpawn::lay(&mut m, &mut cx);
                let spawn = w.registry.default_state("minecraft:frogspawn").unwrap();
                assert!(
                    events
                        .iter()
                        .any(|e| matches!(e, MobEvent::SetBlock { pos: [8, 64, 7], state } if *state == spawn)),
                    "the frogspawn is set: {events:?}"
                );
                assert!(
                    events.iter().any(|e| matches!(
                        e,
                        MobEvent::GameEvent {
                            event: "minecraft:block_place",
                            at: [8.5, 64.5, 7.5],
                            source: 1,
                            state,
                        } if *state == spawn
                    )),
                    "the new frogspawn posts block_place from the frog: {events:?}"
                );
                assert!(!is_pregnant(&m), "laying erases the pregnancy");
            },
        );
        assert!(ran, "vanilla data is missing");
    }
}
