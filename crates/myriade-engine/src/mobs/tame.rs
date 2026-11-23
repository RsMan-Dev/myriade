//! Tamable animals (`TamableAnimal`): wolves with their variants, taming,
//! sitting, following and defending the owner, begging, anger and shaking
//! off water (`Wolf`, `WolfModel`).
use super::{
    MobEvent, MobWorld, PlayerView, Species, Who,
    anim::{Poses, State},
    goals::{
        Breed, Cx, Float, Goal, HurtByTarget, JUMP, LOOK, LeapAtTarget, LookAtPlayer, MOVE,
        MeleeAttack, NearestTarget, Panic, Prey, RandomLookAround, Stroll, TARGET, goal_clone,
        reduced,
    },
};
use horizon_core::geometry::trig::Mth;
use horizon_core::{
    geometry::{Aabb, Axis, Vec3},
    random::RandomSource,
};
use std::f32::consts::PI;

/// Wolf variants: registry name and texture file (`WolfVariants`).
pub const WOLF_VARIANTS: [(&str, &str); 9] = [
    ("pale", "wolf"),
    ("spotted", "wolf_spotted"),
    ("snowy", "wolf_snowy"),
    ("black", "wolf_black"),
    ("ashen", "wolf_ashen"),
    ("rusty", "wolf_rusty"),
    ("woods", "wolf_woods"),
    ("chestnut", "wolf_chestnut"),
    ("striped", "wolf_striped"),
];
/// The wolf variant a biome spawns (its high-priority `BiomeCheck`).
pub fn wolf_variant_for(biome: &str, has_tag: &dyn Fn(&str) -> bool) -> u8 {
    if has_tag("minecraft:is_savanna") {
        1
    } else if biome == "minecraft:grove" {
        2
    } else if biome == "minecraft:old_growth_pine_taiga" {
        3
    } else if biome == "minecraft:snowy_taiga" {
        4
    } else if has_tag("minecraft:is_jungle") {
        5
    } else if biome == "minecraft:forest" {
        6
    } else if biome == "minecraft:old_growth_spruce_taiga" {
        7
    } else if has_tag("minecraft:is_badlands") {
        8
    } else {
        0
    }
}
/// The default collar: red.
pub const RED: u8 = 14;

// AnimState slots: begging interest and the shake animation.
pub const INTEREST: usize = 0;
pub const SHAKE: usize = 1;

/// `LandOnOwnersShoulderGoal`: a standing parrot, 100 ticks old, lands on
/// the shoulder of the owner it touches; the host does the landing.
#[derive(Clone, Debug)]
struct LandOnOwnersShoulder;
impl Goal for LandOnOwnersShoulder {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// `NonTameRandomTargetGoal`: a tameable mob's prey target, only while it is
/// wild. Vanilla re-tests the subclass selector while hunting, so a baby
/// turtle that enters water stops being hunted.
#[derive(Clone, Debug)]
struct NonTameTarget(NearestTarget);
impl NonTameTarget {
    fn new(inner: NearestTarget) -> Self {
        Self(inner.named("NonTameRandomTargetGoal"))
    }
    fn prey_matches(prey: &Prey, who: Who, w: &MobWorld) -> bool {
        match prey {
            Prey::Player => who == Who::Player,
            Prey::Species(list) => match who {
                Who::Player => false,
                Who::Mob(id) => w.seen(id).is_some_and(|s| list.contains(&s.species)),
            },
            Prey::Filter(f) => match who {
                Who::Player => false,
                Who::Mob(id) => w.seen(id).is_some_and(f),
            },
        }
    }
}
impl Goal for NonTameTarget {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        "NonTameRandomTargetGoal"
    }
}
/// `Turtle.BABY_ON_LAND_SELECTOR`: a baby turtle out of water.
fn baby_turtle_on_land(s: &super::Seen) -> bool {
    s.species == Species::Turtle && s.baby && !s.in_water
}
/// `Wolf.PREY_SELECTOR`: sheep, rabbits and foxes.
fn wolf_prey(s: &super::Seen) -> bool {
    matches!(s.species, Species::Sheep | Species::Rabbit | Species::Fox)
}
/// Whether `uuid` is the saved owner. A host with no UUID yet owns what it
/// tamed under the placeholder `[0, 0, 0, 1]`.
pub fn owned_by(owner: Option<[i32; 4]>, uuid: [i32; 4]) -> bool {
    owner.is_some_and(|o| o == uuid || (o == [0, 0, 0, 1] && uuid == [0; 4]))
}

