//! Random positions, once for every module: `DefaultRandomPos` / `LandRandomPos` (the plain, away, towards and scored
//! forms) with the vanilla rules of a candidate: the offset is rounded to a block BEFORE it is added to the position
//! (`RandomPos.generateRandomPosTowardDirection`), the position is checked (height, stable destination, the path
//! malus), and the best of ten by the species' `walk_target_value` wins (the first on a tie). A module calls `pick` and
//! never writes its own draw. The numbers come from `Mob` (`goals.rs` `default_random_pos*`, `land_random_pos*`,
//! `ai.rs` `walk_target_value`); the random stream is the entity's `Rng` (the same laws, not the same stream).
use crate::IFloor as _;
use crate::cow::{Level, NavParams, Rng};
use crate::rules::spots::outside_home;
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::MobWorld;

/// `PathfinderMob.getWalkTargetValue`: what the species prefers to walk to (data of the species, set at spawn).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkValue {
    /// Every place alike (`Enderman`, `Pillager`, `Warden`, `Axolotl`...: the ten tries pick the first valid).
    Zero,
    /// `Animal.getWalkTargetValue`: grass 10, else the light above the dark (`brightness - 0.5`).
    Animal,
    /// `Monster.getWalkTargetValue`: the opposite of the light (`0.5 - brightness`): the dark.
    Monster,
    /// `MushroomCow`: mycelium 10, else the light.
    Mooshroom,
    /// `Bee`: any air block.
    Bee,
    /// `Turtle.getWalkTargetValue` (not going home): water or sand under the spot 10, else the light.
    Turtle,
    /// `Guardian.getWalkTargetValue`: water 10 plus the light, else the monster's.
    Guardian,
    /// `HappyGhast.getWalkTargetValue`: open air only, 10 just above the ground, else 5.
    HappyGhast,
    /// `Hoglin.getWalkTargetValue`: crimson nylium 10, else 0 (the repellent's -1 is not modelled: no entity keeps one).
    Hoglin,
    /// `Strider.getWalkTargetValue`: lava 10, else 0, or -infinity when the strider is in lava (`Spotter.in_lava`).
    Strider,
    /// `Silverfish.getWalkTargetValue`: an infested host under the spot 10, else the monster's.
    Silverfish,
}
/// The value of walking to the block `p` (the species' preference).
pub fn walk_target_value(mw: &MobWorld, wv: WalkValue, p: [i32; 3]) -> f32 {
    let light = || mw.brightness(p[0], p[1], p[2]) - 0.5;
    match wv {
        WalkValue::Zero => 0.0,
        WalkValue::Monster => -light(),
        WalkValue::Mooshroom => {
            if mw.block_name(p[0], p[1] - 1, p[2]) == "minecraft:mycelium" {
                10.0
            } else {
                light()
            }
        }
        WalkValue::Turtle => {
            if mw.fluid(p[0], p[1], p[2]) == myriade_engine_lib::blocks::Fluid::Water || mw.has_tag("minecraft:sand", mw.block_name(p[0], p[1] - 1, p[2])) {
                10.0
            } else {
                light()
            }
        }
        WalkValue::Guardian => {
            if mw.fluid(p[0], p[1], p[2]) == myriade_engine_lib::blocks::Fluid::Water {
                10.0 + light()
            } else {
                -light()
            }
        }
        WalkValue::HappyGhast => {
            let air = |y: i32| mw.facts.is_air(mw.world.block(p[0], y, p[2]));
            if !air(p[1]) {
                0.0
            } else if air(p[1] - 1) && !air(p[1] - 2) {
                10.0
            } else {
                5.0
            }
        }
        WalkValue::Bee => {
            if mw.facts.is_air(mw.world.block(p[0], p[1], p[2])) {
                10.0
            } else {
                0.0
            }
        }
        WalkValue::Hoglin => {
            if mw.block_name(p[0], p[1] - 1, p[2]) == "minecraft:crimson_nylium" {
                10.0
            } else {
                0.0
            }
        }
        WalkValue::Strider => {
            if mw.fluid(p[0], p[1], p[2]) == myriade_engine_lib::blocks::Fluid::Lava {
                10.0
            } else {
                0.0
            }
        }
        WalkValue::Silverfish => {
            if myriade_engine_lib::mobs::hostile::is_infested_host(mw.block_name(p[0], p[1] - 1, p[2])) {
                10.0
            } else {
                -light()
            }
        }
        WalkValue::Animal => {
            if mw.block_name(p[0], p[1] - 1, p[2]) == "minecraft:grass_block" {
                10.0
            } else {
                light()
            }
        }
    }
}
/// `PathNavigation.isStableDestination` of a ground navigation: a solid block under the spot.
pub fn stable_destination(mw: &MobWorld, p: [i32; 3]) -> bool {
    mw.facts.get(mw.world.block(p[0], p[1] - 1, p[2])).solid_render
}

