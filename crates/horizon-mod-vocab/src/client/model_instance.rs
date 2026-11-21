//! Instanced copies of one resolved model, for a single world view.
//!
//! `ModelInstance::tint` is `0xAARRGGBB`, the same packed colour as [`crate::WorldQuad::color`].
//! [`LightCoords`] is block and sky light, each `0..=15`. The engine converts tint and light
//! to the frame format. Mods never assume that format.

/// Block and sky light, each `0..=15`, in vanilla's convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub struct LightCoords {
    /// Block light, `0..=15`.
    pub block: u8,
    /// Sky light, `0..=15`.
    pub sky: u8,
}

impl LightCoords {
    /// Both channels at 15.
    pub const FULL_BRIGHT: Self = Self { block: 15, sky: 15 };

    /// `block` and `sky` each in `0..=15`, or `None`.
    pub fn new(block: u8, sky: u8) -> Option<Self> {
        (block <= 15 && sky <= 15).then_some(Self { block, sky })
    }
}

/// White-flash column and the hurt tint. Hurt hides the white flash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub struct Overlay {
    /// White flash column, `0..=15`. Zero is no flash.
    pub white: u8,
    /// The hurt red.
    pub hurt: bool,
}

impl Overlay {
    /// No white flash and no hurt tint.
    pub const NONE: Self = Self {
        white: 0,
        hurt: false,
    };

    /// `white` in `0..=15`, or `None`.
    pub fn new(white: u8, hurt: bool) -> Option<Self> {
        (white <= 15).then_some(Self { white, hurt })
    }
}

/// Row-major 3×4 identity, local to a copy's origin.
pub const IDENTITY_TRANSFORM: [f32; 12] =
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// One copy. `origin` is world blocks; `transform` is row-major 3×4 in that local space.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub struct ModelInstance {
    /// World-space origin, in blocks. The camera is subtracted in `f64`.
    pub origin: [f64; 3],
    /// Row-major 3×4 affine transform, applied before `origin`.
    pub transform: [f32; 12],
    /// Packed `0xAARRGGBB`, the same convention as [`crate::WorldQuad::color`].
    pub tint: u32,
    /// Block and sky light in vanilla's `0..=15` convention.
    pub light: LightCoords,
    /// Hurt tint and white flash, in vanilla's overlay convention.
    pub overlay: Overlay,
}

impl ModelInstance {
    /// One copy. The draw call checks finiteness, light range and the view budget.
    #[inline]
    pub fn new(
        origin: [f64; 3],
        transform: [f32; 12],
        tint: u32,
        light: LightCoords,
        overlay: Overlay,
    ) -> Self {
        Self {
            origin,
            transform,
            tint,
            light,
            overlay,
        }
    }

    /// Identity transform at `origin`, full bright, with no overlay.
    #[inline]
    pub fn at(origin: [f64; 3], tint: u32) -> Self {
        Self::new(
            origin,
            IDENTITY_TRANSFORM,
            tint,
            LightCoords::FULL_BRIGHT,
            Overlay::NONE,
        )
    }
}

/// Why a whole instance batch was refused. Nothing from that batch is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub enum InstanceError {
    /// The handle is stale or not a resolved standalone model.
    Model(crate::ModelError),
    /// An origin or transform component is not finite.
    NonFinite,
    /// A light channel is outside `0..=15`.
    InvalidLight,
    /// The batch would exceed this view's 16,384 instance budget.
    CapacityExceeded,
    /// The model has a translucent face and is not drawn as solid.
    Translucent,
}

/// Persistent copies of one resolved model, retired on reload, world close, mod disable or destroy.
/// Only the creating mod may access or submit the group; others receive [`GroupError::Stale`].
/// Retired handles never become valid again, even when their storage slot is reused.
/// Native and wasm mods use the same storage, ownership and drawing rules.
/// Static groups cache CPU conversion; all visible GPU-routed copies still upload every frame.
/// Frequent group pose changes rebuild the live cache and can cost more than transient copies.
// no wire: decoding from_parts rejects generation zero (an unpublished instance group handle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
// exhaustive: only the engine creates handles
pub struct InstanceGroupRef(pub(crate) u64, pub(crate) u32);

impl InstanceGroupRef {
    pub(crate) fn from_parts(generation: u64, slot: u32) -> Result<Self, &'static str> {
        if generation == 0 {
            return Err("unpublished instance group generation");
        }
        Ok(Self(generation, slot))
    }
}

/// Why a persistent group operation was refused. Errors leave storage and view unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub enum GroupError {
    /// The group belongs to another mod, was destroyed, its owner was disabled,
    /// or its world/resources retired. Re-enabling or slot reuse cannot revive it.
    Stale,
    /// The model could not be resolved.
    Model(crate::ModelError),
    /// An origin or transform component is not finite.
    NonFinite,
    /// A light channel is outside 0..=15.
    InvalidLight,
    /// The range exceeds reserved capacity.
    OutOfRange,
    /// The mod's reserved capacity or the view budget would be exceeded.
    CapacityExceeded,
    /// Translucent models cannot use this route.
    Translucent,
}

impl From<InstanceError> for GroupError {
    fn from(error: InstanceError) -> Self {
        match error {
            InstanceError::Model(error) => Self::Model(error),
            InstanceError::NonFinite => Self::NonFinite,
            InstanceError::InvalidLight => Self::InvalidLight,
            InstanceError::CapacityExceeded => Self::CapacityExceeded,
            InstanceError::Translucent => Self::Translucent,
        }
    }
}

impl crate::__private::game::Fields for InstanceGroupRef {
    type Fields<'a> = crate::__private::game::ModelRefFields;
    fn fields(&self) -> Self::Fields<'_> {
        crate::__private::game::ModelRefFields {
            generation: self.0,
            slot: self.1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_overlay_constructors_reject_a_channel_past_15() {
        assert!(LightCoords::new(16, 0).is_none());
        assert!(Overlay::new(16, false).is_none());
        assert_eq!(
            ModelInstance::at([1.0, 2.0, 3.0], 0xFF80_C040).tint,
            0xFF80_C040
        );
    }
}