/// `TamableAnimal.TamableAnimalPanicGoal`: the wrapped `PanicGoal` is built
/// with the damage tag its species fears (`#panic_environmental_causes` for a
/// wolf, the default `#panic_causes` for a cat or parrot), and a panic
/// teleports a pet to a far owner.
#[derive(Clone, Debug)]
struct TamePanic(Panic);
impl Goal for TamePanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
}
/// `SitWhenOrderedToGoal`.
#[derive(Clone, Debug)]
struct SitWhenOrdered;
impl Goal for SitWhenOrdered {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
}
/// `Wolf.WolfAvoidEntityGoal`: untamed wolves flee llamas.
#[derive(Clone, Debug)]
struct AvoidLlamas(super::goals::AvoidEntity);
impl AvoidLlamas {
    fn new() -> Self {
        Self(super::goals::AvoidEntity::new(
            super::goals::Avoid::Species(vec![Species::Llama, Species::TraderLlama]),
            24.0,
            1.5,
            1.5,
        ))
    }
}
/// `Wolf.WolfAvoidEntityGoal`: a wild wolf runs from a llama, the more
/// likely the stronger it is (`strength >= random.nextInt(5)`), and drops
/// its target while it does.
impl Goal for AvoidLlamas {
    goal_clone!();
    fn name(&self) -> &'static str {
        "WolfAvoidEntityGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
}
/// `FollowOwnerGoal(speed, startDistance, stopDistance)`.
#[derive(Clone, Debug)]
struct FollowOwner {
    speed: f64,
    start: f64,
    stop: f64,
    recalc: i32,
}
impl FollowOwner {
    fn new(speed: f64, start: f64, stop: f64) -> Self {
        Self {
            speed,
            start,
            stop,
            recalc: 0,
        }
    }
}
impl Goal for FollowOwner {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}
/// `OwnerHurtByTargetGoal`: attack what hurt the owner.
#[derive(Clone, Debug)]
struct OwnerHurtBy {
    timestamp: i64,
}
impl Goal for OwnerHurtBy {
    goal_clone!();
    fn flags(&self) -> u8 {
        TARGET
    }
}
/// `OwnerHurtTargetGoal`: attack what the owner attacks.
#[derive(Clone, Debug)]
struct OwnerHurt {
    timestamp: i64,
}
impl Goal for OwnerHurt {
    goal_clone!();
    fn flags(&self) -> u8 {
        TARGET
    }
}
/// `BegGoal`: tilt the head at a player holding something interesting.
#[derive(Clone, Debug)]
struct Beg {
    food: std::sync::Arc<[String]>,
    ticks: i32,
}
impl Beg {
    fn interesting(&self, w: &MobWorld) -> bool {
        let Some(p) = w.selected_player() else {
            return false;
        };
        let held = |i: &Option<std::sync::Arc<str>>| {
            // `BegGoal.playerHoldingInteresting`: a bone or food, tame or not.
            i.as_deref()
                .is_some_and(|i| i == "minecraft:bone" || self.food.iter().any(|f| f == i))
        };
        p.alive && (held(&p.main_hand) || held(&p.off_hand))
    }
}
impl Goal for Beg {
    goal_clone!();
    fn flags(&self) -> u8 {
        LOOK
    }
}
/// `NeutralMob.isValidPlayerTarget`: a player in survival or adventure, off
/// peaceful.
fn valid_player_target(w: &MobWorld) -> bool {
    w.selected_player()
        .is_some_and(|p| !p.creative && !p.spectator)
        && w.difficulty.id != 0
}

/// `WolfRenderState.getBodyRollAngle`.
fn body_roll(shake: f32, offset: f32) -> f32 {
    let p = ((shake + offset) / 1.8).clamp(0.0, 1.0);
    (p * PI).msin() * (p * PI * 11.0).msin() * 0.15 * PI
}

/// Cat variants (`CatVariants`): registry name, texture file.
pub const CAT_VARIANTS: [&str; 11] = [
    "tabby",
    "black",
    "red",
    "siamese",
    "british_shorthair",
    "calico",
    "persian",
    "ragdoll",
    "white",
    "jellie",
    "all_black",
];
// Cat AnimState slots.
const LIE: usize = 0;
const LIE_TAIL: usize = 1;
const RELAX: usize = 2;

