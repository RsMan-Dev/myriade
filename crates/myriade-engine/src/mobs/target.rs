//! Target writes and the species-specific side effects of `setTarget`.
use super::{
    MobEvent, Species, Who, enderman,
    finalize::{self, Attr, Op},
    fox,
};
use horizon_core::random::RandomSource;

