//! The per-tick mob update in vanilla order (`Entity.baseTick`,
//! `LivingEntity.baseTick/tick/aiStep`, `Mob.serverAiStep`) and the
//! dispatch to each species family's goals and special behaviour.
pub(crate) use freezing::in_powder_snow;
pub use freezing::{can_freeze, wears_gaze_disguise};
mod feeding;
mod freezing;

pub use feeding::takes_food;

use super::{
    Damage, DamageKind, MobEvent, MobWorld, Species, Who, aquatic,
    control::{BodyRotation, LookControl, MoveControl},
    farm,
    goals::{Cx, GoalSelector},
    hostile,
    nav::Navigation,
    path::{PATH_TYPES, Path},
    push::{Cap, Verdict, root_vehicle},
    undead,
};
use crate::blocks::Fluid;
use crate::breathing::{Breath, breathe, drown, own_air};
use crate::health::DamageLevel;
use crate::powder_snow::particle_velocity as powder_snow_particle_velocity;
use horizon_core::{
    geometry::{Aabb, ShapeOp, Vec3, VoxelShape},
    random::RandomSource,
};
use horizon_world::native_load_context::LoopContext;

#[cfg(test)]
mod inside_walk_tests {
    use super::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    /// What the test's fire does, as a part's callback would: it queues a push
    /// and a wither effect, and can kill or panic on later contacts. The
    /// mod_api dispatch hop this stands for is the player's, the item's and the
    /// the projectile's tests' to prove; the mob tests here prove the walk, its
    /// arms, the flush and the containment.
    /// The module's fire reads process-wide statics: one test at a time.
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static CONTACTS: Mutex<Vec<[i32; 3]>> = Mutex::new(Vec::new());
    static KILL_AFTER: AtomicUsize = AtomicUsize::new(usize::MAX);
    static PANIC_AFTER: AtomicUsize = AtomicUsize::new(usize::MAX);
    static ENABLED: AtomicBool = AtomicBool::new(false);

    fn reset(kill: usize, panic: usize, enabled: bool) {
        mod_api::__private::resolve_effects(["minecraft:wither"], |_| Some(0)).unwrap();
        CONTACTS.lock().unwrap().clear();
        KILL_AFTER.store(kill, Ordering::SeqCst);
        PANIC_AFTER.store(panic, Ordering::SeqCst);
        ENABLED.store(enabled, Ordering::SeqCst);
    }

    /// What the fire's callback does, as a part's would.
    fn fire_like_a_part(at: horizon_core::position::BlockPos, staged: &mut myriade_types::InsideEffects) {
        let mut contacts = CONTACTS.lock().unwrap();
        contacts.push([at.x, at.y, at.z]);
        let seen = contacts.len();
        drop(contacts);
        if seen > PANIC_AFTER.load(Ordering::SeqCst) {
            panic!("an entity_inside part panics");
        }
        if seen > KILL_AFTER.load(Ordering::SeqCst) {
            staged.run_after(
                myriade_types::InsideEffect::Freeze,
                myriade_types::EntityWrite::Hurt {
                    source: None,
                    amount: 1000.0,
                },
            );
        }
        staged.run_after(
            myriade_types::InsideEffect::Freeze,
            myriade_types::EntityWrite::Push([0.5, 0.0, 0.0]),
        );
        staged.run_after(
            myriade_types::InsideEffect::Freeze,
            myriade_types::EntityWrite::AddEffect {
                effect: mod_api::__private::effect_key("minecraft:wither")
                    .expect("an effect key resolves"),
                duration: 40,
                amplifier: 0,
            },
        );
    }

    fn install_walk_hooks() {
        let _ = crate::block_contact::HOOKS.set(crate::block_contact::Hooks {
            step_on: |_, _| {},
            any_inside: || ENABLED.load(Ordering::SeqCst),
            inside_listens: |_| ENABLED.load(Ordering::SeqCst),
            pass_listens: |_| false,
            fall_listens: |_| false,
            bounce_listens: |_| false,
            fall_on: |_, _, _, _, _, _| {},
            bounce_on: |_, _, _, _, _| {},
            // The production binding's own shape: the whole callback staged,
            // a panic contained with it.
            inside_fire: |_level,
                          _dimension,
                          _entity,
                          _kind,
                          _state,
                          at,
                          _precise,
                          staged,
                          _writes| {
                let fired = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fire_like_a_part(at, staged);
                }));
                let _ = fired;
            },
        });
    }


    /// The fire's pushes and its wither effect reach the mob in its own tick,
    /// one contact a block, in the shared walk's order.
    #[test]
    #[ignore = "daily"]
    fn a_part_pushes_a_mob_once_a_block_in_walk_order() {
        let _test = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset(usize::MAX, usize::MAX, true);
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            |world, registry| {
                world.chunks.insert(
                    (0, 0),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
                );
                let stone = registry.default_state("minecraft:stone").unwrap();
                for x in [1, 2] {
                    world.set(x, 64, 1, stone).unwrap();
                }
                install_walk_hooks();
            },
            |w, types| {
                let mut mob = mob_at_move(types);
                let mut level = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let before = mob.velocity.x;
                crate::block_contact::in_level(Some("minecraft:overworld"), || {
                    effects_from_blocks(&mut mob, w, &mut level.level(), &mut events);
                });
                assert_eq!(
                    *CONTACTS.lock().unwrap(),
                    vec![[1, 64, 1], [2, 64, 1]],
                    "one contact a block, in the walk's order"
                );
                assert!(
                    (mob.velocity.x - before - 1.0).abs() < 1e-9,
                    "both pushes reached the mob"
                );
                assert!(
                    mob.extra
                        .effects
                        .iter()
                        .any(|effect| effect.id == "minecraft:wither" && effect.duration == 40),
                    "the wither effect reached the mob's own list"
                );
                assert!(crate::block_contact::take_pending().is_empty());
            },
        );
        assert!(ran, "the vanilla test resources must be available");
    }

    /// A killing write stops the mob's flush: the walk still reaches both
    /// blocks, the writes after the death never apply.
    #[test]
    #[ignore = "daily"]
    fn a_killing_write_stops_the_mobs_flush() {
        let _test = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset(0, usize::MAX, true);
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            |world, registry| {
                world.chunks.insert(
                    (0, 0),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
                );
                let stone = registry.default_state("minecraft:stone").unwrap();
                for x in [1, 2] {
                    world.set(x, 64, 1, stone).unwrap();
                }
                install_walk_hooks();
            },
            |w, types| {
                let mut mob = mob_at_move(types);
                let mut level = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let before = mob.velocity.x;
                crate::block_contact::in_level(Some("minecraft:overworld"), || {
                    effects_from_blocks(&mut mob, w, &mut level.level(), &mut events);
                });
                assert_eq!(CONTACTS.lock().unwrap().len(), 2);
                assert!(!mob.alive(), "the killing write applied");
                assert!(
                    (mob.velocity.x - before).abs() < 1e-9,
                    "the writes after the death never apply"
                );
            },
        );
        assert!(ran, "the vanilla test resources must be available");
    }

    /// A panicking part is contained mid-walk: the mob's tick answers, its
    /// state keeps what it had, and the level host still answers after (the
    /// lifted rows themselves are arismoko/horizon#2285's restoration, its own tests'; the
    /// item batch test proves the lifted pair end to end).
    #[test]
    #[ignore = "daily"]
    fn a_panicking_part_leaves_the_mobs_tick_restored() {
        let _test = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset(usize::MAX, 0, true);
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            |world, registry| {
                world.chunks.insert(
                    (0, 0),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
                );
                let stone = registry.default_state("minecraft:stone").unwrap();
                for x in [1, 2] {
                    world.set(x, 64, 1, stone).unwrap();
                }
                install_walk_hooks();
            },
            |w, types| {
                let mut mob = mob_at_move(types);
                let mut level = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let before = mob.velocity.x;
                crate::block_contact::in_level(Some("minecraft:overworld"), || {
                    effects_from_blocks(&mut mob, w, &mut level.level(), &mut events);
                });
                assert_eq!(
                    CONTACTS.lock().unwrap().len(),
                    2,
                    "both contacts reached the fire; each panic was contained"
                );
                assert!(mob.alive(), "the mob's tick answered");
                assert!(
                    (mob.velocity.x - before).abs() < 1e-9,
                    "the panicking callback's staged writes never applied"
                );
                assert!(
                    level.level().random().next_int_bound(2).is_ok(),
                    "the level host still answers"
                );
            },
        );
        assert!(ran, "the vanilla test resources must be available");
    }

    /// Without a listening part the walk runs its vanilla moments only:
    /// nothing of the part plumbing applies, exactly what main did with no
    /// listener.
    #[test]
    #[ignore = "daily"]
    fn a_mob_without_a_part_keeps_vanilla_behavior() {
        let _test = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset(usize::MAX, usize::MAX, false);
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            |world, registry| {
                world.chunks.insert(
                    (0, 0),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
                );
                let stone = registry.default_state("minecraft:stone").unwrap();
                for x in [1, 2] {
                    world.set(x, 64, 1, stone).unwrap();
                }
                install_walk_hooks();
            },
            |w, types| {
                let mut mob = mob_at_move(types);
                let mut level = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let before = (mob.velocity.x, mob.health, mob.extra.effects.len());
                crate::block_contact::in_level(Some("minecraft:overworld"), || {
                    effects_from_blocks(&mut mob, w, &mut level.level(), &mut events);
                });
                assert_eq!(
                    (mob.velocity.x, mob.health, mob.extra.effects.len()),
                    before,
                    "no writes without a part"
                );
                assert!(CONTACTS.lock().unwrap().is_empty());
                assert!(crate::block_contact::take_pending().is_empty());
            },
        );
        assert!(ran, "the vanilla test resources must be available");
    }

}
#[path = "ai_glide.rs"]
mod glide;
use glide::update_fall_flying;