/// `Cat.registerGoals`: `WaterAvoidingRandomStrollGoal(this, 0.8, 1.0000001E-5F)`,
/// the ocelot's probability (a cat strolls out of water one tick in 100000).
pub fn cat_stroll() -> Stroll {
    let mut stroll = Stroll::avoiding_water(0.8);
    stroll.water_avoiding = Some(1.000_000_1e-5);
    stroll
}
/// `Ocelot.OcelotTemptGoal(ocelot, 0.6, ocelot food, true)`.
#[derive(Clone, Debug)]
struct OcelotTempt(super::goals::Tempt);
impl Goal for OcelotTempt {
    goal_clone!();
    fn name(&self) -> &'static str {
        "OcelotTemptGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
}
impl OcelotTempt {
}
/// `Ocelot.OcelotAvoidEntityGoal(ocelot, Player.class, 16.0F, 0.8, 1.33)`.
#[derive(Clone, Debug)]
struct OcelotAvoid(super::goals::AvoidEntity);
impl OcelotAvoid {
    fn new() -> Self {
        Self(super::goals::AvoidEntity::new(
            super::goals::Avoid::Player,
            16.0,
            0.8,
            1.33,
        ))
    }
}
impl Goal for OcelotAvoid {
    goal_clone!();
    fn name(&self) -> &'static str {
        "OcelotAvoidEntityGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
}
/// `Cat.CatTemptGoal`: only wild cats come for fish. The second field is
/// `selectedPlayer`, the player the cat has picked to approach unscared.
#[derive(Clone, Debug)]
struct CatTempt(super::goals::Tempt, Option<[i32; 4]>);
impl Goal for CatTempt {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
}
fn has_tag(w: &MobWorld, p: [i32; 3], tag: &str) -> bool {
    let name = w.facts.get(w.world.block(p[0], p[1], p[2])).name.clone();
    w.block_tags.get(tag).is_some_and(|t| t.contains(&*name))
}
fn empty_above(w: &MobWorld, p: [i32; 3]) -> bool {
    w.facts
        .get(w.world.block(p[0], p[1] + 1, p[2]))
        .name
        .as_ref()
        == "minecraft:air"
}
/// `Cat.CatAvoidEntityGoal`: the player avoidance only while untamed.
#[derive(Clone, Debug)]
struct CatAvoid(super::goals::AvoidEntity);
impl Goal for CatAvoid {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
}
/// `Cat.CatRelaxOnOwnerGoal`: while the owner sleeps, the cat lies down beside
/// them, and may leave a morning gift when they wake up.
#[derive(Clone, Debug, Default)]
struct CatRelaxOnOwner {
    goal: Option<[i32; 3]>,
    on_bed_ticks: i32,
}
impl CatRelaxOnOwner {
}
impl Goal for CatRelaxOnOwner {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// `CatLieOnBlockGoal`: tame cats lie on beds and other warm blocks.
#[derive(Clone, Debug)]
struct CatLieOnBlock(super::goals::MoveToBlock);
fn can_lie_on(w: &MobWorld, p: [i32; 3]) -> bool {
    empty_above(w, p) && has_tag(w, p, "minecraft:cats_can_lie_on")
}
impl Goal for CatLieOnBlock {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `CatSitOnBlockGoal`: unopened chests, lit furnaces and the foot of beds.
#[derive(Clone, Debug)]
struct CatSitOnBlock(super::goals::MoveToBlock);
fn can_sit_on(w: &MobWorld, p: [i32; 3]) -> bool {
    if !empty_above(w, p) || !has_tag(w, p, "minecraft:cats_can_sit_on") {
        return false;
    }
    let f = w.facts.get(w.world.block(p[0], p[1], p[2]));
    match f.name.as_ref() {
        // The lit state is the only light source a furnace has.
        "minecraft:furnace" => f.emission > 0,
        // `ChestBlockEntity.getOpenCount(level, pos) < 1`: players' openers
        // and the copper golems' own count.
        "minecraft:chest" => {
            let key = (p[0], p[1], p[2]);
            !w.lid_openers.contains_key(&key)
                && w.world.chest_openers.get(&key).is_none_or(|o| o.0 < 1)
        }
        // A bed's foot only (`BedBlock.PART != HEAD`).
        _ => w.prop(p[0], p[1], p[2], "part") != Some("head"),
    }
}
impl Goal for CatSitOnBlock {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `OcelotAttackGoal`: stalk slowly, dash in, swipe every second.
#[derive(Clone, Debug)]
struct OcelotAttack {
    attack_time: i32,
}
impl Goal for OcelotAttack {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `Cat.finalizeSpawn` variant: any but all-black, which a full moon allows.
pub fn cat_variant(w_moon_full: bool, r: &mut impl RandomSource) -> u8 {
    let n = if w_moon_full { 11 } else { 10 };
    r.next_int_bound(n).unwrap_or(0) as u8
}
/// `Mth.rotLerp`: degrees-wrapping, applied by the feline models to radians.
fn rot_lerp(t: f32, from: f32, to: f32) -> f32 {
    from + t * super::living::wrap(to - from)
}
/// `FelineRenderState` values the feline models read.
#[derive(Clone, Copy, Debug, Default)]
pub struct Feline {
    pub crouching: bool,
    pub sprinting: bool,
    pub sitting: bool,
    pub lie_down: f32,
    pub lie_down_tail: f32,
    pub relax_one: f32,
}
pub fn feline(p: &mut Poses, s: &State, st: &Feline) {
    let baby = s.baby;
    let age = if baby { 0.5 } else { 1.0 };
    let (crouching, sprinting, sitting) = (st.crouching, st.sprinting, st.sitting);
    let add = |p: &mut Poses, part: &str, x: f32, y: f32, z: f32| {
        let v = p.get(part);
        v[0] += x;
        v[1] += y;
        v[2] += z;
    };
    if crouching {
        add(p, "body", 0.0, age, 0.0);
        add(p, "head", 0.0, 2.0 * age, 0.0);
        add(p, "tail1", 0.0, age, 0.0);
        add(p, "tail2", 0.0, -4.0 * age, 2.0 * age);
        p.xrot("tail1", PI / 2.0);
        p.xrot("tail2", PI / 2.0);
    } else if sprinting {
        let y1 = p.get("tail1")[1];
        p.get("tail2")[1] = y1;
        add(p, "tail2", 0.0, 0.0, 2.0 * age);
        p.xrot("tail1", PI / 2.0);
        p.xrot("tail2", PI / 2.0);
    }
    p.xrot("head", s.x_rot.to_radians());
    p.yrot("head", s.y_rot.to_radians());
    let (pos, sp) = (s.pos, s.speed);
    if !sitting {
        if !baby {
            p.xrot("body", PI / 2.0);
        }
        if sprinting {
            p.xrot("left_hind_leg", (pos * 0.6662).mcos() * sp);
            p.xrot("right_hind_leg", (pos * 0.6662 + 0.3).mcos() * sp);
            p.xrot("left_front_leg", (pos * 0.6662 + PI + 0.3).mcos() * sp);
            p.xrot("right_front_leg", (pos * 0.6662 + PI).mcos() * sp);
            p.xrot("tail2", 1.727_876_1 + PI / 10.0 * pos.mcos() * sp);
        } else {
            p.xrot("left_hind_leg", (pos * 0.6662).mcos() * sp);
            p.xrot("right_hind_leg", (pos * 0.6662 + PI).mcos() * sp);
            p.xrot("left_front_leg", (pos * 0.6662 + PI).mcos() * sp);
            p.xrot("right_front_leg", (pos * 0.6662).mcos() * sp);
            let swing = if crouching { 0.471_238_94 } else { PI / 4.0 };
            p.xrot("tail2", 1.727_876_1 + swing * pos.mcos() * sp);
        }
    } else if baby {
        let x = p.get("body")[3] - 0.436_332_32;
        p.xrot("body", x);
        add(p, "body", 0.0, 1.25, 0.0);
        add(p, "head", 0.0, 0.0, 0.75);
        let t = p.get("tail1")[3] + 0.545_415_4;
        p.xrot("tail1", t);
        add(p, "tail1", 0.0, 4.0, -0.9);
        add(p, "left_hind_leg", 0.0, 0.0, -0.9);
        add(p, "right_hind_leg", 0.0, 0.0, -0.9);
    } else {
        p.xrot("body", PI / 4.0);
        add(p, "body", 0.0, -4.0 * age, 5.0 * age);
        add(p, "head", 0.0, -3.3 * age, age);
        add(p, "tail1", 0.0, 8.0 * age, -2.0 * age);
        add(p, "tail2", 0.0, 2.0 * age, -0.8 * age);
        p.xrot("tail1", 1.727_876_1);
        p.xrot("tail2", 2.670_354);
        p.xrot("left_front_leg", -PI / 20.0);
        add(p, "left_front_leg", 0.0, 2.0 * age, -2.0 * age);
        p.xrot("right_front_leg", -PI / 20.0);
        add(p, "right_front_leg", 0.0, 2.0 * age, -2.0 * age);
        p.xrot("left_hind_leg", -PI / 2.0);
        add(p, "left_hind_leg", 0.0, 3.0 * age, -4.0 * age);
        p.xrot("right_hind_leg", -PI / 2.0);
        add(p, "right_hind_leg", 0.0, 3.0 * age, -4.0 * age);
    }
    let lie = st.lie_down;
    let lie_tail = st.lie_down_tail;
    if lie > 0.0 && baby {
        add(p, "body", 1.0, 0.0, 0.0);
        let hx = p.get("head")[3];
        p.xrot("head", rot_lerp(lie, hx, PI / 18.0));
        let hz = p.get("head")[5];
        p.zrot("head", rot_lerp(lie, hz, -PI * 5.0 / 12.0));
        add(p, "head", 1.5, 0.75, -0.5);
        p.xrot("right_front_leg", -PI / 4.0);
        add(p, "right_front_leg", 3.5, -0.5, 0.0);
        p.xrot("left_front_leg", -PI / 2.0);
        add(p, "left_front_leg", 1.5, -1.0, -2.0);
        p.xrot("right_hind_leg", PI * 2.0 / 9.0);
        p.yrot("right_hind_leg", PI / 9.0);
        p.zrot("right_hind_leg", -PI / 9.0);
        add(p, "right_hind_leg", 2.5, -0.25, 0.5);
        add(p, "left_hind_leg", 1.5, 0.0, -1.0);
        let t = p.get("tail1");
        let (tx, ty, tz) = (t[3], t[4], t[5]);
        p.xrot("tail1", tx + rot_lerp(lie_tail, tx, -PI / 6.0));
        p.yrot("tail1", ty + rot_lerp(lie_tail, ty, 0.0));
        p.zrot("tail1", tz + rot_lerp(lie_tail, tz, -PI / 18.0));
        add(p, "tail1", 1.0, 0.5, -0.25);
    } else if lie > 0.0 {
        let (hz, hy) = (p.get("head")[5], p.get("head")[4]);
        p.zrot("head", rot_lerp(lie, hz, -1.270_796_3));
        p.yrot("head", rot_lerp(lie, hy, 1.270_796_3));
        p.xrot("left_front_leg", -1.270_796_3);
        p.xrot("right_front_leg", -0.470_796_35);
        p.zrot("right_front_leg", -0.2);
        add(p, "right_front_leg", age, 0.0, 0.0);
        p.xrot("left_hind_leg", -0.4);
        p.xrot("right_hind_leg", 0.5);
        p.zrot("right_hind_leg", -0.5);
        add(p, "right_hind_leg", 0.8 * age, 2.0 * age, 0.0);
        let (t1, t2) = (p.get("tail1")[3], p.get("tail2")[3]);
        p.xrot("tail1", rot_lerp(lie_tail, t1, 0.8));
        p.xrot("tail2", rot_lerp(lie_tail, t2, -0.4));
    }
    let relax = st.relax_one;
    if relax > 0.0 {
        let hx = p.get("head")[3];
        p.xrot("head", rot_lerp(relax, hx, -0.581_776_44));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{Damage, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;



    /// `TamableAnimal.unableToMoveToOwner` (#12939): a pet on a lead, with lead
    /// data still to be found, or riding something neither follows nor
    /// teleports to its owner.
    #[test]
    #[ignore = "daily"]
    fn a_leashed_or_riding_pet_stays_put() {
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            let mut m = wolf();
            set_tame(&mut m, true);
            assert!(!unable_to_move_to_owner(&m, w), "a free pet follows");
            m.extra.leash = Some(super::super::leash::Holder::Knot([0, 64, 3]));
            assert!(unable_to_move_to_owner(&m, w), "tied to a fence");
            m.extra.leash = None;
            m.extra.passenger = true;
            assert!(unable_to_move_to_owner(&m, w), "in a boat");
            m.extra.passenger = false;
            m.extra.sitting = true;
            assert!(unable_to_move_to_owner(&m, w), "ordered to sit");
        });
        if !ran {
            eprintln!("skipped: no extracted vanilla data");
        }
    }
    /// `SitWhenOrderedToGoal.start` only stops and sits: a wolf begging for the
    /// bone its owner holds keeps its head tilt (`BegGoal` owns it).
    #[test]
    #[ignore = "daily"]
    fn sitting_down_keeps_a_wolfs_begging_tilt() {
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            let mut fixture = crate::health::TestLevel::new(0);
            let mut events = Vec::new();
            let mut cx = Cx {
                w,
                level: &mut fixture.level(),
                events: &mut events,
            };
            let mut m = wolf();
            set_tame(&mut m, true);
            m.extra.flag = true;
            m.extra.sitting = true;
            SitWhenOrdered.start(&mut m, &mut cx);
            assert!(m.extra.flag, "the begging tilt is BegGoal's to end");
        });
        if !ran {
            eprintln!("skipped: no extracted vanilla data");
        }
    }

