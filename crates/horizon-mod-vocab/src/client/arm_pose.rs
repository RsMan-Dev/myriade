//! `HumanoidModel.ArmPose`: the names an item declaration gives the vanilla arm poses.

/// `HumanoidModel.ArmPose`'s constants, the arm poses a humanoid model has.
#[derive(
    horizon_mod_api_macros::Wire,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    serde::Deserialize,
    serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
// exhaustive: vanilla constants
pub enum ArmPose {
    /// No held item pose.
    Empty,
    /// Ordinary held item.
    Item,
    /// Blocking with a shield.
    Block,
    /// Drawing a bow.
    #[serde(rename = "bow_and_arrow")]
    Bow,
    /// Holding a loaded crossbow.
    CrossbowHold,
    /// Charging a crossbow.
    CrossbowCharge,
    /// Raising a trident to throw it (or to launch with Riptide).
    ThrowTrident,
    /// Playing a goat horn.
    TootHorn,
    /// Looking through a spyglass (`HumanoidModel.ArmPose.SPYGLASS`).
    Spyglass,
    /// A spear held out (`HumanoidModel.ArmPose.SPEAR`), at rest or charging.
    Spear,
    /// A brush being used (`HumanoidModel.ArmPose.BRUSH`).
    Brush,
}
