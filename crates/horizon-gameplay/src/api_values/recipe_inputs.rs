//! Read-only recipe inputs: resident inventory limits apply to writes, not predicates.
use super::{ApiStack, EngineStack, StackWriteError, encode_stack};
use horizon_items::items::Items;

/// Validate registry membership once while retaining the registry's shared item identity.
/// Counts and damage are API values, not attempted inventory writes.
pub fn stack(value: &ApiStack, items: &Items) -> Result<EngineStack, StackWriteError> {
    let (name, _) = items
        .items
        .get_key_value(value.item())
        .ok_or(StackWriteError::UnknownItem)?;
    encode_stack(value, name.clone())
}