mod step;
pub(super) use step::inherited_step_sound;


/// `SwingAnimation`: the swing's kind and its base ticks, as the mob sends them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwingAnimation {
    pub kind: u8,
    pub duration: i32,
}

impl SwingAnimation {
    /// `SwingAnimation.DEFAULT`: WHACK for 6 ticks.
    pub const DEFAULT: SwingAnimation = SwingAnimation {
        kind: 1,
        duration: 6,
    };
}

/// The swing's ticks: haste shortens it, mining fatigue lengthens it.
pub fn swing_duration(animation: SwingAnimation, dig_speed: Option<i32>, fatigue: Option<i32>) -> i32 {
    let duration = animation.duration;
    if let Some(amplification) = dig_speed {
        duration - (1 + amplification)
    } else if let Some(amplifier) = fatigue {
        duration + (1 + amplifier) * 2
    } else {
        duration
    }
}

#[derive(Clone, Debug, Default)]
pub struct Ai {
    /// `Mob.homePosition` and `homeRadius`: where a restricted mob stays
    /// (a lead's holder, a happy ghast's float area).
    pub home: Option<([i32; 3], i32)>,
    /// Mob keeps homePosition even when clearHome makes homeRadius -1.
    /// Active homes still use `home`; this only retains its hidden position.
    last_home_position: [i32; 3],
    pub goals: GoalSelector,
    pub targets: GoalSelector,
    pub nav: Navigation,
    pub(super) navigation_redirect: Option<Box<super::nav::Redirect>>,
    pub mov: MoveControl,
    pub look: LookControl,
    pub body: BodyRotation,
    /// JumpControl.jump (requested this tick).
    pub jump: bool,
    /// LivingEntity.jumping
    pub jumping: bool,
    pub no_jump_delay: i32,
    pub xxa: f32,
    pub yya: f32,
    pub zza: f32,
    pub speed: f32,
    pub target: Option<Who>,
    pub last_hurt_by: Option<Who>,
    pub last_hurt_by_time: i32,
    /// The bare UUID of an unresolved `lastHurtByMob` (see `hurt_by`).
    pub last_hurt_by_ref: Option<[i32; 4]>,
    /// Ticks left during which kills count as player kills.
    pub last_hurt_by_player: i32,
    pub no_action_time: i32,
    pub ai_paused: bool, // A mod paused serverAiStep (`MobRef::set_ai_paused`): mobs/ai_pause.rs.
    pub dab_turns: u32,
    pub aggressive: bool,
    pub panicking: bool,
    pub malus: [Option<f32>; PATH_TYPES],
    /// A path computed in canUse and consumed by start.
    pub pending_path: Option<Path>,
    /// Sensing: line-of-sight results cached for one tick.
    pub seen_cache: Vec<(Who, bool)>,
    /// Swing animation progress (attackAnim) and ticks.
    pub swing_time: i32,
    pub swinging: bool,
    /// The current swing's `SwingDescription.durationTicks`; 0 is the default 6.
    pub swing_duration: i32,
    /// Walked distance and the next footstep.
    pub steps: super::living::Steps,
}
impl Ai {
    /// `Mob.getHomePosition`, including the position retained by clearHome.
    pub fn home_position(&self) -> [i32; 3] {
        self.home.map_or(self.last_home_position, |(pos, _)| pos)
    }
    /// Adopt only the two persistent home fields after loading Mob data.
    pub fn restore_home(&mut self, pos: [i32; 3], radius: i32) {
        self.last_home_position = pos;
        self.home = (radius != -1).then_some((pos, radius));
    }
    /// `Mob.clearHome`: clear the radius, retaining the position.
    pub fn clear_home(&mut self) {
        self.last_home_position = self.home_position();
        self.home = None;
    }
    /// `Mob.isWithinRestriction(pos)`: an unrestricted mob is within
    /// everywhere.
    pub fn is_within_home(&self, p: [i32; 3]) -> bool {
        self.home
            .is_none_or(|(c, r)| super::goals::within_home(c, r, p))
    }
}
/// The module implementing a species' behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Farm,
    Undead,
    Hostile,
    Aquatic,
    Generic,
    /// No goals at all (Giant).
    Inert,
    Witch,
    Enderman,
    Tame,
    /// Blazes and ghasts.
    Nether,
    Golem,
    Illager,
    /// Piglins, brutes, hoglins and zoglins.
    Piglin,
    Equine,
    Phantom,
    /// Slimes and magma cubes.
    Cube,
    Guardian,
    Shulker,
    Vex,
    Ravager,
    Wither,
    Allay,
    Villager,
    Trader,
    Breeze,
    Sniffer,
    HappyGhast,
    Warden,
    Creaking,
    /// Nautili and zombie nautili.
    Nautilus,
    Strider,
    Bat,
    Bee,
    Panda,
    PolarBear,
}
pub fn family(s: Species) -> Family {
    use Species as S;
    match s {
        S::Pig | S::Cow | S::Mooshroom | S::Sheep | S::Chicken => Family::Farm,
        S::Zombie
        | S::Husk
        | S::Drowned
        | S::ZombieVillager
        | S::Skeleton
        | S::Stray
        | S::Bogged
        | S::Parched
        | S::WitherSkeleton
        | S::ZombifiedPiglin => Family::Undead,
        S::Creeper | S::Spider | S::CaveSpider | S::Silverfish | S::Endermite => Family::Hostile,
        S::Squid
        | S::GlowSquid
        | S::Cod
        | S::Tadpole
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Dolphin
        | S::Axolotl => Family::Aquatic,
        S::Giant => Family::Inert,
        S::Witch => Family::Witch,
        S::Wolf | S::Cat => Family::Tame,
        S::Blaze | S::Ghast => Family::Nether,
        S::IronGolem | S::SnowGolem | S::CopperGolem => Family::Golem,
        S::Pillager | S::Vindicator | S::Evoker | S::Illusioner => Family::Illager,
        S::Vex => Family::Vex,
        S::Ravager => Family::Ravager,
        S::Wither => Family::Wither,
        S::Allay => Family::Allay,
        S::Creaking => Family::Creaking,
        S::Villager => Family::Villager,
        S::WanderingTrader => Family::Trader,
        S::Breeze => Family::Breeze,
        S::Sniffer => Family::Sniffer,
        S::HappyGhast => Family::HappyGhast,
        S::Piglin | S::PiglinBrute | S::Hoglin | S::Zoglin => Family::Piglin,
        S::Horse
        | S::Donkey
        | S::Mule
        | S::SkeletonHorse
        | S::ZombieHorse
        | S::Camel
        | S::CamelHusk
        | S::Llama
        | S::TraderLlama => Family::Equine,
        S::Nautilus | S::ZombieNautilus => Family::Nautilus,
        S::Phantom => Family::Phantom,
        S::Enderman => Family::Enderman,
        S::Slime | S::MagmaCube | S::SulfurCube => Family::Cube,
        S::Guardian | S::ElderGuardian => Family::Guardian,
        S::Warden => Family::Warden,
        S::Shulker => Family::Shulker,
        S::Strider => Family::Strider,
        S::Bat => Family::Bat,
        S::Bee => Family::Bee,
        S::Panda => Family::Panda,
        S::PolarBear => Family::PolarBear,
        _ => Family::Generic,
    }
}



