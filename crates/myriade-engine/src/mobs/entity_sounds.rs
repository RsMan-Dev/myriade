//! Sound slots retain dynamic vanilla selection when inherited.
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default)]
pub enum Selection {
    #[default]
    Inherit,
    Id(&'static str),
    Mute,
}
impl Selection {
    /// No lookup or allocation for inherited or muted slots.
    pub fn select(self, inherited: &'static str) -> &'static str {
        match self {
            Self::Inherit => inherited,
            Self::Id(id) => id,
            Self::Mute => "",
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Sounds {
    pub ambient: Selection,
    pub hurt: Selection,
    pub death: Selection,
    pub step: Selection,
}
/// Namespace normalization at audio/network boundaries. Full ids are borrowed unchanged.
pub fn full_id(id: &str) -> Cow<'_, str> {
    if id.contains(':') {
        Cow::Borrowed(id)
    } else {
        Cow::Owned(format!("minecraft:{id}"))
    }
}

pub(crate) fn intern_full_id(id: &str) -> &'static str {
    use std::{collections::HashSet, sync::Mutex};
    static IDS: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
    let mut ids = IDS.lock().unwrap_or_else(|e| e.into_inner());
    let ids = ids.get_or_insert_with(HashSet::new);
    if let Some(id) = ids.get(id) {
        return id;
    }
    let id = Box::leak(id.to_owned().into_boxed_str());
    ids.insert(id);
    id
}


