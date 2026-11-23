use crate::*;


impl BlockKey {
}

impl FluidKey {
}

impl ItemKey {
}

impl EntityTypeKey {
}

impl EffectKey {
}

impl DimensionKey {
    /// `minecraft:overworld`.
    pub const OVERWORLD: Self = Self(0);
    /// `minecraft:the_nether`.
    pub const NETHER: Self = Self(1);
    /// `minecraft:the_end`.
    pub const END: Self = Self(2);

    #[cfg(any(not(target_arch = "wasm32"), test))]
    pub fn from_slot(slot: u32) -> Result<Self, &'static str> {
        let key = Self(slot);
        dimension_id(key).ok_or("unknown DimensionKey slot")?;
        Ok(key)
    }
}

/// The key of dimension `id`, numbering it when the game meets it for the first time.
pub fn dimension_key(id: &str) -> DimensionKey {
    DimensionKey(crate::rt_registry_keys::dimension_slot(id))
}

/// The name of a dimension's key, None for a slot the game never gave out.
pub fn dimension_id(key: DimensionKey) -> Option<&'static str> {
    crate::rt_registry_keys::dimension_id(key.0)
}