    const WHITE: u32 = 0xFFFF_FFFF;

    #[test]
    fn a_cat_lying_near_a_sleeper_lies_on_top_of_them() {
        use horizon_core::geometry::Aabb;
        let sleeper = Aabb::new(Vec3::new(2.4, 64.0, 0.4), Vec3::new(2.6, 64.2, 0.6));
        let far = Aabb::new(Vec3::new(5.4, 64.0, 0.4), Vec3::new(5.6, 64.2, 0.6));
        let mut m = cat();
        m.anim.v[LIE] = 1.0;
        m.extra.flag = true;
        cat_lying_on_sleeper(&mut m, &[far]);
        assert!(!m.extra.on_sleeper);
        cat_lying_on_sleeper(&mut m, &[far, sleeper]);
        assert!(m.extra.on_sleeper);
        let on_top = cat_rotations(&m, 1.0);
        // The extra `translate(0.15 * lie, 0, 0)` after the roll.
        m.extra.on_sleeper = false;
        let plain = cat_rotations(&m, 1.0);
        assert_ne!(on_top, plain);
        assert!((on_top.w_axis.y - plain.w_axis.y - 0.15).abs() < 1.0e-4);
        // Not lying: nothing to lie on.
        m.extra.flag = false;
        cat_lying_on_sleeper(&mut m, &[sleeper]);
        assert!(!m.extra.on_sleeper);
    }
    /// `Wolf.die`: the wet flag, the shake counter and both shake animations
    /// are cleared before any loot is rolled. The death runs
    /// `drops::death_state` first, so the reset has to be asked there, not
    /// only exist.
    #[test]
    fn a_wolf_that_dies_mid_shake_is_dry_and_still() {
        let mut m = wolf();
        // Soaked, and shaking it off: the tick sets the wet flag and entity
        // event 8 starts the shake, which counts up 0.05 a tick.
        m.extra.flag2 = true;
        entity_event(&mut m, 8);
        m.anim.v[SHAKE] = 0.9;
        m.anim.vo[SHAKE] = 0.85;
        crate::mobs::drops::death_state(&mut m);
        assert!(!m.extra.flag2, "a dead wolf is not wet");
        assert_eq!(m.extra.counter2, 0, "a dead wolf is not shaking");
        assert_eq!(m.anim.v[SHAKE], 0.0, "shakeAnim");
        assert_eq!(m.anim.vo[SHAKE], 0.0, "shakeAnimO");
    }
    fn entity_events(events: &[MobEvent]) -> Vec<i8> {
        events
            .iter()
            .filter_map(|e| match e {
                MobEvent::EntityEvent { event, .. } => Some(*event),
                _ => None,
            })
            .collect()
    }

