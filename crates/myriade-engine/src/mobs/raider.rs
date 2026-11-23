use horizon_core::nbt::Tag;
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};

/// The ominous banner is a white banner with the raid patterns; raiders wear
/// it as their head item.
pub const BANNER: &str = "minecraft:white_banner";

/// `PatrollingMonster` and `Raider` state (saved as `patrol_target`,
/// `PatrolLeader`, `Patrolling`, `Wave`, `CanJoinRaid`, `RaidId`).
#[derive(Clone, Debug, Default)]
pub struct State {
    pub patrol_leader: bool,
    pub patrolling: bool,
    pub patrol_target: Option<[i32; 3]>,
    /// Goal cooldown after a failed long distance path (game time).
    pub patrol_cooldown_until: i64,
    pub wave: i32,
    pub can_join_raid: bool,
    /// The raid this raider belongs to.
    pub raid_id: Option<i32>,
    pub ticks_outside_raid: i32,
    pub celebrating: bool,
    /// The raid is active (`hasActiveRaid`); set by the game every tick.
    pub active_raid: bool,
    /// A raid is active within 96 blocks (`ServerLevel.isRaided`); set by
    /// the game every tick for vindicators.
    pub raided: bool,
    /// The raid is won or lost (`Raid.isOver`), lost (`isLoss`).
    pub raid_over: bool,
    pub raid_loss: bool,
    pub raid_center: [i32; 3],
    /// Standing in or beside a village (`ServerLevel.isVillage`).
    pub in_village: bool,
    /// A dropped ominous banner this raider may fetch.
    pub banner_at: Option<[f64; 3]>,
    /// The item entity of `banner_at`.
    pub banner_item: u32,
    /// Banners with no path to them, and until when (game time):
    /// `unreachableBannerCache`.
    pub unreachable_banners: Vec<(u32, i64)>,
    /// `PathfindToRaidGoal.tick` ran its 20 tick recruitment: the game
    /// recruits the raiders around this one.
    pub recruit_due: bool,
    /// Beds around the raid's centre.
    pub homes: Option<std::sync::Arc<Vec<[i32; 3]>>>,
    /// A ravager carrying a raider, set every tick from `Extra::rider`.
    pub rider: bool,
    /// The raid counts this raider (`addWaveMob` ran since it joined or was
    /// loaded with its `RaidId`).
    pub registered: bool,
    /// `Raider.hurtServer` updates the raid's bar before the damage lands:
    /// when it was last hurt (see [`next_hurt_seq`]) and its health then.
    pub hurt_seq: u64,
    pub pre_hurt_health: f32,
}

static HURT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// A counter ordering the hurts of raiders across the game.
pub fn next_hurt_seq() -> u64 {
    HURT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}
impl State {
    /// A ravager carrying a raider (`hasControllingPassenger`).
    pub fn has_rider(&self) -> bool {
        self.rider
    }
}




/// `Raid.getOminousBannerInstance`.
pub fn ominous_banner() -> crate::items::ItemStack {
    let mut stack = crate::items::ItemStack::new(BANNER, 1);
    stack.data_mut().ominous_banner = true;
    stack
}

/// Why a mob was spawned (`EntitySpawnReason`), as far as raiders care.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Natural spawning: one leader in 17, and witches stay out of raids.
    Natural,
    /// Commands, eggs, conversions and other spawns: one leader in 17.
    Other,
    Patrol,
    Event,
}



/// `Vec3.yRot` (the angle is in radians, and the patrol goal passes 90).
fn y_rot(v: Vec3, angle: f32) -> Vec3 {
    let (cos, sin) = (f64::from(angle.cos()), f64::from(angle.sin()));
    Vec3::new(v.x * cos + v.z * sin, v.y, v.z * cos - v.x * sin)
}