/// Subclasses of `net.minecraft.world.entity.monster.Monster`.
pub fn is_monster(s: Species) -> bool {
    use Species as S;
    matches!(
        s,
        S::Zombie
            | S::Husk
            | S::Drowned
            | S::ZombieVillager
            | S::ZombifiedPiglin
            | S::Giant
            | S::Skeleton
            | S::Stray
            | S::Bogged
            | S::Parched
            | S::WitherSkeleton
            | S::Creeper
            | S::Spider
            | S::CaveSpider
            | S::Enderman
            | S::Endermite
            | S::Silverfish
            | S::Witch
            | S::Pillager
            | S::Vindicator
            | S::Evoker
            | S::Illusioner
            | S::Ravager
            | S::Vex
            | S::Piglin
            | S::PiglinBrute
            | S::Zoglin
            | S::Blaze
            | S::Guardian
            | S::ElderGuardian
            | S::Breeze
            | S::Creaking
            | S::Warden
            | S::Wither
    )
}
/// A tick either completed or paused at the fox's inline consumption boundary.
#[derive(Debug)]
pub enum TickProgress {
    Complete(bool),
    Consumption {
        decision: super::fox::Consumption,
        /// The base-tick snapshot, before contact ignition or listener writes.
        was_on_fire: bool,
    },
}







/// The mob's view of the shared inside walk: its box, its life, the level's
/// blocks, its own `entityInside` moments (kept as their own code, run
/// mid-walk where vanilla runs them), and the part's contact, fired mid-walk
/// over the level random host the mob's tick already holds.
struct MobInside<'a, 'w, 'l> {    w: &'a MobWorld<'w>,
    level: &'a mut DamageLevel<'l>,
    events: &'a mut Vec<MobEvent>,
}

impl crate::block_contact::InsideWalk for MobInside<'_, '_, '_> {
    type Error = ();
    fn bounds_at(&self, at: Vec3) -> horizon_core::geometry::Aabb {
        self.m.bounds_at(at)
    }
    fn is_alive(&self) -> bool {
        self.m.alive()
    }
    fn block(&self, at: horizon_core::position::BlockPos) -> horizon_world::registry::BlockStateId {
        self.w.world.block(at.x, at.y, at.z)
    }
    fn is_air(&self, state: horizon_world::registry::BlockStateId) -> bool {
        self.w.registry.state(state).is_none_or(|d| {
            matches!(
                d.name.as_str(),
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
            )
        })
    }
    fn inside_shape(
        &mut self,
        _state: horizon_world::registry::BlockStateId,
        _at: horizon_core::position::BlockPos,
    ) -> crate::block_contact::InsideShape {
        // `getEntityInsideCollisionShape` is the whole block for every block a
        // mob meets, as the mob's own walk always treated it.
        crate::block_contact::InsideShape::Full
    }
    fn entity_inside(
        &mut self,
        at: horizon_core::position::BlockPos,
        state: horizon_world::registry::BlockStateId,
        _step: i32,
        precise: bool,
        effects: &mut myriade_types::InsideEffects,
    ) -> std::result::Result<(), ()> {
        let (x, y, z) = (at.x, at.y, at.z);
        let p = [x, y, z];
        // The part's contact, mid-walk, staged: its writes join this step only
        // when it answered, and a panic is contained.
        if let Some(hooks) = crate::block_contact::HOOKS.get()
            && let Some(dimension) = crate::block_contact::current_level()
            && (hooks.inside_listens)(state)
        {
            let mut staged = myriade_types::InsideEffects::default();
            let mut writes = Vec::new();
            let kind = self.m.kind.id.clone();
            let id = self.m.id as i32;
            (hooks.inside_fire)(
                self.level,
                dimension,
                id,
                &kind,
                state,
                at,
                precise,
                &mut staged,
                &mut writes,
            );
            effects.absorb_step(staged);
            crate::block_contact::inside_written(state, at, writes);
        }
        // PowderSnowBlock.entityInside: a burning mob destroys the
        // block (`canEntityGrief` for the mob), then EXTINGUISH puts it out.
        if self.w.block_name(x, y, z) == "minecraft:powder_snow" {
            if self.m.is_on_fire() && self.w.can_grief(self.m.id) {
                self.events.push(MobEvent::BreakBlock {
                    pos: p,
                    drop: false,
                    by: None,
                    breaker: None,
                });
            }
            self.m.extinguish_fire();
        }
        match self.w.block_name(x, y, z) {
            "minecraft:cactus" => {
                self.m
                    .damage(self.level, 1.0, Damage::of(DamageKind::Cactus));
            }
            // CampfireBlock.entityInside: a lit campfire hurts a
            // living entity by its fire damage (1, soul 2).
            name @ ("minecraft:campfire" | "minecraft:soul_campfire")
                if self.w.prop(x, y, z, "lit") == Some("true") =>
            {
                let damage = if name == "minecraft:campfire" {
                    1.0
                } else {
                    2.0
                };
                self.m
                    .damage(self.level, damage, Damage::of(DamageKind::Campfire));
            }
            // WebBlock.entityInside -> makeStuckInBlock, which a
            // spider ignores for cobwebs and a wither always.
            "minecraft:cobweb"
                if !matches!(
                    self.m.species(),
                    Species::Spider | Species::CaveSpider | Species::Wither
                ) =>
            {
                self.m.fall_distance = 0.0;
                self.m.stuck_multiplier = if self.m.has_effect("minecraft:weaving") {
                    Vec3::new(0.5, 0.25, 0.5)
                } else {
                    Vec3::new(0.25, f64::from(0.05_f32), 0.25)
                };
            }
            "minecraft:sweet_berry_bush"
                if !matches!(self.m.species(), Species::Fox | Species::Bee) =>
            {
                if self.m.species() != Species::Wither {
                    self.m.fall_distance = 0.0;
                    self.m.stuck_multiplier =
                        Vec3::new(f64::from(0.8_f32), 0.75, f64::from(0.8_f32));
                }
                let moved = self.m.previous.subtract(self.m.position);
                if self.w.prop(x, y, z, "age") != Some("0")
                    && moved.horizontal_distance_squared() > 0.0
                    && (moved.x.abs() >= f64::from(0.003_f32)
                        || moved.z.abs() >= f64::from(0.003_f32))
                {
                    self.m
                        .damage(self.level, 1.0, Damage::of(DamageKind::SweetBerryBush));
                }
            }
            // PowderSnowBlock.entityInside: a living entity is stuck
            // (and forgets its fall) only while the block at its
            // own feet is powder snow.
            "minecraft:powder_snow"
                if self.w.block_name(
                    self.m.position.x.floor() as i32,
                    self.m.position.y.floor() as i32,
                    self.m.position.z.floor() as i32,
                ) == "minecraft:powder_snow" =>
            {
                if self.m.species() != Species::Wither {
                    self.m.fall_distance = 0.0;
                    self.m.stuck_multiplier =
                        Vec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32));
                }
                let known_movement = self
                    .w
                    .controlling_player_movement
                    .unwrap_or(self.m.velocity);
                if let Some(velocity) =
                    powder_snow_particle_velocity(known_movement, self.level.random())
                {
                    self.events.push(MobEvent::PowderSnowParticle {
                        at: [self.m.position.x, f64::from(y + 1), self.m.position.z],
                        velocity,
                    });
                }
            }
            "minecraft:honey_block" => honey_slide(self.m, p, self.level, self.events),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod powder_snow_particle_tests {
    use super::*;

}

/// `Entity.isInWall` (minus the noPhysics early-out, which the caller applies):
/// visit every cell covered by the thin eye box and ask whether a suffocating
/// collision shape actually intersects it. Width multiplication is a float.
pub fn in_wall(
    width: f32,
    eye: [f64; 3],
    mut intersects: impl FnMut(i32, i32, i32, &Aabb) -> bool,
) -> bool {
    let side = f64::from(width * 0.8_f32);
    let half = [side / 2.0, 1.0e-6 / 2.0, side / 2.0];
    let lo = [0, 1, 2].map(|i| eye[i] - half[i]);
    let hi = [0, 1, 2].map(|i| eye[i] + half[i]);
    let bounds = Aabb::new(
        Vec3::new(lo[0], lo[1], lo[2]),
        Vec3::new(hi[0], hi[1], hi[2]),
    );
    let c0 = lo.map(|v| v.floor() as i32);
    let c1 = hi.map(|v| v.floor() as i32);
    (c0[0]..=c1[0])
        .any(|x| (c0[1]..=c1[1]).any(|y| (c0[2]..=c1[2]).any(|z| intersects(x, y, z, &bounds))))
}






// Keep the actual selectors (including partially updated goal state) attached to
// their mob even when a goal callback unwinds. Drop does not invoke goal code.
struct BorrowedGoalSelectors<'a> {    targets: GoalSelector,
    goals: GoalSelector,
}