    /// A wolf of a server (the default single player's server thread, a LAN
    /// guest): `Wolf.tick` makes it wet in water on the client too, which
    /// `getWetShade` darkens; entity event 8 starts the shake, which the
    /// client runs to the end, splashing, and then the wolf is dry (#2885).
    #[test]
    fn a_wolf_of_a_server_darkens_in_water_and_shakes_dry_on_its_event() {
        let mut m = wolf();
        let mut events = Vec::new();
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        // 0.75 of the colour: 191.
        assert_eq!(tint(&m, 1.0), 0xFFBF_BFBF, "wet in water");
        m.fluids.in_water = false;
        client_tick(&mut m, false, &mut events);
        assert_eq!(tint(&m, 1.0), 0xFFBF_BFBF, "still wet before it shakes");
        assert_eq!(m.anim.v[SHAKE], 0.0);
        // Rain wets it as well.
        let mut dry = wolf();
        client_tick(&mut dry, true, &mut events);
        assert_ne!(tint(&dry, 1.0), WHITE, "wet in the rain");

        crate::mobs::sync::entity_event(&mut m, 8);
        let mut ticks = 0;
        while m.extra.flag2 && ticks < 100 {
            client_tick(&mut m, false, &mut events);
            ticks += 1;
            if ticks == 1 {
                assert!((m.anim.v[SHAKE] - 0.05).abs() < 1e-6);
                assert_eq!(m.anim.vo[SHAKE], 0.0);
            }
        }
        // shakeAnimO reaches 2 on the 41st tick.
        assert!((41..=42).contains(&ticks), "shook for {ticks} ticks");
        assert_eq!(tint(&m, 1.0), WHITE, "dry after the shake");
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MobEvent::ParticleAt { kind: "splash", .. })),
            "the client draws the splashes"
        );
        assert!(
            !events.iter().any(|e| matches!(e, MobEvent::Sound(..))),
            "the shake sound is the server's"
        );
    }

    /// `Wolf.handleEntityEvent(56)`: back in the water, the server cancels
    /// the shake; the wolf stays wet. A client does not cancel on its own.
    #[test]
    fn a_wolf_of_a_server_stops_shaking_on_its_cancel_event() {
        let mut m = wolf();
        let mut events = Vec::new();
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        m.fluids.in_water = false;
        crate::mobs::sync::entity_event(&mut m, 8);
        for _ in 0..10 {
            client_tick(&mut m, false, &mut events);
        }
        assert!(m.anim.v[SHAKE] > 0.4);
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        assert!(m.extra.counter2 > 0, "only the server cancels");
        crate::mobs::sync::entity_event(&mut m, 56);
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert_ne!(tint(&m, 1.0), WHITE, "still wet");
    }

    /// `Wolf.aiStep` broadcasts entity event 8 when a wet wolf standing on
    /// the ground starts shaking, and `Wolf.tick` event 56 when water or rain
    /// cancels it: the clients' only way to know.
    #[test]
    fn a_server_tells_the_clients_when_a_wolf_shakes() {
        let mut m = wolf();
        let mut events = Vec::new();
        server_shake(&mut m, true, &mut events);
        assert!(m.extra.flag2);
        // Out of the water: it starts and makes its sound in the same tick.
        server_shake(&mut m, false, &mut events);
        assert_eq!(entity_events(&events), [8]);
        assert!(m.extra.counter2 > 0);
        assert!((m.anim.v[SHAKE] - 0.05).abs() < 1e-6);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MobEvent::Sound("entity.wolf.shake", ..)))
        );
        server_shake(&mut m, true, &mut events);
        assert_eq!(entity_events(&events), [8, 56]);
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert!(m.extra.flag2, "still wet");
    }


    #[test]
    fn a_hit_stands_a_sitting_wolf_or_parrot_up() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Wolf.hurtServer and Parrot.hurtServer call setOrderedToSit(false)
        // before LivingEntity.hurtServer.
        for (species, name) in [
            (Species::Wolf, "minecraft:wolf"),
            (Species::Parrot, "minecraft:parrot"),
        ] {
            let mut m = sitting(species, name);
            assert!(m.damage(level, 1.0, Damage::player([0.0, 64.0, 1.0])));
            assert!(!m.extra.sitting, "{name} still sits after a hit");
            // A weaker hit inside the hurt frames deals nothing, but it
            // reached hurtServer, so it still stands the mob up.
            m.extra.sitting = true;
            assert!(!m.damage(level, 0.5, Damage::player([0.0, 64.0, 1.0])));
            assert!(!m.extra.sitting, "{name} still sits after a blocked hit");
        }
    }

    #[test]
    fn a_hit_leaves_a_sitting_cat_sitting() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Cat has no hurtServer override: a hit does not stand it up.
        let mut m = sitting(Species::Cat, "minecraft:cat");
        assert!(m.damage(level, 1.0, Damage::player([0.0, 64.0, 1.0])));
        assert!(m.extra.sitting);
    }

    /// `Cat.canMate` and `Wolf.canMate`: the seeker only has to be tamed, so a
    /// mob ordered to sit still goes looking for a mate. Only a wolf's
    /// *partner* is refused while it sits (`wolf.isInSittingPose()`), which is
    /// the one sitting check vanilla makes.
    #[test]
    #[ignore = "daily"]
    fn a_sitting_tame_mob_still_looks_for_a_mate() {
        use crate::mobs::goals::{Breed, Cx, Goal};
        use crate::mobs::world::with_test_world;
        let named = |species, name: &str, id: u32, sat: bool| {
            let mut m = sitting(species, name);
            m.id = id;
            m.extra.in_love = 600;
            // Ordered to sit on the ground, so also in the pose the partner
            // check reads.
            m.extra.sitting = sat;
            m.extra.sitting_pose = sat;
            m
        };
        for (species, name, seeker_sat, partner_sat, breeds) in [
            // A tamed cat pairs with a tamed cat, sitting or not.
            (Species::Cat, "minecraft:cat", true, true, true),
            (Species::Cat, "minecraft:cat", true, false, true),
            // A wolf pairs unless the wolf it would take is sitting.
            (Species::Wolf, "minecraft:wolf", true, false, true),
            (Species::Wolf, "minecraft:wolf", true, true, false),
            (Species::Wolf, "minecraft:wolf", false, true, false),
        ] {
            let mut seeker = named(species, name, 1, seeker_sat);
            let partner = named(species, name, 2, partner_sat);
            let seen = [partner.seen(None, &crate::items::Items::empty())];
            assert!(
                with_test_world(&seen, |w, _| {
                    let mut events = Vec::new();
                    let mut test_level = crate::health::TestLevel::new(0);
                    let level = &mut test_level.level();
                    let mut cx = Cx {
                        w,
                        level,
                        events: &mut events,
                    };
                    let mut goal = Breed::new(1.0);
                    assert_eq!(
                        goal.can_use(&mut seeker, &mut cx),
                        breeds,
                        "{name}: seeker sits {seeker_sat}, partner sits {partner_sat}"
                    );
                }),
                "vanilla data is missing"
            );
        }
    }

}

