//! The warden's `Brain` (`Warden.BRAIN_PROVIDER` and `WardenAi`), ported
//! behaviour for behaviour on the villager brain's machinery (`Slot`
//! memories with their time to live, `Tracker`s, `WalkTarget`s and the
//! nearest-visible list): the two sensors and their scan phases, the core,
//! emerge, dig, idle, roar, fight, investigate and sniff activities, and their
//! behaviours started and ticked in vanilla's order (priority, then the
//! activity's slot in `Brain.availableBehaviorsByPriority`'s hash map, then
//! the order the activity lists them in).
//!
//! The level random (`ServerLevel.getRandom`) draws every `Behavior`'s
//! duration and the idle `DoNothing`, the sniff cooldown and the spot near a
//! disturbance; the warden's own random drives strolls, sounds and swimming.
//! `RunOne` shuffles with a random of its own, seeded with the brain's
//! shuffle seed.
use super::{
    MobEvent, MobWorld, TargetChange, Who,
    brain::Timed,
    goals::Cx,
    path::Path,
    villager_brain::{Slot, Tracker, Visible, WalkTarget, compute_path, reached},
    warden::{self, Pose},
};
use crate::health::DamageLevel;
use horizon_core::{
    geometry::Vec3,
    random::{EntityRandom, LegacyRandom, RandomSource},
};

/// `WardenAi` timings.
pub const EMERGE_DURATION: i32 = 134;
pub const ROAR_DURATION: i32 = 84;
const SNIFFING_DURATION: i32 = 84;
const DIGGING_DURATION: i32 = 100;
pub const DIGGING_COOLDOWN: i64 = 1200;
const MELEE_ATTACK_COOLDOWN: i64 = 18;
const SONIC_DURATION: i32 = 60;
const SONIC_SOUND_DELAY: i64 = 34;
const ROAR_SOUND_DELAY: i64 = 25;
const DISTURBANCE_EXPIRY: i64 = 100;
/// `Warden.createAttributes`: the follow range the fight's look target uses.
const FOLLOW_RANGE: f64 = 24.0;

/// `Activity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Core,
    Emerge,
    Dig,
    Idle,
    Roar,
    Fight,
    Investigate,
    Sniff,
}
impl Act {
    pub fn name(self) -> &'static str {
        match self {
            Act::Core => "core",
            Act::Emerge => "emerge",
            Act::Dig => "dig",
            Act::Idle => "idle",
            Act::Roar => "roar",
            Act::Fight => "fight",
            Act::Investigate => "investigate",
            Act::Sniff => "sniff",
        }
    }
    /// The bucket of `HashMap<Activity, ...>` (16 buckets, the hash of the
    /// name); roar and investigate share one but never a priority.
    fn slot(self) -> u8 {
        match self {
            Act::Core => 1,
            Act::Dig => 3,
            Act::Idle => 5,
            Act::Sniff => 6,
            Act::Emerge => 10,
            Act::Roar | Act::Investigate => 11,
            Act::Fight => 14,
        }
    }
    /// Its place in `getActivities` (insertion order inside a bucket).
    fn order(self) -> u8 {
        self as u8
    }
}