impl<'a> BorrowedGoalSelectors<'a> {
}

impl Drop for BorrowedGoalSelectors<'_> {
    fn drop(&mut self) {
        self.mob.ai.targets = std::mem::take(&mut self.targets);
        self.mob.ai.goals = std::mem::take(&mut self.goals);
    }
}

#[cfg(test)]
mod selector_restore_tests {
    use super::super::goals::Goal;
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Debug)]
    struct TrackedGoal {
        progress: i32,
        drops: Arc<AtomicUsize>,
    }
    impl Drop for TrackedGoal {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl Goal for TrackedGoal {
        fn flags(&self) -> u8 {
            0
        }
        fn box_clone(&self) -> Box<dyn Goal> {
            panic!("selectors must not clone goals")
        }
        fn set_start_delay(&mut self, ticks: i32) {
            self.progress = ticks;
        }
        fn name(&self) -> &'static str {
            if self.progress == 7 {
                "progress"
            } else {
                "initial"
            }
        }
    }


}

/// `Cat.removeWhenFarAway` and `Ocelot.removeWhenFarAway`: an untamed cat or an
/// untrusting ocelot is removed once it is older than 2400 ticks. That is
/// `Entity.tickCount`, not `AgeableMob.age`, which only counts a baby up to
/// adulthood. `Animal.removeWhenFarAway` keeps every other animal.
fn feline_removed_when_far(s: Species, tamed: bool, ticks: u32) -> bool {
    matches!(s, Species::Cat | Species::Ocelot) && !tamed && ticks > 2400
}
/// The types whose `removeWhenFarAway` is always false (`Villager`,
/// `WanderingTrader`, `AbstractGolem`: iron, snow and copper golems and
/// shulkers; `Allay`, `Warden`), and the wither, whose `checkDespawn` never
/// removes it for distance.
fn never_removed_when_far(s: Species) -> bool {
    matches!(
        s,
        Species::Villager
            | Species::WanderingTrader
            | Species::IronGolem
            | Species::SnowGolem
            | Species::CopperGolem
            | Species::Shulker
            | Species::Allay
            | Species::Warden
            | Species::Wither
    )
}
/// `AgeableMob.canUseGoldenDandelion`: a golden dandelion in hand, a baby
/// whose particle timer has run out, and a type outside
/// `#minecraft:cannot_be_age_locked`.
pub fn can_use_golden_dandelion(
    item: &str,
    baby: bool,
    cooldown: i32,
    cannot_be_age_locked: bool,
) -> bool {
    item == "minecraft:golden_dandelion" && baby && cooldown == 0 && !cannot_be_age_locked
}
/// Whether `playEatingSound` binds the sound to the animal: the frog's, the
/// goat's and the sniffer's are `level().playSound(null, this, ..., NEUTRAL,
/// ...)`, the others `Entity.playSound` at its position.
pub fn eating_sound_is_bound(species: Species) -> bool {
    matches!(species, Species::Frog | Species::Goat | Species::Sniffer)
}
/// Mob.doHurtTarget: attack damage (+ species extras) to the target.
pub use super::combat::do_hurt_target;






#[cfg(test)]
mod bucket_axolotl_tests {
    use super::*;
    #[test]
    fn bucket_axolotl_uses_aquatic_not_generic_land_ai() {
        assert_eq!(family(Species::Axolotl), Family::Aquatic);
    }
}
#[cfg(test)]
mod despawn_tests {
    use super::*;
    /// Village villagers are placed without PersistenceRequired (unlike the
    /// village's iron golem), so they must never despawn for distance.
    #[test]
    fn villagers_traders_and_golems_never_despawn_for_distance() {
        for s in [
            Species::Villager,
            Species::WanderingTrader,
            Species::IronGolem,
            Species::SnowGolem,
            Species::CopperGolem,
            Species::Shulker,
        ] {
            assert!(never_removed_when_far(s), "{s:?}");
        }
        for s in [Species::Zombie, Species::ZombieVillager, Species::Pillager] {
            assert!(!never_removed_when_far(s), "{s:?}");
        }
    }

    /// `Cat.removeWhenFarAway` (`!isTame() && tickCount > 2400`) and
    /// `Ocelot.removeWhenFarAway` (`!isTrusting() && tickCount > 2400`): the two
    /// animals vanilla lets go, on `Entity.tickCount` and never on
    /// `AgeableMob.age`, which an adult keeps at 0.
    #[test]
    fn an_untamed_feline_goes_once_it_is_old() {
        for s in [Species::Cat, Species::Ocelot] {
            assert!(!feline_removed_when_far(s, false, 2400), "{s:?}");
            assert!(feline_removed_when_far(s, false, 2401), "{s:?}");
            assert!(!feline_removed_when_far(s, true, 12000), "{s:?}");
        }
        // Animal.removeWhenFarAway: every other animal stays at any age.
        for s in [Species::Cow, Species::Sheep, Species::Wolf] {
            assert!(!feline_removed_when_far(s, false, 12000), "{s:?}");
        }
    }
}

#[cfg(test)]
mod frog_tests {
    use super::*;
    use crate::mobs::MobTypes;
    use crate::mobs::control::MoveKind;
    use std::sync::Arc;

}

#[cfg(test)]
mod poison_effect_tests {
    use super::*;
    use crate::mobs::MobTypes;
    use std::sync::Arc;










}

#[cfg(test)]
mod hit_effect_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;


}

#[cfg(test)]
mod attack_sound_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;


    /// `Mob.doHurtTarget` ends with `playAttackSound`, which `Dolphin`,
    /// `Axolotl` and `Panda` override with their own bite at volume 1 and
    /// pitch 1. Without the dolphin arm its bite is silent (no sound event at
    /// all); the axolotl and the panda sit next to it as the control.
    #[test]
    #[ignore = "daily"]
    fn a_dolphins_blow_plays_its_attack_sound() {
        let Some(dolphin) = attack_sounds("minecraft:dolphin", false) else {
            return;
        };
        assert_eq!(dolphin, vec![("entity.dolphin.attack", 1.0, 1.0)]);
        for (kind, sound) in [
            ("minecraft:axolotl", "entity.axolotl.attack"),
            ("minecraft:panda", "entity.panda.bite"),
        ] {
            assert_eq!(
                attack_sounds(kind, false),
                Some(vec![(sound, 1.0, 1.0)]),
                "{kind}"
            );
        }
        // Entity.playSound skips a silent entity, so neither attack sound plays.
        for kind in ["minecraft:dolphin", "minecraft:axolotl", "minecraft:panda"] {
            assert_eq!(attack_sounds(kind, true), Some(vec![]), "{kind}");
        }
    }
}

#[cfg(test)]
mod same_vehicle_push_tests {
    use super::*;
    use crate::mobs::world::with_test_world;









}

#[cfg(test)]
mod warden_push_tests {
    use super::*;
    use crate::mobs::warden::{Pose, set_pose};
    use crate::mobs::world::{test_player, with_test_world};

}

#[cfg(test)]
mod ridden_tests {
    use super::*;
    use crate::mobs::world::with_test_world;


    #[test]
    #[ignore = "daily"]
    fn the_server_leaves_a_horse_to_its_riders_client() {
        let Some(moved) = ridden(false) else {
            return;
        };
        // The host's own ride: the horse walks west and falls through the air.
        assert!(moved.position.x < 0.4, "{:?}", moved.position);
        assert!(moved.position.y < 64.0, "{:?}", moved.position);
        let held = ridden(true).unwrap();
        assert_eq!(held.position, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(held.velocity, Vec3::ZERO);
        assert_eq!(held.yaw, 90.0, "tickRidden still turns it");
    }
}

#[cfg(test)]
mod bed_wake_tests {
    use super::*;
    use crate::mobs::world::with_test_world;

