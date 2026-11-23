use horizon_core::{
    geometry::{
        Vec3,
        trig::{DEG_TO_RAD, Mth},
    },
    jmath,
    random::{EntityRandom, RandomSource},
};

/// `Panda.Gene` ids (the order of `Gene.values()`).
pub const LAZY: u8 = 1;
pub const WORRIED: u8 = 2;
pub const PLAYFUL: u8 = 3;
pub const WEAK: u8 = 5;
pub const AGGRESSIVE: u8 = 6;
/// `Gene.getSerializedName`, by id.
pub const GENES: [&str; 7] = [
    "normal",
    "lazy",
    "worried",
    "playful",
    "brown",
    "weak",
    "aggressive",
];

/// `DATA_ID_FLAGS`.
pub const FLAG_SNEEZE: u8 = 2;
pub const FLAG_ROLL: u8 = 4;
pub const FLAG_SIT: u8 = 8;
pub const FLAG_ON_BACK: u8 = 16;

/// The crumbs of one bite: where and how fast each one flies.
pub type Crumbs = Vec<([f64; 3], [f64; 3])>;



// ---- genes ----------------------------------------------------------------

/// `Panda.Gene.CODEC` for `MainGene` and `HiddenGene`.
pub fn gene_from_name(name: &str) -> u8 {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    GENES.iter().position(|g| *g == name).unwrap_or(0) as u8
}

// ---- state ----------------------------------------------------------------


// ---- synched data ---------------------------------------------------------


// ---- the tick --------------------------------------------------------------

fn lerp(a: f32, o: f32, n: f32) -> f32 {
    o + a * (n - o)
}
/// `updateSitAmount`, `updateOnBackAnimation` and `updateRollAmount`.
fn step_amount(amount: &mut [f32; 2], on: bool) {
    amount[0] = amount[1];
    amount[1] = if on {
        (amount[1] + 0.15).min(1.0)
    } else {
        (amount[1] - 0.19).max(0.0)
    };
}
/// `afterSneeze`'s particle, from the nose.
fn sneeze_particle(
    at: [f64; 3],
    width: f32,
    body_yaw: f32,
    eye: f64,
    movement: Vec3,
) -> ([f64; 3], [f64; 3]) {
    let reach = f64::from(width + 1.0) * 0.5;
    let angle = body_yaw * DEG_TO_RAD;
    (
        [
            at[0] - reach * f64::from(angle.msin()),
            eye - f64::from(0.1_f32),
            at[2] + reach * f64::from(angle.mcos()),
        ],
        [movement.x, 0.0, movement.z],
    )
}

// ---- drawing ---------------------------------------------------------------


// ---- goals -----------------------------------------------------------------

/// A goal that starts only when `when` allows it (`PandaAttackGoal`,
/// `PandaAvoidGoal`).
#[derive(Clone, Debug)]
struct When<G> {
    goal: G,}





/// `PandaSitGoal`: sit down with bamboo or cake in the mouth, or go to get
/// some from the ground.
#[derive(Clone, Debug, Default)]
struct PandaSit {
    cooldown: i64,
}

/// `PandaLieOnBackGoal`: a lazy panda now and then lies down.
#[derive(Clone, Debug, Default)]
struct PandaLieOnBack {
    cooldown: i64,
}

/// `PandaSneezeGoal`: a cub sneezes now and then, a weak one more.
#[derive(Clone, Debug)]
struct PandaSneeze;

/// `PandaLookAtPlayerGoal`: looks at the nearest player, or at the one the
/// breed goal sulks at.
#[derive(Clone, Debug, Default)]
struct PandaLook {
    look_time: i32,
}
impl PandaLook {
}

/// `PandaRollGoal`: a cub, or a playful panda, rolls over when it faces a
/// drop and now and then at random.
#[derive(Clone, Debug)]
struct PandaRoll;


#[cfg(test)]
mod look_at_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};

}

#[cfg(test)]
mod freeze_panic_tests {
    use super::*;

}