/// `PathNavigation.isStableDestination` of the navigation the entity moves with (the active mode of locomotion gives it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stable {
    /// A solid block under the spot (a walker).
    #[default]
    Ground,
    /// The spot is not a solid block (a swimmer: `WaterBoundPathNavigation`).
    Water,
    /// Something other than air under the spot (`AmphibiousPathNavigation`: a turtle).
    Amphibious,
}
pub fn stable_for(kind: Stable, mw: &MobWorld, p: [i32; 3]) -> bool {
    match kind {
        Stable::Ground => stable_destination(mw, p),
        Stable::Water => !mw.facts.get(mw.world.block(p[0], p[1], p[2])).solid_render,
        Stable::Amphibious => !mw.facts.is_air(mw.world.block(p[0], p[1] - 1, p[2])),
    }
}

/// What a draw needs of the entity: where it is, its stream, its path maluses and its preference.
/// `PathfinderMob.restrictTo(center, radius)`: where the entity belongs (its home): the positions it picks stay within it while it is near.
#[derive(bevy_ecs::prelude::Component, Clone, Copy, Debug)]
pub struct Home {
    pub center: [i32; 3],
    pub radius: i32,
}
pub struct Spotter<'a> {
    pub home: Option<Home>,
    pub mw: &'a MobWorld<'a>,
    pub pos: [f64; 3],
    pub nav: &'a NavParams,
    pub walk: WalkValue,
    pub rng: &'a mut Rng,
    /// The stable destination of the navigation in use (a swimmer's is not a walker's).
    pub stable: Stable,
    /// The entity is in lava (`Entity.isInLava`): for a strider, the spots out of lava weigh -infinity.
    pub in_lava: bool,
}
/// The forms of the vanilla position pickers.
#[derive(Clone, Copy, Debug)]
pub enum Pick {
    /// `DefaultRandomPos.getPos(mob, h, v)`.
    Default { h: i32, v: i32 },
    /// `DefaultRandomPos.getPosAway(mob, h, v, from)`.
    DefaultAway { h: i32, v: i32, from: [f64; 3] },
    /// `DefaultRandomPos.getPosTowards(mob, h, v, to, max_rad)`.
    DefaultTowards { h: i32, v: i32, to: [f64; 3], max_rad: f64 },
    /// `LandRandomPos.getPos(mob, h, v)`.
    Land { h: i32, v: i32 },
    /// `LandRandomPos.getPosAway(mob, min_h, max_h, v, from)` (`min_h` 0 for the form without a minimum).
    LandAway { min_h: f64, max_h: f64, v: i32, from: [f64; 3] },
    /// `LandRandomPos.getPosTowards(mob, h, v, to)`.
    LandTowards { h: i32, v: i32, to: [f64; 3] },
    /// `Mob.flying_stroll_pos`: `HoverRandomPos(8, 7, 3, 1)` in the half circle of the view `(xd, zd)`, else `AirAndWaterRandomPos(8, 4, -2)`.
    Flying { xd: f64, zd: f64 },
    /// `AirAndWaterRandomPos.getPos(mob, h, v, -2, xd, zd, PI / 2)` alone, no hover spot first (`RandomStroll.fly`: the allay's 10 by 7).
    AirAndWater { h: i32, v: i32, xd: f64, zd: f64 },
    /// `AirRandomPos.getPosTowards(h, v, fly_height, to, max_rad)`: an `AirAndWaterRandomPos` towards a point, out of the water (the bee's).
    AirTowards { h: i32, v: i32, fly: i32, to: [f64; 3], max_rad: f64 },
}
impl Rng {
    /// `RandomSource.nextFloat`.
    pub fn next_float(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
}
impl Spotter<'_> {
    /// `GoalUtils.isRestricted(mobRestricted(mob, h), mob, p)`: near its home, a position outside it is refused.
    fn restricted(&self, h: f64, p: [i32; 3]) -> bool {
        self.home.is_some_and(|home| outside_home(home.center, home.radius, self.pos, h, p))
    }
    fn malus_is_zero(&self, p: [i32; 3]) -> bool {
        let t = crate::pathsearch::path_type_static(self.mw, p[0], p[1], p[2]);
        self.nav.malus[t as usize].unwrap_or_else(|| t.malus()) == 0.0
    }
    fn direction(&mut self, h: i32, v: i32) -> [i32; 3] {
        let x = self.rng.next_int(2 * h + 1) - h;
        let y = self.rng.next_int(2 * v + 1) - v;
        let z = self.rng.next_int(2 * h + 1) - h;
        [x, y, z]
    }
    /// `RandomPos.generateRandomDirectionWithinRadians`.
    fn direction_within(&mut self, min: f64, max: f64, v: i32, xd: f64, zd: f64, max_rad: f64) -> Option<[i32; 3]> {
        self.direction_within_height(min, max, v, 0, xd, zd, max_rad)
    }
    /// The same with the flying height added to the vertical offset (`RandomPos.generateRandomDirectionWithinRadians`).
    #[allow(clippy::too_many_arguments)]
    fn direction_within_height(&mut self, min: f64, max: f64, v: i32, fly_height: i32, xd: f64, zd: f64, max_rad: f64) -> Option<[i32; 3]> {
        let center = horizon_core::geometry::trig::atan2(zd, xd) - f64::from(std::f32::consts::FRAC_PI_2);
        let yrad = center + f64::from(2.0 * self.rng.next_float() - 1.0) * max_rad;
        let dist = (min + self.rng.next_f64().sqrt() * (max - min)) * f64::from(std::f32::consts::SQRT_2);
        let xt = -dist * f64::from((yrad as f32).sin());
        let zt = dist * f64::from((yrad as f32).cos());
        if xt.abs() > max || zt.abs() > max {
            return None;
        }
        let yt = self.rng.next_int(2 * v + 1) - v + fly_height;
        Some([xt.ifloor(), yt, zt.ifloor()])
    }
    /// The offset is a block offset: it is rounded first, then added to the position and rounded again.
    fn toward(&self, d: [i32; 3]) -> [i32; 3] {
        crate::rules::spots::toward(self.pos, d)
    }
    fn in_height(&self, p: [i32; 3]) -> bool {
        p[1] >= self.mw.world.bottom_y() && p[1] < self.mw.world.top_y()
    }
    /// `PathNavigation.isStableDestination`: a strider's navigation also takes a lava spot as stable (`StriderPathNavigation`).
    fn stable_at(&self, p: [i32; 3]) -> bool {
        stable_for(self.stable, self.mw, p) || (matches!(self.walk, WalkValue::Strider) && self.mw.fluid(p[0], p[1], p[2]) == myriade_engine_lib::blocks::Fluid::Lava)
    }
    fn default_check(&self, h: f64, p: [i32; 3]) -> bool {
        self.in_height(p) && !self.restricted(h, p) && self.stable_at(p) && self.malus_is_zero(p)
    }
    /// `LandRandomPos.movePosUpOutOfSolid` and the water / malus refusal.
    fn land_up_out_of_solid(&self, mut p: [i32; 3]) -> Option<[i32; 3]> {
        let mw = self.mw;
        if mw.solid(p[0], p[1], p[2]) {
            p[1] += 1;
            while p[1] < mw.world.top_y() && mw.solid(p[0], p[1], p[2]) {
                p[1] += 1;
            }
        }
        let water = mw.fluid(p[0], p[1], p[2]) == myriade_engine_lib::blocks::Fluid::Water;
        (!water && self.malus_is_zero(p)).then_some(p)
    }
    fn candidate(&mut self, pick: Pick) -> Option<[i32; 3]> {
        match pick {
            Pick::Default { h, v } => {
                let d = self.direction(h, v);
                let p = self.toward(d);
                self.default_check(f64::from(h), p).then_some(p)
            }
            Pick::DefaultAway { h, v, from } => {
                let (xd, zd) = (self.pos[0] - from[0], self.pos[2] - from[2]);
                let d = self.direction_within(0.0, f64::from(h), v, xd, zd, std::f64::consts::FRAC_PI_2)?;
                let p = self.toward(d);
                self.default_check(f64::from(h), p).then_some(p)
            }
            Pick::DefaultTowards { h, v, to, max_rad } => {
                let (xd, zd) = (to[0] - self.pos[0], to[2] - self.pos[2]);
                let d = self.direction_within(0.0, f64::from(h), v, xd, zd, max_rad)?;
                let p = self.toward(d);
                self.default_check(f64::from(h), p).then_some(p)
            }
            Pick::Land { h, v } => {
                let d = self.direction(h, v);
                let p = self.toward(d);
                if !self.in_height(p) || self.restricted(f64::from(h), p) || !self.stable_at(p) {
                    return None;
                }
                self.land_up_out_of_solid(p)
            }
            Pick::LandAway { .. } | Pick::LandTowards { .. } | Pick::Flying { .. } | Pick::AirAndWater { .. } | Pick::AirTowards { .. } => unreachable!("drawn by `pick`"),
        }
    }
    /// `HoverRandomPos.generateRandomPos(8, 7, 3, 1)`: the spot, raised 1..=3 blocks above a solid it is in, out of water.
    fn hover(&mut self, xd: f64, zd: f64, max_rad: f64) -> Option<[i32; 3]> {
        let d = self.direction_within(0.0, 8.0, 7, xd, zd, max_rad)?;
        let p = self.toward(d);
        let mw = self.mw;
        if !self.in_height(p) {
            return None;
        }
        // Not solid: the spot stays in the air (`moveUpToAboveSolid` returns it as is), as vanilla's `HoverRandomPos` keeps it.
        let above = self.rng.next_int(3) + 1;
        let p = self.move_up_to_above_solid(p, above);
        let water = mw.fluid(p[0], p[1], p[2]) == myriade_engine_lib::blocks::Fluid::Water;
        (!water && self.malus_is_zero(p)).then_some(p)
    }
    fn move_up_to_above_solid(&self, mut p: [i32; 3], above: i32) -> [i32; 3] {
        let mw = self.mw;
        if !mw.solid(p[0], p[1], p[2]) {
            return p;
        }
        p[1] += 1;
        while p[1] < mw.world.top_y() && mw.solid(p[0], p[1], p[2]) {
            p[1] += 1;
        }
        let first = p[1];
        while p[1] < mw.world.top_y() && p[1] - first < above {
            p[1] += 1;
            if mw.solid(p[0], p[1], p[2]) {
                p[1] -= 1;
                break;
            }
        }
        p
    }
    /// `AirAndWaterRandomPos.generateRandomPos(8, 4, -2)`.
    fn air_and_water(&mut self, xd: f64, zd: f64, max_rad: f64) -> Option<[i32; 3]> {
        self.air_and_water_p(8, 4, -2, xd, zd, max_rad)
    }
    fn air_and_water_p(&mut self, h: i32, v: i32, fly: i32, xd: f64, zd: f64, max_rad: f64) -> Option<[i32; 3]> {
        let d = self.direction_within_height(0.0, f64::from(h), v, fly, xd, zd, max_rad)?;
        let mut p = self.toward(d);
        if !self.in_height(p) {
            return None;
        }
        let mw = self.mw;
        if mw.solid(p[0], p[1], p[2]) {
            p[1] += 1;
            while p[1] < mw.world.top_y() && mw.solid(p[0], p[1], p[2]) {
                p[1] += 1;
            }
        }
        self.malus_is_zero(p).then_some(p)
    }
    /// `LandRandomPos.getPosInDirection`: ten tries within a quarter turn of the direction `(xd, zd)`.
    fn land_in_direction(&mut self, min_h: f64, max_h: f64, v: i32, xd: f64, zd: f64) -> Option<[f64; 3]> {
        self.best_of_ten(|s| {
            let d = s.direction_within(min_h, max_h, v, xd, zd, f64::from(std::f32::consts::FRAC_PI_2))?;
            let p = s.toward(d);
            if !s.in_height(p) || s.restricted(max_h, p) || !s.stable_at(p) {
                return None;
            }
            s.land_up_out_of_solid(p)
        })
    }
    /// `PathfinderMob`-side `pickBest`: ten tries, the highest `walk_target_value` wins (the first on a tie).
    fn best_of_ten(&mut self, mut supplier: impl FnMut(&mut Self) -> Option<[i32; 3]>) -> Option<[f64; 3]> {
        let mut best: Option<[i32; 3]> = None;
        let mut best_weight = f64::NEG_INFINITY;
        for _ in 0..10 {
            if let Some(p) = supplier(self) {
                let lava = matches!(self.walk, WalkValue::Strider) && self.mw.fluid(p[0], p[1], p[2]) != myriade_engine_lib::blocks::Fluid::Lava;
                let v = if self.in_lava && lava { f64::NEG_INFINITY } else { f64::from(walk_target_value(self.mw, self.walk, p)) };
                if v > best_weight {
                    best_weight = v;
                    best = Some(p);
                }
            }
        }
        best.map(|p| [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5])
    }
    /// A position by one of the vanilla pickers, or `None` (the caller's goal does not start).
    pub fn pick(&mut self, pick: Pick) -> Option<[f64; 3]> {
        match pick {
            Pick::LandAway { min_h, max_h, v, from } => {
                let (mut xd, mut zd) = (self.pos[0] - from[0], self.pos[2] - from[2]);
                if xd == 0.0 && zd == 0.0 {
                    xd = self.rng.next_f64() - 0.5;
                    zd = self.rng.next_f64() - 0.5;
                }
                self.land_in_direction(min_h, max_h, v, xd, zd)
            }
            Pick::LandTowards { h, v, to } => {
                let (xd, zd) = (to[0] - self.pos[0], to[2] - self.pos[2]);
                self.land_in_direction(0.0, f64::from(h), v, xd, zd)
            }
            Pick::Flying { xd, zd } => {
                let max_rad = f64::from(std::f32::consts::FRAC_PI_2);
                self.best_of_ten(|s| s.hover(xd, zd, max_rad)).or_else(|| self.best_of_ten(|s| s.air_and_water(xd, zd, max_rad)))
            }
            Pick::AirAndWater { h, v, xd, zd } => self.best_of_ten(|s| s.air_and_water_p(h, v, -2, xd, zd, f64::from(std::f32::consts::FRAC_PI_2))),
            Pick::AirTowards { h, v, fly, to, max_rad } => {
                let (xd, zd) = (to[0] - self.pos[0], to[2] - self.pos[2]);
                self.best_of_ten(|s| {
                    let p = s.air_and_water_p(h, v, fly, xd, zd, max_rad)?;
                    (s.mw.fluid(p[0], p[1], p[2]) != myriade_engine_lib::blocks::Fluid::Water).then_some(p)
                })
            }
            other => self.best_of_ten(|s| s.candidate(other)),
        }
    }
}
/// Everything a system needs to build a `Spotter` for an entity.
pub fn spotter<'a>(level: &'a Level, pos: [f64; 3], nav: &'a NavParams, walk: WalkValue, rng: &'a mut Rng) -> Spotter<'a> {
    Spotter { home: None, mw: level.mw.0, pos, nav, walk, rng, stable: Stable::Ground, in_lava: false }
}
/// The call a module makes: a position for the entity `e` by the vanilla picker `pick`, drawn from its own random stream
/// (written back), with its position, its path maluses and its walk preference read from its components. `None` when the
/// picker finds nothing (the goal does not start) or the entity has no body or no walking.
pub fn pick_for(w: &mut World, e: Entity, pick: Pick) -> Option<[f64; 3]> {
    let pos = w.get::<crate::cow::Pos>(e)?.0;
    let nav = *w.get::<NavParams>(e)?;
    let walk = w.get::<WalkValue>(e).copied().unwrap_or(WalkValue::Zero);
    let level = *w.get_resource::<Level>()?;
    let mut rng = Rng(w.get::<Rng>(e)?.0);
    // The amphibious navigation (a turtle's) takes a spot with something other than air under it, in either medium.
    let amphibious = w.get::<crate::locomotion::swim::SwimParams>(e).is_some_and(|p| p.amphibious.is_some());
    let stable = if amphibious {
        Stable::Amphibious
    } else if w.get::<crate::locomotion::SwimMode>(e).is_some() {
        Stable::Water
    } else {
        Stable::Ground
    };
    let in_lava = w.get::<crate::cow::FluidState>(e).is_some_and(|f| f.in_lava);
    crate::pathsearch::adjacent_reset();
    let mut sp = spotter(&level, [pos.x, pos.y, pos.z], &nav, walk, &mut rng);
    sp.stable = stable;
    sp.in_lava = in_lava;
    sp.home = w.get::<Home>(e).copied();
    let out = sp.pick(pick);
    w.get_mut::<Rng>(e)?.0 = rng.0;
    out
}