    #[test]
    #[ignore = "daily"]
    fn loaded_sleeping_position_refreshes_in_actual_mob_tick() {
        use horizon_core::nbt::Tag;
        let build = |world: &mut crate::world::World,
                     registry: &horizon_world::registry::BlockRegistry| {
            world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
            );
            let stone = registry.default_state("minecraft:stone").unwrap();
            for x in 0..12 {
                for z in 0..12 {
                    world.set(x, 63, z, stone).unwrap();
                }
            }
            let mut bed = registry
                .state(registry.default_state("minecraft:red_bed").unwrap())
                .unwrap()
                .clone();
            bed.properties.insert("facing".into(), "north".into());
            bed.properties.insert("occupied".into(), "false".into());
            for (part, z) in [("head", 0), ("foot", 1)] {
                bed.properties.insert("part".into(), part.into());
                world.set(0, 64, z, registry.id(&bed).unwrap()).unwrap();
            }
        };
        let ran = crate::mobs::world::with_built_test_world(&[], build, |w, types| {
            assert_eq!(w.block_name(0, 64, 0), "minecraft:red_bed");
            assert_eq!(w.prop(0, 64, 0, "part"), Some("head"));
            assert_eq!(w.prop(0, 64, 0, "facing"), Some("north"));
            let types = [(
                "minecraft:villager".to_owned(),
                std::sync::Arc::new(types.types["minecraft:villager"].clone()),
            )]
            .into_iter()
            .collect();
            let input = horizon_core::nbt::parse_snbt(r#"{id:"minecraft:villager",Pos:[0.5d,64.5625d,0.5d],Health:20f,NoAI:1b,NoGravity:1b,PersistenceRequired:1b,sleeping_pos:[I;0,64,0],Motion:[0d,0d,0d]}"#).unwrap().0;
            let mut loaded =
                crate::entity_nbt::mob_from_nbt(&input, &types, 233, Some(w.registry)).unwrap();
            assert_eq!(
                loaded.position,
                Vec3::new(0.5, 64.5625, 0.5),
                "loading alone must not perform the first-tick correction"
            );
            assert!(loaded.first_tick && loaded.no_ai && loaded.no_gravity);
            assert!(super::super::villager::sleeping(&loaded));
            assert_eq!(
                super::super::villager::sleeping_pos(&loaded),
                Some([0, 64, 0])
            );
            let mut awake_input = input.clone();
            let Tag::Compound(ref mut fields) = awake_input else {
                unreachable!()
            };
            fields.remove(&"sleeping_pos".into());
            fields.insert(
                "Pos".into(),
                Tag::List(
                    [5.5_f64, 64.0, 5.5]
                        .map(|v| Tag::Double(v.to_bits()))
                        .to_vec(),
                ),
            );
            let mut awake =
                crate::entity_nbt::mob_from_nbt(&awake_input, &types, 234, Some(w.registry))
                    .unwrap();
            let mut normal = awake.clone();
            normal.id = 235;
            let mut normal_events = Vec::new();
            assert!(super::super::villager::start_sleeping(
                &mut normal,
                [0, 64, 0],
                w,
                &mut normal_events
            ));
            assert_eq!(normal.position, Vec3::new(0.5, 64.6875, 0.5));
            assert!(normal_events.iter().any(|e| matches!(
                e,
                MobEvent::BedOccupied {
                    pos: [0, 64, 0],
                    occupied: true
                }
            )));
            let mut fixture = crate::health::TestLevel::new(0);
            let mut loaded_events = Vec::new();
            for n in 1..=2 {
                assert!(loaded.tick(w, &mut fixture.level(), &mut loaded_events));
                assert_eq!(loaded.age, n);
                assert!(!loaded.first_tick);
                assert_eq!(
                    loaded.position,
                    Vec3::new(0.5, 64.6875, 0.5),
                    "actual loaded sleeper at tick {n}"
                );
                assert_eq!(loaded.velocity, Vec3::ZERO);
                assert_eq!(loaded.health, 20.0);
                assert_eq!(
                    super::super::villager::sleeping_pos(&loaded),
                    Some([0, 64, 0])
                );
                assert!(awake.tick(w, &mut fixture.level(), &mut Vec::new()));
                assert_eq!(awake.position, Vec3::new(5.5, 64.0, 5.5));
                assert!(!super::super::villager::sleeping(&awake));
                assert!(normal.tick(w, &mut fixture.level(), &mut normal_events));
                assert_eq!(normal.position, Vec3::new(0.5, 64.6875, 0.5));
                assert_eq!(normal.velocity, Vec3::ZERO);
            }
            assert!(
                !loaded_events
                    .iter()
                    .any(|e| matches!(e, MobEvent::BedOccupied { .. })),
                "refresh must not re-run startSleeping side effects"
            );
        });
        assert!(ran, "prepared Mob/bed world must actually run");
    }


    #[test]
    #[ignore = "daily"]
    fn a_sleeper_without_ai_still_stands_when_its_bed_is_gone() {
        let Some(m) = no_ai_sleeper() else { return };
        assert!(
            !super::super::villager::sleeping(&m),
            "a missing bed wakes a no-AI sleeper"
        );
    }
}

#[cfg(test)]
mod turtle_ambient_tests {
    use super::*;
    use std::sync::Arc;


    /// `Turtle.getAmbientSound`: land ambient only for an adult on the ground
    /// and out of water.
    #[test]
    fn a_turtle_is_silent_in_water_in_the_air_and_as_a_baby() {
        let mut m = turtle();
        m.on_ground = true;
        assert!(!turtle_ambient_silent(&m));
        m.on_ground = false;
        assert!(turtle_ambient_silent(&m));
        m.on_ground = true;
        m.fluids.in_water = true;
        assert!(turtle_ambient_silent(&m));
    }
}

#[cfg(test)]
mod feed_tests {
    use super::*;
    use crate::mobs::MobTypes;
    use std::sync::Arc;



}

#[cfg(test)]
mod forgiveness_tests {
    use super::*;
    use std::sync::Arc;


    #[test]
    fn forgiveness_keeps_cancelled_target_but_ends_anger() {
        let mut golem = mob(Species::IronGolem);
        golem.extra.angry_at = Some(Who::Player);
        golem.extra.anger = 300;
        golem.ai.target = Some(Who::Player);
        assert!(forgives_dead_player(&golem));
        forgive_dead_player_with_change(&mut golem, super::super::TargetChange::Keep);
        assert_eq!(golem.ai.target, Some(Who::Player));
        assert_eq!(golem.extra.anger, 0);

        golem.extra.angry_at = Some(Who::Player);
        forgive_dead_player_with_change(
            &mut golem,
            super::super::TargetChange::Set(Some(Who::Mob(7))),
        );
        assert_eq!(golem.ai.target, Some(Who::Mob(7)));
    }

    #[test]
    #[ignore = "daily"]
    fn peaceful_drops_an_iron_golems_existing_player_target() {
        crate::mobs::world::with_test_world(&[], |world, _| {
            let mut world = world.clone();
            world.player = Some(crate::mobs::world::test_player(Vec3::new(1.0, 0.0, 0.0)));
            world.difficulty = super::super::spawning::LocalDifficulty::new(0, 18000, 0, 0.0);
            let mut golem = mob(Species::IronGolem);
            golem.ai.target = Some(Who::Player);
            golem.extra.anger = 300;
            golem.ai.last_hurt_by = Some(Who::Player);
            let mut events = Vec::new();
            tick(
                &mut golem,
                &world,
                &mut crate::health::TestLevel::new(0).level(),
                &mut events,
            );
            assert_eq!(golem.ai.target, None);
            assert_eq!(golem.extra.anger, 0);
            assert_eq!(golem.ai.last_hurt_by, None);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, MobEvent::AttackPlayer(..)))
            );
        });
    }

    /// The enderman's and zombified piglin's persistent anger is the target
    /// they hold: a dying player they are angry at takes it (and the last
    /// hurt by) with them, as `NeutralMob.playerDied` does through
    /// `stopBeingAngry`.
    #[test]
    fn a_target_anger_at_the_dying_player_ends() {
        for species in [Species::Enderman, Species::ZombifiedPiglin] {
            let mut m = mob(species);
            m.ai.target = Some(Who::Player);
            m.ai.last_hurt_by = Some(Who::Player);
            forgive_dead_player(&mut m);
            assert_eq!(m.ai.target, None, "{species:?}");
            assert_eq!(m.ai.last_hurt_by, None, "{species:?}");
            // Anger at something else is not the dying player's to end.
            let mut m = mob(species);
            m.ai.target = Some(Who::Mob(7));
            forgive_dead_player(&mut m);
            assert_eq!(m.ai.target, Some(Who::Mob(7)), "{species:?}");
        }
    }

    /// The iron golem's anger is a timer and its target: the dying player
    /// it is angry at ends both (`NeutralMob.playerDied`), even once the golem
    /// has lost sight of them, and anger at another mob stays.
    #[test]
    fn an_iron_golems_timer_and_target_end_together() {
        for target in [Some(Who::Player), None] {
            let mut golem = mob(Species::IronGolem);
            golem.extra.anger = 300;
            golem.extra.angry_at = Some(Who::Player);
            golem.ai.target = target;
            golem.ai.last_hurt_by = Some(Who::Player);
            forgive_dead_player(&mut golem);
            assert_eq!(golem.extra.anger, 0, "{target:?}");
            assert_eq!(golem.extra.angry_at, None, "{target:?}");
            assert_eq!(golem.ai.target, None, "{target:?}");
            assert_eq!(golem.ai.last_hurt_by, None, "{target:?}");
        }

        let mut golem = mob(Species::IronGolem);
        golem.extra.anger = 300;
        golem.extra.angry_at = Some(Who::Mob(7));
        golem.ai.target = Some(Who::Mob(7));
        forgive_dead_player(&mut golem);
        assert_eq!(golem.extra.anger, 300, "anger at another mob stays");
        assert_eq!(golem.ai.target, Some(Who::Mob(7)));
    }
}