/// The memories the warden's brain registers.
#[derive(Clone, Debug, Default)]
pub struct Memories {
    pub nearest_players: Slot<Vec<Who>>,
    pub nearest_visible_player: Slot<Who>,
    pub nearest_visible_targetable_player: Slot<Who>,
    pub nearest_visible_targetable_players: Slot<Vec<Who>>,
    pub mobs: Slot<Vec<Who>>,
    pub visible_mobs: Slot<Visible>,
    pub nearest_attackable: Slot<Who>,
    pub look_target: Slot<Tracker>,
    pub walk_target: Slot<WalkTarget>,
    pub path: Slot<Path>,
    pub cant_reach_walk_target_since: Slot<i64>,
    pub attack_target: Slot<Who>,
    pub attack_cooling_down: Slot<bool>,
    pub roar_target: Slot<Who>,
    pub disturbance_location: Slot<[i32; 3]>,
    pub recent_projectile: Slot<()>,
    pub is_sniffing: Slot<()>,
    pub is_emerging: Slot<()>,
    pub roar_sound_delay: Slot<()>,
    pub dig_cooldown: Slot<()>,
    pub roar_sound_cooldown: Slot<()>,
    pub sniff_cooldown: Slot<()>,
    pub touch_cooldown: Slot<()>,
    pub vibration_cooldown: Slot<()>,
    pub sonic_boom_cooldown: Slot<()>,
    pub sonic_boom_sound_cooldown: Slot<()>,
    pub sonic_boom_sound_delay: Slot<()>,
}
impl Memories {
    fn tick(&mut self) {
        self.nearest_players.tick();
        self.nearest_visible_player.tick();
        self.nearest_visible_targetable_player.tick();
        self.nearest_visible_targetable_players.tick();
        self.mobs.tick();
        self.visible_mobs.tick();
        self.nearest_attackable.tick();
        self.look_target.tick();
        self.walk_target.tick();
        self.path.tick();
        self.cant_reach_walk_target_since.tick();
        self.attack_target.tick();
        self.attack_cooling_down.tick();
        self.roar_target.tick();
        self.disturbance_location.tick();
        self.recent_projectile.tick();
        self.is_sniffing.tick();
        self.is_emerging.tick();
        self.roar_sound_delay.tick();
        self.dig_cooldown.tick();
        self.roar_sound_cooldown.tick();
        self.sniff_cooldown.tick();
        self.touch_cooldown.tick();
        self.vibration_cooldown.tick();
        self.sonic_boom_cooldown.tick();
        self.sonic_boom_sound_cooldown.tick();
        self.sonic_boom_sound_delay.tick();
    }
}

/// `RunOne`'s `ShufflingList` over a stroll (weight 2) and `DoNothing` (1).
#[derive(Clone, Debug)]
struct Gate {
    running: bool,
    random: LegacyRandom,
    kids: Vec<(B, i32, f64)>,
}

/// One behaviour of the warden's activities.
#[derive(Clone, Debug)]
enum B {
    Swim(Timed),
    SetWardenLookTarget(bool),
    LookAtTargetSink(Timed),
    MoveToTargetSink {
        t: Timed,
        cooldown: i32,
        path: Option<Path>,
        last_target: Option<[i32; 3]>,
        speed: f32,
    },
    Emerging(Timed),
    ForceUnmount(Timed),
    Digging(Timed),
    SetRoarTarget(bool),
    TryToSniff(bool),
    RunOne(Box<Gate>),
    Stroll(bool),
    DoNothing(Timed),
    GoToDisturbance(bool),
    Sniffing(Timed),
    Roar(Timed),
    DigCooldownSetter(bool),
    StopAttackingIfTargetInvalid(bool),
    SetEntityLookTarget(bool),
    SetWalkTargetFromAttackTarget(bool),
    SonicBoom(Timed),
    MeleeAttack(bool),
}

#[derive(Clone, Debug)]
struct Entry {
    prio: i32,
    act: Act,
    b: B,
}