/// `NeutralMob.isAngryAtAllPlayers`: with the `universal_anger` game rule on, a
/// wolf still angry with no target left goes for any player; with the rule off
/// it holds none, and a creative player is never one (`isValidPlayerTarget`).
#[cfg(test)]
mod universal_anger_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;

}

#[cfg(test)]
mod cat_tempt_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}

#[cfg(test)]
mod tamable_panic_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world;
    use crate::mobs::{Damage, DamageKind};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    /// `TamableAnimal.TamableAnimalPanicGoal` is built with a damage tag: a
    /// cat's and a parrot's take the default `#panic_causes` (any damage that
    /// makes a mob panic), a wolf's `#panic_environmental_causes` (fire,
    /// freezing, cactus, lightning, lava...).
    #[test]
    #[ignore = "daily"]
    fn tamable_animals_panic_from_the_damage_they_fear() {
        // Needs the extracted mob data (`with_built_test_world`); CI has none.
        let (Some(cat), Some(frozen), Some(hit)) = (
            panics("minecraft:cat", register_cat, DamageKind::PlayerAttack),
            panics("minecraft:wolf", register, DamageKind::Freeze),
            panics("minecraft:wolf", register, DamageKind::PlayerAttack),
        ) else {
            eprintln!("skipped: no extracted mob data");
            return;
        };
        assert!(cat, "a cat hit by a player runs");
        assert!(frozen, "a freezing wolf runs");
        assert!(!hit, "a wolf hit by a player does not panic");
    }
}

#[cfg(test)]
mod non_tame_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}


#[cfg(test)]
mod goal_timing_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;

    /// `FollowOwnerGoal` has no `requiresUpdateEveryTick`, so its `tick` runs on
    /// the full selector updates only and `adjustedTickDelay(10)` is
    /// `reducedTickDelay(10)` of them.
    #[test]
    fn a_pet_recalculates_its_path_on_full_updates_only() {
        let goal = FollowOwner::new(1.0, 10.0, 2.0);
        assert!(
            !goal.every_tick(),
            "FollowOwnerGoal has no requiresUpdateEveryTick override"
        );
    }

}