/// `PatrollingMonster.LongDistancePatrolGoal`: walk toward the patrol target
/// in 10 block hops (curving around it), the leader dragging its companions.
#[derive(Clone, Debug)]
pub struct LongDistancePatrol {
    speed: f64,
    leader_speed: f64,
}
impl LongDistancePatrol {
    pub fn new(speed: f64, leader_speed: f64) -> Self {
        Self {
            speed,
            leader_speed,
        }
    }
}
/// A box that holds every mob whose position is within `(rx, ry, rz)` of
/// `at`, one block wider so its bounds (at most 1.95 wide) meet it too.
fn reach_box(at: Vec3, rx: f64, ry: f64, rz: f64) -> Aabb {
    Aabb::new(
        Vec3::new(at.x - rx - 1.0, at.y - ry - 1.0, at.z - rz - 1.0),
        Vec3::new(at.x + rx + 1.0, at.y + ry + 1.0, at.z + rz + 1.0),
    )
}
impl LongDistancePatrol {
}
/// `Vec3i.closerToCenterThan`.
fn close_to_center(p: [i32; 3], at: Vec3, distance: f64) -> bool {
    Vec3::new(
        f64::from(p[0]) + 0.5,
        f64::from(p[1]) + 0.5,
        f64::from(p[2]) + 0.5,
    )
    .distance_squared(at)
        < distance * distance
}

/// `Raider.ObtainRaidLeaderBannerGoal`: walk to a fallen ominous banner when
/// the wave has no living leader (the game finds the banner and picks it up).
#[derive(Clone, Debug, Default)]
pub struct ObtainBanner {
    path_to_banner: Option<super::path::Path>,
}
impl ObtainBanner {
}

/// `PathfindToRaidGoal`: outside the village, head for the raid's centre in
/// hops of up to 15 blocks, recruiting nearby raiders every 20 ticks.
#[derive(Clone, Debug, Default)]
pub struct PathfindToRaid {
    recruitment_tick: i32,
}
impl PathfindToRaid {
    fn valid(s: &State) -> bool {
        s.active_raid && !s.raid_over && !s.in_village
    }
}

/// `Raider.RaiderMoveThroughVillageGoal`: walk to a bed of the village not
/// visited lately, then onwards to the next one.
#[derive(Clone, Debug)]
pub struct MoveThroughVillage {
    speed: f64,
    distance: f64,
    poi: Option<[i32; 3]>,
    visited: Vec<[i32; 3]>,
    stuck: bool,
}
impl MoveThroughVillage {
    pub fn new(speed: f64, distance: f64) -> Self {
        Self {
            speed,
            distance,
            poi: None,
            visited: Vec::new(),
            stuck: false,
        }
    }
}

/// `Raider.RaiderCelebration`: after a lost village, cheer and jump.
#[derive(Clone, Debug)]
pub struct Celebration;

/// `Raider.HoldGroundAttackGoal`: while patrolling, stand still and let the
/// patrol shout when something shows up; attack once it comes close.
#[derive(Clone, Debug)]
pub struct HoldGroundAttack {
    hostile_radius_sqr: f64,
}
impl HoldGroundAttack {
    pub fn new(radius: f64) -> Self {
        Self {
            hostile_radius_sqr: radius * radius,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raiders_and_leaders() {
        assert!(is_raider(Species::Ravager) && is_raider(Species::Witch));
        assert!(!is_raider(Species::Zombie));
        assert!(can_be_leader(Species::Pillager) && can_be_leader(Species::Evoker));
        assert!(!can_be_leader(Species::Witch) && !can_be_leader(Species::Ravager));
    }
    #[test]
    fn long_distance_patrol_turns_by_ninety_radians() {
        // PatrollingMonster passes 90.0F to Vec3.yRot (radians).
        let v = y_rot(Vec3::new(10.0, 3.0, 0.0), 90.0);
        let (c, s) = (90f64.cos(), 90f64.sin());
        assert!((v.x - 10.0 * c).abs() < 1e-4 && (v.z + 10.0 * s).abs() < 1e-4);
        assert_eq!(v.y, 3.0);
    }
    #[test]
    fn closer_to_center_uses_block_centres() {
        let at = Vec3::new(10.5, 64.5, 10.5);
        assert!(close_to_center([10, 64, 10], at, 1.0));
        assert!(!close_to_center([30, 64, 10], at, 10.0));
    }


}