/// The warden's brain.
#[derive(Clone, Debug)]
pub struct Brain {
    pub mem: Memories,
    /// `NEAREST_PLAYERS` and `WARDEN_ENTITY_SENSOR`: ticks to the next scan.
    sensors: [i64; 2],
    /// The active non-core activity (`CORE` is always active).
    pub activity: Act,
    list: Vec<Entry>,
    pub shuffle_seed: i64,
}
impl Default for Brain {
    fn default() -> Self {
        // No mob id: nothing attaches (a default brain is the empty shell a
        // warden's state starts from; `init_brain` builds the real one).
        Self::new("", 0)
    }
}
impl Brain {
    pub fn new(_mob_id: &str, shuffle_seed: i64) -> Self {
        let list = activities(shuffle_seed);
        Self {
            mem: Memories::default(),
            sensors: [0; 2],
            activity: Act::Idle,
            list,
            shuffle_seed,
        }
    }
    pub fn is_active(&self, a: Act) -> bool {
        a == Act::Core || a == self.activity
    }
    fn requirements_met(&self, a: Act) -> bool {
        let m = &self.mem;
        match a {
            Act::Emerge => m.is_emerging.present(),
            Act::Dig => !m.roar_target.present() && !m.dig_cooldown.present(),
            Act::Roar => m.roar_target.present(),
            Act::Fight => m.attack_target.present(),
            Act::Investigate => m.disturbance_location.present(),
            Act::Sniff => m.is_sniffing.present(),
            Act::Core | Act::Idle => true,
        }
    }
    /// `setActiveActivity`: leaving an activity erases the memory it needed.
    fn set_active(&mut self, a: Act) {
        if self.is_active(a) {
            return;
        }
        let m = &mut self.mem;
        match self.activity {
            Act::Emerge => m.is_emerging.clear(),
            Act::Roar => m.roar_target.clear(),
            Act::Fight => m.attack_target.clear(),
            Act::Investigate => m.disturbance_location.clear(),
            Act::Sniff => m.is_sniffing.clear(),
            Act::Core | Act::Dig | Act::Idle => {}
        }
        self.activity = a;
    }
    /// `WardenAi.updateActivity`: `setActiveActivityToFirstValid`.
    pub(super) fn will_clear_attack_target(&self) -> bool {
        self.activity == Act::Fight
            && self.mem.attack_target.present()
            && [
                Act::Emerge,
                Act::Dig,
                Act::Roar,
                Act::Fight,
                Act::Investigate,
                Act::Sniff,
                Act::Idle,
            ]
            .into_iter()
            .find(|&activity| self.requirements_met(activity))
            .is_some_and(|activity| activity != Act::Fight)
    }

    pub fn update_activity(&mut self) {
        for a in [
            Act::Emerge,
            Act::Dig,
            Act::Roar,
            Act::Fight,
            Act::Investigate,
            Act::Sniff,
            Act::Idle,
        ] {
            if self.requirements_met(a) {
                self.set_active(a);
                break;
            }
        }
    }
    /// `Sensor.randomlyDelayStart` for both sensors, from the warden's random.
    pub fn delay_sensors(&mut self, random: &mut EntityRandom) {
        for s in &mut self.sensors {
            *s = i64::from(random.next_int_bound(20).unwrap_or(0));
        }
    }
}

/// `WardenAi.getActivities`, in the order `Brain.startEachNonRunningBehavior`
/// meets the behaviours.
fn activities(seed: i64) -> Vec<Entry> {
    let gate = Gate {
        running: false,
        random: LegacyRandom::new(seed),
        kids: vec![
            (B::Stroll(false), 2, 0.0),
            (B::DoNothing(Timed::new(30, 60)), 1, 0.0),
        ],
    };
    let mut list = vec![
        (0, Act::Core, B::Swim(Timed::new(60, 60))),
        (1, Act::Core, B::SetWardenLookTarget(false)),
        (2, Act::Core, B::LookAtTargetSink(Timed::new(45, 90))),
        (
            3,
            Act::Core,
            B::MoveToTargetSink {
                t: Timed::new(150, 250),
                cooldown: 0,
                path: None,
                last_target: None,
                speed: 0.0,
            },
        ),
        (
            5,
            Act::Emerge,
            B::Emerging(Timed::new(EMERGE_DURATION, EMERGE_DURATION)),
        ),
        (0, Act::Dig, B::ForceUnmount(Timed::new(60, 60))),
        (
            1,
            Act::Dig,
            B::Digging(Timed::new(DIGGING_DURATION, DIGGING_DURATION)),
        ),
        (10, Act::Idle, B::SetRoarTarget(false)),
        (11, Act::Idle, B::TryToSniff(false)),
        (12, Act::Idle, B::RunOne(Box::new(gate))),
        (
            10,
            Act::Roar,
            B::Roar(Timed::new(ROAR_DURATION, ROAR_DURATION)),
        ),
        (10, Act::Fight, B::DigCooldownSetter(false)),
        (11, Act::Fight, B::StopAttackingIfTargetInvalid(false)),
        (12, Act::Fight, B::SetEntityLookTarget(false)),
        (13, Act::Fight, B::SetWalkTargetFromAttackTarget(false)),
        (
            14,
            Act::Fight,
            B::SonicBoom(Timed::new(SONIC_DURATION, SONIC_DURATION)),
        ),
        (15, Act::Fight, B::MeleeAttack(false)),
        (5, Act::Investigate, B::SetRoarTarget(false)),
        (6, Act::Investigate, B::GoToDisturbance(false)),
        (5, Act::Sniff, B::SetRoarTarget(false)),
        (
            6,
            Act::Sniff,
            B::Sniffing(Timed::new(SNIFFING_DURATION, SNIFFING_DURATION)),
        ),
    ];
    // A stable sort keeps each activity's own order.
    list.sort_by_key(|(prio, act, _)| (*prio, act.slot(), act.order()));
    list.into_iter()
        .map(|(prio, act, b)| Entry { prio, act, b })
        .collect()
}