#[cfg(test)]
mod endermite_goal_tests {
    use super::*;
    use crate::mobs::world::with_test_world;

}

#[cfg(test)]
mod villager_ambient_tests {
    use super::*;
    use crate::mobs::world::with_test_world;


}

#[cfg(test)]
mod fox_screech_tests {
    use super::*;
    use crate::mobs::PlayerView;
    use crate::mobs::world::with_test_world;

    fn player_at(x: f64) -> PlayerView {
        crate::mobs::world::test_player(Vec3::new(x, 64.0, 0.5))
    }


    /// `Fox.getAmbientSound` and `playAmbientSound`: at night with nobody
    /// within 16 blocks, about one call in ten screeches at volume 2; by day,
    /// in the End, or with a player near, never.
    #[test]
    #[ignore = "daily"]
    fn a_fox_screeches_only_at_night_with_no_player_near() {
        assert!(
            with_test_world(&[], |w, types| {
                let mut night = w.clone();
                night.daylight = false;
                let alone = sounds(&night, types);
                assert_eq!(alone.len(), 200);
                let screeches: Vec<_> = alone
                    .iter()
                    .filter(|s| s.0 == "entity.fox.screech")
                    .collect();
                assert!(!screeches.is_empty(), "no screech in 200 night calls");
                assert!(screeches.len() < 60, "{} screeches", screeches.len());
                // Played by playAmbientSound itself at volume 2...
                assert!(screeches.iter().any(|s| s.1 == 2.0));
                // ...or picked again through Mob.playAmbientSound (makeSound, volume 1).
                assert!(screeches.iter().all(|s| s.1 == 2.0 || s.1 == 1.0));
                assert!(
                    alone
                        .iter()
                        .all(|s| s.0 == "entity.fox.screech" || s.0 == "entity.fox.ambient")
                );

                let by_day = sounds(w, types);
                assert!(by_day.iter().all(|s| s.0 == "entity.fox.ambient"));

                let mut end = night.clone();
                end.daylight = true;
                end.dimension_type =
                    horizon_world::dimension_type::DimensionFacts::vanilla("minecraft:the_end")
                        .unwrap();
                assert!(
                    sounds(&end, types)
                        .iter()
                        .any(|s| s.0 == "entity.fox.screech")
                );

                let mut near = night.clone();
                near.player = Some(player_at(16.0));
                assert!(
                    sounds(&near, types)
                        .iter()
                        .all(|s| s.0 == "entity.fox.ambient")
                );

                let mut far = night.clone();
                far.player = Some(player_at(18.0));
                assert!(
                    sounds(&far, types)
                        .iter()
                        .any(|s| s.0 == "entity.fox.screech")
                );

                let mut spectator = night.clone();
                let mut p = player_at(2.0);
                p.spectator = true;
                spectator.player = Some(p);
                assert!(
                    sounds(&spectator, types)
                        .iter()
                        .any(|s| s.0 == "entity.fox.screech")
                );
            }),
            "vanilla data is missing"
        );
    }
}

#[cfg(test)]
mod age_up_tests {
    use super::*;
    use std::sync::Arc;




    /// `AgeableMob.ageUp` ends in `setAge`, and `setAge` is where
    /// `Goat.ageBoundaryReached` sets the attack damage to 2.0 for an adult:
    /// feeding a baby goat its last two meals makes it a 2.0 goat, and the
    /// meal that leaves it one tick short keeps the baby's 1.0.
    #[test]
    fn feeding_a_baby_goat_past_its_boundary_sets_the_adult_attack_damage() {
        let mut m = baby_goat(-21);
        assert_eq!(m.attrs.attack_damage, 1.0, "a newborn goat hits for 1");

        // 20 ticks of growth: age -1, still a baby.
        age_up(&mut m, 1, true);
        assert!(m.is_baby());
        assert_eq!(m.attrs.attack_damage, 1.0);

        // The next 20 ticks cross the boundary.
        age_up(&mut m, 1, true);
        assert!(!m.is_baby());
        assert_eq!(m.attrs.attack_damage, 2.0);
    }

}

#[cfg(test)]
mod in_wall_tests {
    use super::{Aabb, ShapeOp, Vec3, VoxelShape, in_wall};

    #[test]
    fn a_suffocating_partial_shape_must_reach_the_eye() {
        let slab = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))).unwrap();
        let intersects = |x, y, z, eye: &Aabb| {
            (x, y, z) == (0, 0, 0)
                && slab
                    .join_is_not_empty(&VoxelShape::from_box(*eye).unwrap(), ShapeOp::AND)
                    .unwrap()
        };
        assert!(in_wall(0.6, [0.5, 0.25, 0.5], intersects));
        assert!(!in_wall(0.6, [0.5, 0.75, 0.5], intersects));
    }

    /// `Entity.isInWall` has no width guard (#5872): a hoglin (1.3965 wide)
    /// whose eye is in a solid block, or whose 1.117-wide eye box reaches a
    /// neighbouring wall column, takes in-wall damage like a narrow mob.
    #[test]
    fn wide_mobs_suffocate_by_the_eye_box() {
        let wall = |x: i32, y: i32, z: i32, eye: &Aabb| {
            x >= 0
                && VoxelShape::block()
                    .moved(Vec3::new(f64::from(x), f64::from(y), f64::from(z)))
                    .join_is_not_empty(&VoxelShape::from_box(*eye).unwrap(), ShapeOp::AND)
                    .unwrap()
        };
        // Eye inside the wall: hoglin and zombie (0.6) alike.
        assert!(in_wall(1.3965, [0.5, 64.5, 0.5], wall));
        assert!(in_wall(0.6, [0.5, 64.5, 0.5], wall));
        // Eye in air at x=-0.2: the zombie's 0.48 box (-0.44..0.04) clips
        // x=0, and the hoglin's wider box does too.
        assert!(in_wall(0.6, [-0.2, 64.5, 0.5], wall));
        // At x=-0.3 the zombie's box (-0.54..-0.06) clears the wall; the
        // hoglin's half-width 0.5586 (-0.8586..0.2586) does not.
        assert!(!in_wall(0.6, [-0.3, 64.5, 0.5], wall));
        assert!(in_wall(1.3965, [-0.3, 64.5, 0.5], wall));
        // Open air.
        assert!(!in_wall(1.3965, [-3.0, 64.5, 0.5], wall));
        // A box edge exactly on the wall face only touches it.
        assert!(!in_wall(1.0, [-0.4, 64.5, 0.5], wall));
    }
}

#[cfg(test)]
mod burn_and_rose_tests {
    use super::*;
    use horizon_world::registry::BlockRegistry;
    use std::sync::Arc;

