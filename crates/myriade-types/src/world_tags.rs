use crate::*;


/// A process-lifetime tag name resolved once through [`Tags::key`].
/// Membership belongs to the current authoritative data reload, not to the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TagKey(pub u32);

impl TagKey {
    #[cfg(any(not(target_arch = "wasm32"), test))]
    pub fn from_slot(slot: u32) -> Result<Self, &'static str> {
        crate::rt_registry_keys::with_tag_name(slot, |id| {
            id.map(|_| Self(slot)).ok_or("unknown TagKey slot")
        })
    }
}

/// Read merged block, item and fluid tags without borrowing the game's registry structs.
/// Unknown tags answer false or an empty member list. A client reads the tags its server
/// sent. Membership reads answer `None` before authoritative tags are loaded, on a client
/// before its server's are applied, or while their key table is unavailable.
pub trait Tags {
}

/// Borrow a tag's canonical name within the runtime's read lock, without formatting or cloning.
pub fn with_tag_name<R>(key: TagKey, read: impl FnOnce(Option<&str>) -> R) -> R {
    crate::rt_registry_keys::with_tag_name(key.0, read)
}