/// What behaviours read and emit during a brain tick.
pub struct Bx<'a, 'w, 'l> {
    pub w: &'a MobWorld<'w>,
    pub level: &'a mut DamageLevel<'l>,
    pub events: &'a mut Vec<MobEvent>,
    pub now: i64,
}



// ---- Sensors ------------------------------------------------------------


// ---- Shared warden actions ------------------------------------------------

fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn dist_sqr(a: [i32; 3], b: [i32; 3]) -> i64 {
    (0..3).map(|i| i64::from(a[i] - b[i]).pow(2)).sum()
}
fn same_path(a: Option<&Path>, b: Option<&Path>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.nodes == b.nodes && a.target == b.target,
        _ => false,
    }
}

// ---- Behaviours -----------------------------------------------------------

impl B {
    fn running(&self) -> bool {
        match self {
            B::Swim(t)
            | B::LookAtTargetSink(t)
            | B::Emerging(t)
            | B::ForceUnmount(t)
            | B::Digging(t)
            | B::DoNothing(t)
            | B::Sniffing(t)
            | B::Roar(t)
            | B::SonicBoom(t) => t.running,
            B::MoveToTargetSink { t, .. } => t.running,            B::RunOne(g) => g.running,
            B::SetWardenLookTarget(r)
            | B::SetRoarTarget(r)
            | B::TryToSniff(r)
            | B::Stroll(r)
            | B::GoToDisturbance(r)
            | B::DigCooldownSetter(r)
            | B::StopAttackingIfTargetInvalid(r)
            | B::SetEntityLookTarget(r)
            | B::SetWalkTargetFromAttackTarget(r)
            | B::MeleeAttack(r) => *r,
        }
    }



}

/// `OneShot.tryStart`: running when the trigger fires.
fn one(running: &mut bool, f: impl FnOnce() -> bool) -> bool {
    if f() {
        *running = true;
        true
    } else {
        false
    }
}
/// The `Cx` the mob library's attacks take.
pub(super) fn cx<'a, 'w, 'l>(
    w: &'a MobWorld<'w>,
    level: &'a mut DamageLevel<'l>,
    events: &'a mut Vec<MobEvent>,
) -> Cx<'a, 'w, 'l> {
    Cx { w, level, events }
}

// ---- Printing ---------------------------------------------------------------

fn who(w: Who) -> String {
    match w {
        Who::Player => "player".into(),
        Who::Mob(id) => format!("e{id}"),
    }
}
fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}
fn tracker(t: Tracker) -> String {
    match t {
        Tracker::Entity(e, _) => who(e),
        Tracker::Block(p) => pos(p),
        Tracker::Point(v) => pos([
            v[0].floor() as i32,
            v[1].floor() as i32,
            v[2].floor() as i32,
        ]),
    }
}

