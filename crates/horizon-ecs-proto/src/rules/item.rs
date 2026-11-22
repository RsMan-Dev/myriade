//! The numbers of a dropped stack (`ItemEntity`): its box, its life, its pickup delay, its merge rate and its throw.
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::RandomSource;

/// The size of the box of an item (`EntityType.ITEM`).
pub const SIZE: f64 = 0.25;
/// `ItemEntity.getDespawnTime`'s 6 000 ticks.
pub const LIFE: u64 = 6000;
/// The wheel reaches 4 095 ticks: a longer wait is a deadline at its end, which waits on.
const REACH: u64 = 4000;

/// The box of an item at `p`.
pub fn box_at(p: Vec3) -> Aabb {
    let h = SIZE / 2.0;
    Aabb::new(Vec3::new(p.x - h, p.y, p.z - h), Vec3::new(p.x + h, p.y + SIZE, p.z + h))
}

/// The tick an item's life ends at: its age base plus `LIFE`, kept within the wheel's reach and after `now`.
pub fn expire_at(age_base: i64, now: u64) -> u64 {
    (age_base.saturating_add(LIFE as i64).min((now + REACH) as i64).max((now + 1) as i64)) as u64
}

/// The wait to the item's next check: the next multiple of 40 ticks of its age in its block.
pub fn check_wait(born: u64, now: u64) -> u64 {
    40 - (now - born) % 40
}

/// The pickup delay as a tick count: None when it is set never to be picked up (`i16::MAX` or `u32::MAX`).
pub fn pickup_ready(delay: i64) -> Option<u64> {
    if delay == i64::from(i16::MAX) || delay == i64::from(u32::MAX) { None } else { Some(delay.max(0) as u64) }
}

/// Whether a merge is due at `ticks`: every 2 ticks while the item changes block, every 40 otherwise.
pub fn merge_due(ticks: u64, changed: bool) -> bool {
    ticks % if changed { 2 } else { 40 } == 0
}

/// Whether the stack `theirs` and the stack `mine` fit one stack of `max` together, and `theirs` is below it.
pub fn fits_stack(theirs: u32, mine: u32, max: u32) -> bool {
    theirs < max && theirs + mine <= max
}

/// `ItemStack.getMaxStackSize`: the stack's own `max_stack_size` component, else its item's (64 when the data has none).
pub fn max_stack(mw: &MobWorld<'static>, s: &ItemStack) -> u32 {
    s.own_max_stack_size().unwrap_or_else(|| mw.items.and_then(|i| i.get(&s.item)).map_or(64, |i| i.max_stack)).max(1)
}

/// The velocity an item is thrown with: `nextDouble() * 0.2 - 0.1` sideways, 0.2 up (`ItemEntity`'s constructor).
pub fn throw_velocity(random: &mut impl RandomSource) -> Vec3 {
    Vec3::new(random.next_double() * 0.2 - 0.1, 0.2, random.next_double() * 0.2 - 0.1)
}
