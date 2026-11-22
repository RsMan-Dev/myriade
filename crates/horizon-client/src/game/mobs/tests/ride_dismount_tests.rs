//! `/ride`: mounting keeps the rider's view (`ServerPlayer.startRiding`
//! teleports with `Relative.ROTATION`), a mob that boards a mount after its
//! player takes the seat after the player (`Entity.addPassenger` puts a player
//! first), and getting off follows the vehicle's own
//! `getDismountLocationForPassenger`.
use super::*;

/// A stone floor whose top is y 64, and the blocks listed above it.
struct Ground {
    solid: Vec<[i32; 3]>,
}

impl Ground {
    fn with(blocks: &[[i32; 3]]) -> Self {
        Self {
            solid: blocks.to_vec(),
        }
    }
    fn solid(&self, p: [i32; 3]) -> bool {
        p[1] == 63 || self.solid.contains(&p)
    }
}

impl DismountWorld for Ground {
    fn lava(&self, _: [i32; 3]) -> bool {
        false
    }
    /// `BlockGetter.getBlockFloorHeight`: a full block is its own top, the
    /// block above one is the floor at 0, and air over air has none.
    fn floor_height(&self, p: [i32; 3]) -> f64 {
        if self.solid(p) {
            1.0
        } else if self.solid([p[0], p[1] - 1, p[2]]) {
            0.0
        } else {
            f64::NEG_INFINITY
        }
    }
    fn can_dismount_to(&self, bounds: Aabb) -> bool {
        let (lo, hi) = (bounds.min_position(), bounds.max_position());
        let cells = |a: f64, b: f64| a.floor() as i32..=(b - 1.0e-7).floor() as i32;
        !cells(lo.x, hi.x)
            .any(|x| cells(lo.y, hi.y).any(|y| cells(lo.z, hi.z).any(|z| self.solid([x, y, z]))))
    }
}

/// A mount of `species`, `width` wide and `height` tall, standing at `at`.
fn mount(species: Species, width: f32, height: f64, at: [f64; 3], yaw: f32) -> Mount {
    let half = f64::from(width) / 2.0;
    Mount {
        species,
        position: Vec3::new(at[0], at[1], at[2]),
        bounds: Aabb::new(
            Vec3::new(at[0] - half, at[1], at[2] - half),
            Vec3::new(at[0] + half, at[1] + height, at[2] + half),
        ),
        width,
        yaw,
    }
}

/// A right-handed player facing `yaw`.
fn player(yaw: f32) -> Rider {
    Rider {
        yaw,
        width: 0.6,
        poses: DISMOUNT_POSES
            .iter()
            .map(|&pose| (pose, pose.dimensions(1.0)))
            .collect(),
        main_arm_left: false,
    }
}

/// `Entity.getDismountLocationForPassenger`: a zombie has no rule of its own,
/// so the rider comes out on top of its box, not beside it.
#[test]
fn getting_off_a_zombie_lands_on_its_top() {
    let zombie = mount(Species::Zombie, 0.6, 1.95, [0.5, 64.0, 0.5], 0.0);
    let (at, pose) = dismount_location(&zombie, &player(0.0), &Ground::with(&[]));
    assert_eq!(at, Vec3::new(0.5, 64.0 + 1.95, 0.5));
    assert_eq!(pose, None);
}

/// `AbstractHorse.getDismountLocationForPassenger`: a block in the main hand's
/// column sends the rider up onto it, not round to the other side.
#[test]
fn getting_off_a_horse_steps_up_on_its_main_hand_side() {
    let width = 1.4f32;
    let horse = mount(Species::Horse, width, 1.6, [0.5, 64.0, 0.5], 0.0);
    let ground = Ground::with(&[[-1, 64, 0]]);
    let (at, pose) = dismount_location(&horse, &player(0.0), &ground);
    // Right hand of a south-facing horse: one block west, at the edge of the
    // square its box spans.
    let step = escape_vector(f64::from(width), f64::from(0.6f32), 90.0);
    assert!((step.x + 1.000_005).abs() < 1.0e-6, "{step:?}");
    assert!(step.z.abs() < 1.0e-6, "{step:?}");
    assert_eq!(at, Vec3::new(0.5 + step.x, 65.0, 0.5 + step.z));
    assert_eq!(pose, Some(Pose::Standing));
}

/// `Animal.getDismountLocationForPassenger`: the blocks round the pig in its
/// own facing's order, at its own height.
#[test]
fn getting_off_a_pig_steps_to_the_block_beside_its_facing() {
    let pig = mount(Species::Pig, 0.9, 0.9, [0.5, 64.0, 0.5], 0.0);
    let ground = Ground::with(&[[-1, 64, 0]]);
    let (at, pose) = dismount_location(&pig, &player(0.0), &ground);
    // Facing south: the right-hand block is west and taken, so the left one.
    assert_eq!(at, Vec3::new(1.5, 64.0, 0.5));
    assert_eq!(pose, Some(Pose::Standing));
}
