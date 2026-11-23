use crate::*;
use std::sync::OnceLock;

/// A particle requested at registration, resolved once when providers load.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParticleKey(pub u32);

impl ParticleKey {
}