    fn build_with(block: &'static str) -> impl FnOnce(&mut crate::world::World, &BlockRegistry) {
        move |world, registry| {
            let stone = registry.default_state("minecraft:stone").unwrap();
            let state = registry.default_state(block).unwrap();
            for cx in -1..1 {
                for cz in -1..1 {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
            }
            for x in -4..4 {
                for z in -4..4 {
                    world.set(x, 63, z, stone).unwrap();
                }
            }
            world.set(0, 64, 0, state).unwrap();
        }
    }




    /// `PowderSnowBlock.entityInside`: a mob that is already burning destroys
    /// the powder snow (`mobGriefing`) and is put out, once, and then no longer
    /// catches fire there in daylight.
    #[test]
    #[ignore = "daily"]
    fn a_burning_zombie_in_powder_snow_breaks_the_block_and_is_put_out() {
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            build_with("minecraft:powder_snow"),
            |w, types| {
                let mut day = w.clone();
                day.monsters_burn = true;
                let mut m = zombie(types);
                m.ignite_for_ticks(100);
                assert!(m.is_on_fire());
                let (mut lr, mut events) = (crate::health::TestLevel::new(7), Vec::new());
                assert!(tick(&mut m, &day, &mut lr.level(), &mut events));
                let broken: Vec<([i32; 3], bool)> = events
                    .iter()
                    .filter_map(|e| match e {
                        MobEvent::BreakBlock { pos, drop, .. } => Some((*pos, *drop)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(broken, vec![([0, 64, 0], false)], "the powder snow breaks");
                assert_eq!(m.fire_ticks, 0, "and puts the zombie out");
                events.clear();
                for _ in 0..200 {
                    tick(&mut m, &day, &mut lr.level(), &mut events);
                }
                assert!(!m.removed, "the zombie stays");
                assert_eq!(m.fire_ticks, 0, "no sun burn while it stands in it");
                assert!(
                    !events
                        .iter()
                        .any(|e| matches!(e, MobEvent::BreakBlock { .. })),
                    "nothing left to destroy"
                );
            },
        );
        assert!(ran, "vanilla data is missing");
    }

    /// `WitherRoseBlock.entityInside` tests the world difficulty and
    /// `isInvulnerableTo(wither)`, not the regional difficulty of a fresh
    /// chunk and not the hit timer: a golem that was just hurt still gets it.
    #[test]
    #[ignore = "daily"]
    fn wither_rose_applies_wither_whatever_the_hit_timer_and_local_difficulty() {
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            build_with("minecraft:wither_rose"),
            |w, types| {
                let mut m = golem(types);
                m.invulnerable = 15;
                assert_eq!(w.difficulty.base, 2);
                flower_contact_effects(&mut m, w);
                assert!(m.has_effect("minecraft:wither"), "normal difficulty");

                let mut peaceful = w.clone();
                peaceful.difficulty = crate::mobs::spawning::LocalDifficulty::new(0, 18000, 0, 0.0);
                let mut m = golem(types);
                flower_contact_effects(&mut m, &peaceful);
                assert!(!m.has_effect("minecraft:wither"), "peaceful");
            },
        );
        assert!(ran, "vanilla data is missing");
    }

    /// `Entity.checkInsideBlocks` runs `WitherRoseBlock.entityInside` for every
    /// block the box sweeps between the tick's start and end: a zombie falling
    /// three blocks in one tick (about terminal speed) passes through the
    /// rose's cell between the two boxes and gets wither, while one standing
    /// beside it does not.
    #[test]
    #[ignore = "daily"]
    fn a_mob_falling_past_a_wither_rose_gets_wither_and_one_beside_it_does_not() {
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            build_with("minecraft:wither_rose"),
            |w, types| {
                // Boxes 65..67 and 62..64: the rose's cell 64 is between them.
                let mut falling = zombie(types);
                falling.previous = Vec3::new(0.5, 65.0, 0.5);
                falling.position = Vec3::new(0.5, 62.0, 0.5);
                flower_contact_effects(&mut falling, w);
                assert!(
                    falling.has_effect("minecraft:wither"),
                    "inside the rose on the way past it"
                );

                let mut beside = zombie(types);
                beside.previous = Vec3::new(3.5, 64.0, 0.5);
                beside.position = Vec3::new(3.5, 64.0, 0.5);
                flower_contact_effects(&mut beside, w);
                assert!(!beside.has_effect("minecraft:wither"), "beside it");
            },
        );
        assert!(ran, "vanilla data is missing");
    }

    /// `applyEffectsFromBlocks` runs after `baseTick` counted the fire and
    /// the invulnerable time down: standing in lava a golem reads Fire 300
    /// (not 299) after its first tick, and the hit that tick starts has its
    /// full invulnerable time.
    #[test]
    #[ignore = "daily"]
    fn lava_ignites_to_the_full_time_and_hits_after_the_timers_count_down() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let ran = crate::mobs::world::with_built_test_world(
            &[],
            build_with("minecraft:lava"),
            |w, types| {
                let mut m = golem(types);
                let mut events = Vec::new();
                let start = m.health;
                base_tick(&mut m, w, level, &mut events);
                assert_eq!(m.health, start - 4.0, "one lava hit");
                assert_eq!(m.fire_ticks, 300, "fire after the first tick");
                assert_eq!(m.invulnerable, 20, "the hit's own invulnerable time");
                for _ in 0..10 {
                    base_tick(&mut m, w, level, &mut events);
                }
                assert_eq!(m.health, start - 8.0, "second hit on tick 11");
                assert_eq!(m.fire_ticks, 300);
            },
        );
        assert!(ran, "the vanilla test resources must be available");
    }
}

#[cfg(test)]
mod wither_block_slowdown_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world;
    use horizon_world::registry::BlockRegistry;
    use std::sync::Arc;

    fn build_with(block: &'static str) -> impl FnOnce(&mut crate::world::World, &BlockRegistry) {
        move |world, registry| {
            let stone = registry.default_state("minecraft:stone").unwrap();
            let state = registry.default_state(block).unwrap();
            for cx in -1..1 {
                for cz in -1..1 {
                    world
                        .chunks
                        .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                }
            }
            for x in -4..4 {
                for z in -4..4 {
                    world.set(x, 63, z, stone).unwrap();
                }
            }
            world.set(0, 64, 0, state).unwrap();
        }
    }


    /// `SweetBerryBushBlock.entityInside` -> `makeStuckInBlock`: a regular mob
    /// (e.g. cow) is slowed down and its fall distance is reset to 0.
    #[test]
    #[ignore = "daily"]
    fn cow_gets_stuck_in_sweet_berry_bush() {
        let ran =
            with_built_test_world(&[], build_with("minecraft:sweet_berry_bush"), |w, types| {
                let mut m = mob(types, "minecraft:cow");
                m.fall_distance = 4.5;
                m.stuck_multiplier = Vec3::ZERO;
                let mut test_level = crate::health::TestLevel::new(42);
                let mut level = test_level.level();
                let mut events = Vec::new();
                effects_from_blocks(&mut m, w, &mut level, &mut events);
                assert_eq!(m.fall_distance, 0.0, "cow fall distance reset");
                assert_eq!(
                    m.stuck_multiplier,
                    Vec3::new(f64::from(0.8_f32), 0.75, f64::from(0.8_f32)),
                    "cow stuck multiplier applied"
                );
            });
        assert!(ran, "vanilla data is missing");
    }

    /// `PowderSnowBlock.entityInside` -> `makeStuckInBlock`: a regular mob
    /// (e.g. cow) in powder snow is slowed down and its fall distance is reset to 0.
    #[test]
    #[ignore = "daily"]
    fn cow_gets_stuck_in_powder_snow() {
        let ran = with_built_test_world(&[], build_with("minecraft:powder_snow"), |w, types| {
            let mut m = mob(types, "minecraft:cow");
            m.fall_distance = 4.5;
            m.stuck_multiplier = Vec3::ZERO;
            let mut test_level = crate::health::TestLevel::new(42);
            let mut level = test_level.level();
            let mut events = Vec::new();
            effects_from_blocks(&mut m, w, &mut level, &mut events);
            assert_eq!(m.fall_distance, 0.0, "cow fall distance reset");
            assert_eq!(
                m.stuck_multiplier,
                Vec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32)),
                "cow powder snow stuck multiplier applied"
            );
        });
        assert!(ran, "vanilla data is missing");
    }

    /// `WitherBoss.makeStuckInBlock` is a NO-OP: a wither inside sweet berry bush
    /// does not receive stuck multiplier and preserves its fall distance.
    #[test]
    #[ignore = "daily"]
    fn wither_does_not_get_stuck_in_sweet_berry_bush() {
        let ran =
            with_built_test_world(&[], build_with("minecraft:sweet_berry_bush"), |w, types| {
                let mut m = mob(types, "minecraft:wither");
                m.fall_distance = 4.5;
                m.stuck_multiplier = Vec3::ZERO;
                let mut test_level = crate::health::TestLevel::new(42);
                let mut level = test_level.level();
                let mut events = Vec::new();
                effects_from_blocks(&mut m, w, &mut level, &mut events);
                assert_eq!(m.fall_distance, 4.5, "wither fall distance preserved");
                assert_eq!(m.stuck_multiplier, Vec3::ZERO, "wither not slowed");
            });
        assert!(ran, "vanilla data is missing");
    }

    /// `WitherBoss.makeStuckInBlock` is a NO-OP: a wither inside powder snow
    /// does not receive stuck multiplier and preserves its fall distance.
    #[test]
    #[ignore = "daily"]
    fn wither_does_not_get_stuck_in_powder_snow() {
        let ran = with_built_test_world(&[], build_with("minecraft:powder_snow"), |w, types| {
            let mut m = mob(types, "minecraft:wither");
            m.fall_distance = 4.5;
            m.stuck_multiplier = Vec3::ZERO;
            let mut test_level = crate::health::TestLevel::new(42);
            let mut level = test_level.level();
            let mut events = Vec::new();
            effects_from_blocks(&mut m, w, &mut level, &mut events);
            assert_eq!(m.fall_distance, 4.5, "wither fall distance preserved");
            assert_eq!(m.stuck_multiplier, Vec3::ZERO, "wither not slowed");
        });
        assert!(ran, "vanilla data is missing");
    }

    /// `SweetBerryBushBlock.entityInside`: Wither is NOT exempt from damage
    /// when moving through a mature bush (age > 0).
    #[test]
    #[ignore = "daily"]
    fn wither_takes_damage_when_moving_in_mature_sweet_berry_bush() {
        let ran = with_built_test_world(
            &[],
            |world, registry| {
                world.chunks.insert(
                    (0, 0),
                    std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
                );
                let stone = registry.default_state("minecraft:stone").unwrap();
                let default_bush = registry
                    .default_state("minecraft:sweet_berry_bush")
                    .unwrap();
                let mut state = registry.state(default_bush).unwrap().clone();
                state.properties.insert("age".into(), "3".into());
                let mature_bush = registry.id(&state).unwrap();
                for cx in -1..1 {
                    for cz in -1..1 {
                        world
                            .chunks
                            .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
                    }
                }
                for x in -4..4 {
                    for z in -4..4 {
                        world.set(x, 63, z, stone).unwrap();
                    }
                }
                world.set(0, 64, 0, mature_bush).unwrap();
            },
            |w, types| {
                let mut m = mob(types, "minecraft:wither");
                let initial_health = m.health;
                m.previous = Vec3::new(0.5, 64.0, 0.5);
                m.position = Vec3::new(0.52, 64.0, 0.5);
                let mut test_level = crate::health::TestLevel::new(42);
                let mut level = test_level.level();
                let mut events = Vec::new();
                effects_from_blocks(&mut m, w, &mut level, &mut events);
                assert_eq!(
                    m.health,
                    initial_health - 0.86,
                    "wither takes sweet berry bush damage through armor"
                );
                assert_eq!(m.stuck_multiplier, Vec3::ZERO, "wither still not slowed");
            },
        );
        assert!(ran, "vanilla data is missing");
    }
}

