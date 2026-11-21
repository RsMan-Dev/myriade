//! Values of the villager's gossips (`GossipContainer`, `Villager.gossip`): the kinds and their weights, the caps, the daily decay,
//! the transfer and the draw of the gossips a villager passes on.

pub const DAY: u64 = 24000;
/// `Villager.gossip` : entendre à nouveau attend 1200 ticks, pour les deux.
pub const HEARD_WAIT: u64 = 1200;
pub const PER_EVENT: i32 = 25;
pub const PER_TRADE: i32 = 2;
const DISCARD: i32 = 2;
const TRANSFERRED: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    MajorNegative,
    MinorNegative,
    MinorPositive,
    MajorPositive,
    Trading,
}
impl Kind {
    pub const ALL: [Kind; 5] = [Kind::MajorNegative, Kind::MinorNegative, Kind::MinorPositive, Kind::MajorPositive, Kind::Trading];
    pub fn weight(self) -> i32 {
        match self {
            Kind::MajorNegative => -5,
            Kind::MinorNegative => -1,
            Kind::MinorPositive | Kind::Trading => 1,
            Kind::MajorPositive => 5,
        }
    }
    pub fn max(self) -> i32 {
        match self {
            Kind::MajorNegative => 100,
            Kind::MinorNegative => 200,
            Kind::MinorPositive | Kind::Trading => 25,
            Kind::MajorPositive => 20,
        }
    }
    pub fn decay_per_day(self) -> i32 {
        match self {
            Kind::MajorNegative => 10,
            Kind::MinorNegative => 20,
            Kind::MinorPositive => 1,
            Kind::MajorPositive => 0,
            Kind::Trading => 2,
        }
    }
    pub fn decay_per_transfer(self) -> i32 {
        match self {
            Kind::MinorPositive => 5,
            _ => 20,
        }
    }
}

/// `getReputation`: the weighted sum of the values of one player (one value per kind, in `Kind::ALL` order).
pub fn reputation(values: &[i32; 5]) -> i32 {
    Kind::ALL.iter().map(|k| values[*k as usize] * k.weight()).sum()
}

/// `add`: the stored value of a kind after `amount` is added to `old`: capped at the kind's maximum, and 0 under 2.
pub fn added_value(old: i32, amount: i32, kind: Kind) -> i32 {
    let sum = old + amount;
    let v = if old == 0 { amount } else if sum > kind.max() { kind.max().max(old) } else { sum };
    let v = v.min(kind.max());
    if v < DISCARD { 0 } else { v }
}

/// `decay`: the stored value of a kind after one day; 0 under 2.
pub fn decayed_value(v: i32, kind: Kind) -> i32 {
    if v == 0 {
        return v;
    }
    let next = v - kind.decay_per_day();
    if next < DISCARD { 0 } else { next }
}

/// The value passed on to another villager, or `None` when it falls under 2.
pub fn transferred_value(value: i32, kind: Kind) -> Option<i32> {
    let decayed = value - kind.decay_per_transfer();
    (decayed >= DISCARD).then_some(decayed)
}

/// `selectGossipsForTransfer`: indices into `entries` (kind and value of each gossip) drawn up to 10 times, weighted by size; a
/// repeat is counted once. `draw(n)` gives an integer in `0..n`.
pub fn transfer_picks(entries: &[(Kind, i32)], draw: &mut dyn FnMut(i32) -> i32) -> Vec<usize> {
    let mut ends = Vec::with_capacity(entries.len());
    let mut total = 0;
    for (k, v) in entries {
        total += (v * k.weight()).abs();
        ends.push(total - 1);
    }
    let mut picked: Vec<usize> = Vec::new();
    if total > 0 {
        for _ in 0..TRANSFERRED {
            let index = match ends.binary_search(&draw(total)) {
                Ok(i) | Err(i) => i,
            };
            if !picked.contains(&index) {
                picked.push(index);
            }
        }
    }
    picked
}

/// The gossip day memory lasts until the next day is due, at least one tick.
pub fn day_ttl(since: u64, now: u64) -> u64 {
    (since + DAY).saturating_sub(now).max(1)
}

/// The day has passed since `since`: the values decay.
pub fn day_due(since: u64, now: u64) -> bool {
    now >= since + DAY
}

/// A villager that last heard at `last` may hear again: the wait is over (or it has not heard for a later time).
pub fn free_to_hear(last: u64, now: u64) -> bool {
    now < last || now >= last + HEARD_WAIT
}