#[cfg(test)]
mod warden_owned_memory_tests {
    use super::*;
    use crate::entity_nbt::{mob_from_nbt, mob_to_nbt};
    use crate::mobs::MobType;
    use crate::save::{doubles, put};
    use horizon_core::nbt::{Compound, Tag};
    use horizon_world::registry::BlockRegistry;
    use std::{collections::HashMap, sync::Arc};
    fn string(s: &str) -> Tag {
        Tag::String(s.into())
    }
    use crate::mobs::{Species, world::with_built_test_world};

    fn types() -> HashMap<String, Arc<MobType>> {
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        let ty = Arc::make_mut(&mut kind);
        ty.id = "minecraft:warden".into();
        ty.species = Species::Warden;
        HashMap::from([("minecraft:warden".into(), kind)])
    }
    fn entry(ttl: Option<i64>) -> Tag {
        let mut entry = Tag::compound([("value", Tag::Compound(Compound::new()))]);
        if let Some(ttl) = ttl {
            put(&mut entry, "ttl", Tag::Long(ttl));
        }
        entry
    }
    fn input(no_ai: bool, form: u8) -> Tag {
        let mut raw = Tag::compound([
            ("id", string("minecraft:warden")),
            ("UUID", Tag::IntArray(vec![29, 0, 0, 1])),
            ("Pos", doubles(&[2.5, 3.0, 2.5])),
            ("Motion", doubles(&[0.0; 3])),
            ("Health", Tag::Float(10f32.to_bits())),
            ("NoAI", Tag::Byte(i8::from(no_ai))),
            ("NoGravity", Tag::Byte(1)),
            ("Tags", Tag::List(vec![string("keep")])),
            ("audit:keep", Tag::Int(42)),
        ]);
        if form != 0 {
            let memories = if form == 1 {
                Tag::Compound(Compound::new())
            } else {
                Tag::compound([
                    (
                        "minecraft:is_emerging",
                        entry(matches!(form, 2 | 4).then_some(2)),
                    ),
                    (
                        "minecraft:dig_cooldown",
                        entry(matches!(form, 2 | 4).then_some(if form == 4 { 17 } else { 1200 })),
                    ),
                    ("audit:unrelated", Tag::Int(7)),
                ])
            };
            put(
                &mut raw,
                "Brain",
                Tag::compound([("memories", memories), ("audit:brain", Tag::Int(19))]),
            );
        }
        raw
    }
    fn assert_saved(tag: &Tag, expected: [Option<i64>; 2]) {
        for (key, ttl) in ["minecraft:is_emerging", "minecraft:dig_cooldown"]
            .into_iter()
            .zip(expected)
        {
            let actual = tag
                .get("Brain")
                .and_then(|b| b.get("memories"))
                .and_then(|b| b.get(key));
            match ttl {
                None => assert!(actual.is_none(), "stale {key}: {actual:?}"),
                Some(ttl) => assert_eq!(actual, Some(&entry((ttl != i64::MAX).then_some(ttl)))),
            }
        }
    }
    #[test]
    fn decoded_owned_slots_and_no_raw_saves_are_actual_not_input_echoes() {
        let kinds = types();
        for no_ai in [false, true] {
            for (form, expected) in [
                (2, [Some(2), Some(1200)]),
                (4, [Some(2), Some(17)]),
                (0, [None, None]),
                (1, [None, None]),
                (3, [Some(i64::MAX), Some(i64::MAX)]),
            ] {
                let raw = input(no_ai, form);
                let m = mob_from_nbt(&raw, &kinds, 91, None).unwrap();
                assert_eq!(m.no_ai, no_ai);
                assert_eq!(slot_state(&m), expected, "form={form}");
                let mut stale = raw.clone();
                put(
                    &mut stale,
                    "Brain",
                    Tag::compound([
                        (
                            "memories",
                            Tag::compound([
                                ("minecraft:is_emerging", entry(Some(999))),
                                ("minecraft:dig_cooldown", entry(Some(999))),
                                ("audit:unrelated", Tag::Int(7)),
                            ]),
                        ),
                        ("audit:brain", Tag::Int(19)),
                    ]),
                );
                for source in [None, Some(&raw), Some(&stale)] {
                    let saved = mob_to_nbt(&m, [29, 0, 0, 1], source, None, |_| None);
                    assert_saved(&saved, expected);
                    if source.is_some() {
                        assert_eq!(saved.get("audit:keep"), Some(&Tag::Int(42)));
                        if form != 0 || source == Some(&stale) {
                            assert_eq!(
                                saved.get("Brain").and_then(|b| b.get("audit:brain")),
                                Some(&Tag::Int(19))
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn owned_memory_mutations_remove_stale_raw_without_replacing_the_brain() {
        let raw = input(true, 2);
        let mut m = mob_from_nbt(&raw, &types(), 92, None).unwrap();
        let listener = m.extra.warden.as_ref().unwrap().vibration.to_nbt();
        let mem = &mut m.extra.warden.as_mut().unwrap().brain.mem;
        mem.is_emerging.clear();
        mem.dig_cooldown.set_ttl((), 7);
        mem.sniff_cooldown.set_ttl((), 17);
        let saved = mob_to_nbt(&m, [29, 0, 0, 1], Some(&raw), None, |_| None);
        assert_saved(&saved, [None, Some(7)]);
        assert_eq!(
            saved.get("Brain").and_then(|b| b.get("audit:brain")),
            Some(&Tag::Int(19))
        );
        assert_eq!(
            saved
                .get("Brain")
                .and_then(|b| b.get("memories"))
                .and_then(|b| b.get("audit:unrelated")),
            Some(&Tag::Int(7))
        );
        assert_eq!(
            m.extra
                .warden
                .as_ref()
                .unwrap()
                .brain
                .mem
                .sniff_cooldown
                .ttl(),
            17
        );
        assert_eq!(
            m.extra.warden.as_ref().unwrap().vibration.to_nbt(),
            listener
        );
        let mem = &mut m.extra.warden.as_mut().unwrap().brain.mem;
        mem.is_emerging.set(());
        mem.dig_cooldown.clear();
        assert_saved(
            &mob_to_nbt(&m, [29, 0, 0, 1], Some(&raw), None, |_| None),
            [Some(i64::MAX), None],
        );
    }
    #[test]
    #[ignore = "daily"]
    fn actual_mob_ticks_expire_owned_memories_and_noai_retains_them() {
        let build = |world: &mut crate::world::World, registry: &BlockRegistry| {
            world
                .chunks
                .insert((0, 0), Arc::new(crate::chunk::Chunk::empty(0, 0)));
            let stone = registry.default_state("minecraft:stone").unwrap();
            for x in 0..=4 {
                for z in 0..=4 {
                    world.set(x, 0, z, stone).unwrap();
                }
            }
        };
        let ran = with_built_test_world(&[], build, |w, kinds| {
            let types = HashMap::from([(
                "minecraft:warden".into(),
                Arc::new(kinds.types["minecraft:warden"].clone()),
            )]);
            for no_ai in [false, true] {
                let mut raw = input(no_ai, 4);
                put(&mut raw, "Health", Tag::Float(500f32.to_bits()));
                let mut m = mob_from_nbt(&raw, &types, 93, None).unwrap();
                let loaded = slot_state(&m);
                let mut fixture = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                for tick in 1..=5 {
                    assert!(m.tick(w, &mut fixture.level(), &mut events));
                    assert_eq!(m.age, tick as u32);
                    let expected = if no_ai {
                        [Some(2), Some(17)]
                    } else {
                        [(tick <= 2).then_some(2 - tick), Some(17 - tick)]
                    };
                    assert_eq!(slot_state(&m), expected, "actual tick={tick},NoAI={no_ai}");
                    assert_saved(
                        &mob_to_nbt(&m, [29, 0, 0, 1], None, None, |_| None),
                        expected,
                    );
                    assert_eq!(m.health, 500.0);
                }
                assert_eq!(loaded, [Some(2), Some(17)]);
            }
        });
        assert!(ran, "prepared actual MobWorld prerequisite must run");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

}
