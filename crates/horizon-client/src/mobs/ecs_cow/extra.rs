//! The part of `Extra` a cow reads and writes (`Extra` itself holds non-`Sync` attribute state and cannot be a component).
#[derive(Clone, Debug, Default)]
pub struct CowExtra {
    /// `AgeableMob.age`: negative for a baby, counting up; positive is the breeding pause.
    pub age: i32,
    pub in_love: i32,
    pub forced_age_timer: i32,
    pub age_locked: bool,
    pub variant: u8,
}
