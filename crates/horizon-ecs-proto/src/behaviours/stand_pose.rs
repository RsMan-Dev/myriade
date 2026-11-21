//! The pose of a stand, part by part (`ArmorStand.setHeadPose` and the five others). Not a `Module`: a stand has no goals, and its pose
//! changes by a player's or a command's write, never by a tick. The state is `stand::Pose`, kept only while it differs from the default.
use crate::stand::{pose_of, set_pose};
use bevy_ecs::prelude::*;

/// The six parts, in the order of the jar's `DATA_*_POSE` and of `stand::DEFAULT_POSE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Head,
    Body,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

/// The angles (x, y, z, degrees) of a part.
pub fn get(w: &World, e: Entity, part: Part) -> [f32; 3] {
    pose_of(w, e)[part as usize]
}

/// Writes one part's angles; the other parts keep theirs.
pub fn set(w: &mut World, e: Entity, part: Part, angles: [f32; 3]) {
    let mut pose = pose_of(w, e);
    pose[part as usize] = angles;
    set_pose(w, e, pose);
}
