//! Pandas (`Panda`): the genes and what they show, the sit, lie-on-back, roll
//! and sneeze states with their goals, the unhappy and eat counters, and the
//! client side of the three amounts `PandaModel.setupAnim` reads.
//!
//! `extra.variant` and `extra.variant2` are the main and the hidden gene
//! (`MAIN_GENE_ID`, `HIDDEN_GENE_ID`); the rest is [`State`], whose `flags`,
//! `unhappy`, `sneeze` and `eat` are the synched data a server sends.
use super::{
    MobEvent, MobWorld, Species, Who,
    body_anim::Panda as Pose,
    finalize::{self, Attr},
    goals::{
        Avoid, AvoidEntity, Breed, Cx, Float, FollowParent, Goal, HurtByTarget, JUMP, LOOK, MOVE,
        MeleeAttack, Panic, RandomLookAround, Stroll, Tempt, goal_clone, reduced,
    },
};
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

#[derive(Clone, Debug, Default)]
pub struct State {
    /// `DATA_ID_FLAGS` (sneezing, rolling, sitting, on its back).
    pub flags: u8,
    /// `UNHAPPY_COUNTER`, `SNEEZE_COUNTER` and `EAT_COUNTER`.
    pub unhappy: i32,
    pub sneeze: i32,
    pub eat: i32,
    got_bamboo: bool,
    did_bite: bool,
    /// `rollCounter`, which a client counts for itself.
    pub roll_counter: i32,
    roll_delta: [f64; 2],
    /// `sitAmount`, `onBackAmount` and `rollAmount` with the tick before.
    sit_amount: [f32; 2],
    on_back_amount: [f32; 2],
    roll_amount: [f32; 2],
    /// `isScared` as of the last tick (the model and the item layer read it).
    scared: bool,
    /// `PandaLookAtPlayerGoal.lookAt`, which the breed goal can set.
    look_at: Option<Who>,
    /// What the game saw on the ground: any food within 6 blocks, and the
    /// first within 8.
    pub food_near: bool,
    pub food_at: Option<[f64; 3]>,
    /// The size of the stack in its mouth (the mob holds one item id).
    pub held_count: u32,
    /// What a window draws of the last bite and sneeze, taken by `effects`.
    crumbs: Option<Crumbs>,
    sneezed: Option<([f64; 3], [f64; 3])>,
}


// ---- genes ----------------------------------------------------------------

/// `Panda.Gene.getRandom`.
fn random_gene(random: &mut impl RandomSource) -> u8 {
    finalize::panda_gene(random.next_int_bound(16).unwrap_or(0))
}
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
impl<G: Goal + Clone + 'static> Goal for When<G> {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.goal.flags()
    }
    fn name(&self) -> &'static str {
        self.goal.name()
    }
    fn interruptable(&self) -> bool {
        self.goal.interruptable()
    }
    fn every_tick(&self) -> bool {
        self.goal.every_tick()
    }
}

/// `PandaPanicGoal`: panics at the environment only, and not while sitting.
#[derive(Clone, Debug)]
struct PandaPanic(Panic);
impl Goal for PandaPanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn name(&self) -> &'static str {
        "Panic"
    }
}

/// `PandaBreedGoal`: a panda in love finds no mate happy unless there is
/// bamboo about.
#[derive(Clone, Debug)]
struct PandaBreed {
    breed: Breed,
    unhappy_cooldown: i64,
}
impl PandaBreed {
}
impl Goal for PandaBreed {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn name(&self) -> &'static str {
        "Breed"
    }
}

/// `PandaHurtByTargetGoal`: a panda that was fed bamboo or bit lets go.
#[derive(Clone, Debug)]
struct PandaHurtBy(HurtByTarget);
impl Goal for PandaHurtBy {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        self.0.name()
    }
}


/// `PandaSitGoal`: sit down with bamboo or cake in the mouth, or go to get
/// some from the ground.
#[derive(Clone, Debug, Default)]
struct PandaSit {
    cooldown: i64,
}
impl Goal for PandaSit {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}

/// `PandaLieOnBackGoal`: a lazy panda now and then lies down.
#[derive(Clone, Debug, Default)]
struct PandaLieOnBack {
    cooldown: i64,
}
impl Goal for PandaLieOnBack {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}

/// `PandaSneezeGoal`: a cub sneezes now and then, a weak one more.
#[derive(Clone, Debug)]
struct PandaSneeze;
impl Goal for PandaSneeze {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}

/// `PandaLookAtPlayerGoal`: looks at the nearest player, or at the one the
/// breed goal sulks at.
#[derive(Clone, Debug, Default)]
struct PandaLook {
    look_time: i32,
}
impl PandaLook {
}
impl Goal for PandaLook {
    goal_clone!();
    fn flags(&self) -> u8 {
        LOOK
    }
    fn name(&self) -> &'static str {
        "LookAtPlayer"
    }
}

/// `PandaRollGoal`: a cub, or a playful panda, rolls over when it faces a
/// drop and now and then at random.
#[derive(Clone, Debug)]
struct PandaRoll;
impl Goal for PandaRoll {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK | JUMP
    }
    fn interruptable(&self) -> bool {
        false
    }
}


#[cfg(test)]
mod look_at_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};

}

#[cfg(test)]
mod freeze_panic_tests {
    use super::*;

}