#[cfg(test)]
mod silverfish_walk_target_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world;

}

#[cfg(test)]
mod iron_golem_push_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use std::sync::Arc;


}

#[cfg(test)]
mod allay_healing_tests {
    use super::*;
    use std::sync::Arc;

}

#[cfg(test)]
mod ravager_noai_tests {
    use super::*;
    use std::sync::Arc;

}

#[cfg(test)]
mod air_tests {
    use super::*;



    /// `Dolphin.handleAirSupply` is empty and the dolphin isn't in
    /// `#can_breathe_under_water`: under water it loses a breath a tick from
    /// its 4800, as `LivingEntity.baseTick` takes it, so it must surface.
    #[test]
    #[ignore = "daily"]
    fn a_dolphin_holds_its_breath_under_water() {
        let Some(air) = air_after("minecraft:dolphin", 4800, 10) else {
            return;
        };
        assert_eq!(air, 4790);
    }



    /// A copper golem is in `#can_breathe_under_water`: its air stays full.
    #[test]
    #[ignore = "daily"]
    fn a_copper_golem_breathes_under_water() {
        let Some(air) = air_after("minecraft:copper_golem", 300, 10) else {
            return;
        };
        assert_eq!(air, 300);
    }
}

#[cfg(test)]
mod dolphin_noai_air_tests {
    use super::*;
    use std::sync::Arc;


    /// `Dolphin.tick` restores max air after super.tick for a NoAI dolphin,
    /// even when that superclass tick refilled or consumed loaded air.
    #[test]
    #[ignore = "daily"]
    fn noai_dolphins_restore_air_after_the_full_mob_tick() {
        for water in [false, true] {
            for initial in [4800, 100] {
                check(water, true, initial, [4800, 4800]);
            }
        }
    }

    /// The NoAI exception must not suppress normal underwater consumption.
    #[test]
    #[ignore = "daily"]
    fn normal_dolphins_keep_their_underwater_countdown() {
        check(true, false, 4800, [4799, 4798]);
    }
}

#[cfg(test)]
mod nautilus_noai_air_tests {
    use super::*;
    use std::sync::Arc;


    /// `Nautilus.baseTick`: NoAI skips only handleAirSupply; super.baseTick
    /// still refills a breathing mob by four, capped at300.
    #[test]
    #[ignore = "daily"]
    fn noai_nautilus_keeps_superclass_refill_in_both_media() {
        for water in [false, true] {
            check(water, true, 300, [300, 300]);
            check(water, true, 100, [104, 108]);
        }
    }

    /// With AI, handleAirSupply still uses the pre-super air on land and
    /// writes300 in water. The NoAI exception must not alter that policy.
    #[test]
    #[ignore = "daily"]
    fn normal_nautilus_keeps_its_custom_handler() {
        check(false, false, 300, [299, 298]);
        check(false, false, 100, [99, 98]);
        check(true, false, 300, [300, 300]);
        check(true, false, 100, [300, 300]);
    }
}

#[cfg(test)]
mod freeze_tests {
    use super::*;
    use std::sync::Arc;

}
#[cfg(test)]
mod happy_ghast_ambient_tests {
    use super::*;
    use std::sync::Arc;

}

#[cfg(test)]
mod hidden_speed_causal_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn zero_active_never_promotes_and_infinite_fallback_counts_once() {
        let mut m = mob();
        m.add_effect("speed", 200, 0);
        m.add_effect("speed", 0, 1);
        assert_eq!(m.extra.effect_chains.len(), 1);
        tick_effects(
            &mut m,
            crate::test_resources::server_loop_context(),
            &mut crate::health::TestLevel::new(0).level(),
            None,
        );
        assert!(m.extra.effects.is_empty() && m.extra.effect_chains.is_empty());
        m.add_effect("speed", -1, 0);
        m.add_effect("speed", 2, 1);
        tick_effects(
            &mut m,
            crate::test_resources::server_loop_context(),
            &mut crate::health::TestLevel::new(0).level(),
            None,
        );
        assert_eq!(
            m.extra
                .effects
                .iter()
                .map(|e| (e.id, e.duration, e.amplifier))
                .collect::<Vec<_>>(),
            [("minecraft:speed", 1, 1)]
        );
        assert_eq!(m.extra.effect_chains[0].duration, -1);
        tick_effects(
            &mut m,
            crate::test_resources::server_loop_context(),
            &mut crate::health::TestLevel::new(0).level(),
            None,
        );
        assert_eq!(
            m.extra
                .effects
                .iter()
                .map(|e| (e.id, e.duration, e.amplifier))
                .collect::<Vec<_>>(),
            [("minecraft:speed", -1, 0)]
        );
        tick_effects(
            &mut m,
            crate::test_resources::server_loop_context(),
            &mut crate::health::TestLevel::new(0).level(),
            None,
        );
        assert_eq!(
            m.extra
                .effects
                .iter()
                .map(|e| (e.id, e.duration, e.amplifier))
                .collect::<Vec<_>>(),
            [("minecraft:speed", -1, 0)]
        );
        assert!(m.extra.effect_chains.is_empty());
    }
}

#[cfg(test)]
mod trader_llama_no_ai_tests {
    use super::*;
    use std::sync::Arc;

}


#[cfg(test)]
mod hidden_absorption_promotion_tests {
    use super::*;
    use std::sync::Arc;

}

#[cfg(test)]
mod baby_speed_tests {
    use super::*;
    use crate::mobs::{finalize, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    /// #11994: movement reads the transient `minecraft:baby` modifier itself:
    /// `Zombie.SPEED_MODIFIER_BABY` +0.5 and `Piglin.SPEED_MODIFIER_BABY` +0.2
    /// (`ADD_MULTIPLIED_BASE`), and none once `/attribute ... modifier remove`
    /// took it away, until the next `setBaby`.
    #[test]
    fn a_babys_speed_follows_its_baby_modifier() {
        let mut zombie = baby(Species::Zombie, "minecraft:zombie");
        assert_eq!(speed_bonus(&zombie), 1.5);
        zombie
            .extra
            .attributes
            .get_or_insert_default()
            .baby_speed_removed = true;
        assert_eq!(speed_bonus(&zombie), 1.0, "removed with /attribute");
        finalize::set_baby(&mut zombie, true);
        assert_eq!(speed_bonus(&zombie), 1.5, "setBaby adds it back");
        // The piglin's is 0.2F widened to a double, not the zombie's 0.5.
        let piglin = baby(Species::Piglin, "minecraft:piglin");
        assert_eq!(speed_bonus(&piglin), 1.0 + f64::from(0.2f32));
        let mut adult = baby(Species::Zombie, "minecraft:zombie");
        finalize::set_baby(&mut adult, false);
        assert_eq!(speed_bonus(&adult), 1.0);
    }
}

#[cfg(test)]
mod walk_target_tests {
    use super::*;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
