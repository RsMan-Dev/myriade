//! The outputs toward the host of the lightning, the end crystals, the fangs and the area clouds, read with `EcsWorld::drain_misc`. Merged
//! into `HostEvent::Misc` by the session (host_out.rs is not edited here).
use crate::effects::Kind;
use crate::targeting::Who;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MiscEvent {
    /// A bolt struck at `at`; `cosmetic` for a skeleton trap's, which strikes nothing but shows.
    Lightning { at: [f64; 3], cosmetic: bool },
    /// An end crystal went at `at` (a blow destroyed it).
    CrystalBlast { at: [f64; 3] },
    /// A fang bit at `at` (its bite, hurt or not: the host shows it).
    FangsBite { at: [f64; 3] },
    /// A timed effect of a potion cloud for the player `target` (a creature's is added in P): `effect` at `amplifier` for `ticks`, `hidden` and
    /// `ambient` as the cloud gives it, `source` and `owner` the cloud's owner (the jar's `addEffect(instance, getOwner())`).
    CloudEffect { target: Who, effect: Kind, amplifier: i32, ticks: i32, hidden: bool, ambient: bool, source: Option<Who>, owner: Option<Who> },
}
