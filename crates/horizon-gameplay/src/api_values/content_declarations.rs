//! Effect payload admission for item stacks.
#[path = "arm_pose.rs"]
mod arm_pose;

pub fn validate_decoded_effects(
    patch: Option<&myriade_types::Tag>,
    stack: &horizon_items::items::ItemStack,
) -> Result<(), myriade_types::StackWriteError> {
    // The component codec already decoded each supplied value. Check that admission
    // succeeded rather than parsing untrusted component payloads a second time.
    if let Some(patch) = patch
        && ((patch.get("minecraft:consumable").is_some() && stack.consumable(None).is_none())
            || (patch.get("minecraft:death_protection").is_some()
                && stack.death_protection(None).is_none()))
    {
        return Err(myriade_types::StackWriteError::InvalidEffectPayload);
    }
    Ok(())
}
