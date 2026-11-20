//! Constants of the dragon's body, hurt rules and contacts, copied from Horizon's `game/mobs/dragon.rs` (`D:line`), which keeps them
//! private. Phase ids are the jar's `EnderDragonPhase` declaration order (0-10), to confirm with L3.
use horizon_core::geometry::Aabb;

/// `EnderDragon` health (D:121).
pub const MAX_HEALTH: f32 = 200.0;
/// The part sizes (width, height) of head, neck, body, tail 1-3, wing 1-2 (`PART_SIZES`, D:596).
pub const PART_SIZES: [(f64, f64); 8] = [
    (1.0, 1.0),
    (3.0, 3.0),
    (5.0, 3.0),
    (2.0, 2.0),
    (2.0, 2.0),
    (2.0, 2.0),
    (4.0, 2.0),
    (4.0, 2.0),
];
pub const HEAD: usize = 0;
pub const NECK: usize = 1;
pub const BODY: usize = 2;
pub const WING_1: usize = 6;
pub const WING_2: usize = 7;
/// Bit `j` of entry `i`: flight nodes `i` and `j` are linked (`ADJACENCY`, D:606). Read by the flight nodes (L2).
pub const ADJACENCY: [u32; 24] = [
    6146, 8197, 8202, 16404, 32808, 32848, 65696, 131392, 131712, 263424, 526848, 525313, 1581057, 3166214, 2138120, 6373424,
    4358208, 12910976, 9044480, 9706496, 15216640, 13688832, 11763712, 8257536,
];

pub const TAKEOFF: u8 = 4;
pub const SITTING_FLAMING: u8 = 5;
pub const SITTING_SCANNING: u8 = 6;
pub const SITTING_ATTACKING: u8 = 7;
pub const DYING: u8 = 9;
pub const HOVERING: u8 = 10;
/// `DragonHoverPhase.isSitting` is true in the jar too: the perch is these four phases.
pub const fn is_sitting(phase: u8) -> bool {
    matches!(phase, SITTING_FLAMING | SITTING_SCANNING | SITTING_ATTACKING | HOVERING)
}

/// `LivingEntity.hurtServer`: within this many ticks of a hit, only the excess over the last hit lands (`invulnerable > 10`).
pub const HURT_WINDOW: i32 = 10;
/// The window a hit that lands outside the old one opens (`invulnerableTime`, to confirm in `DamageContainer`).
pub const HURT_INVULNERABLE: i32 = 20;
/// `hurt_time` after a full hit: the wings and heads do not touch a player meanwhile (`recently_hurt`).
pub const HURT_TIME: i32 = 10;
/// A hit below this is no hit (`EnderDragon.hurt`).
pub const MIN_DAMAGE: f32 = 0.01;
/// `dealDefaultKnockback` strength of a full melee hit (`0.4F`).
pub const KNOCKBACK: f32 = 0.4;
/// A quarter of the health taken while perched takes the dragon off (D:910, `sitting_damage`).
pub const PERCH_LIMIT: f32 = 0.25 * MAX_HEALTH;

/// Wing contact: the zone inflated by (4, 2, 4), moved down 2; push 4 away from the body's centre and 0.2 up; 5 damage.
pub const WING_ZONE: [f64; 3] = [4.0, 2.0, 4.0];
pub const WING_DROP: f64 = 2.0;
pub const WING_PUSH: f64 = 4.0;
pub const WING_LIFT: f64 = 0.2;
pub const WING_DAMAGE: f32 = 5.0;
/// Head and neck contact: the box inflated by 1; 10 damage.
pub const HEAD_ZONE: f64 = 1.0;
pub const HEAD_DAMAGE: f32 = 10.0;

/// `Entity.getBoundingBox` of the whole dragon: 8 blocks wide and high, its eye at 0.85 of its height (`EnderDragon.hurt`, D:968).
pub const WHOLE: f64 = 8.0;
pub const EYE: f32 = 8.0 * 0.85;

/// The dragon's whole box, the one an explosion hits and a contact reads (`EnderDragon.getBoundingBox`).
pub fn whole_box(feet: horizon_core::geometry::Vec3) -> Aabb {
    use horizon_core::geometry::Vec3;
    Aabb::new(
        Vec3::new(feet.x - WHOLE, feet.y, feet.z - WHOLE),
        Vec3::new(feet.x + WHOLE, feet.y + WHOLE, feet.z + WHOLE),
    )
}
