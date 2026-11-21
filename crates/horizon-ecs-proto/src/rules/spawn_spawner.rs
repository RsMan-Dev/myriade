use horizon_core::random::{LegacyRandom, RandomSource};

/// `MobSpawner.spawnDelay`: a draw in `[min, max)`, or `min` when the range is empty.
pub fn spawn_delay(min: i32, max: i32, rng: &mut LegacyRandom) -> i32 {
    if max <= min {
        min
    } else {
        min + rng.next_int_bound(max - min).unwrap_or(0)
    }
}

/// `MobSpawner.spawnPosition`: a position in the box of `range` around the block `pos`, with a height of one block either way.
pub fn attempt_position(pos: [i32; 3], range: i32, rng: &mut LegacyRandom) -> [f64; 3] {
    let r = f64::from(range);
    [
        f64::from(pos[0]) + (rng.next_double() - rng.next_double()) * r + 0.5,
        f64::from(pos[1] + rng.next_int_bound(3).unwrap_or(0) - 1),
        f64::from(pos[2]) + (rng.next_double() - rng.next_double()) * r + 0.5,
    ]
}

/// `WeightedRandom.getRandomItem`: one draw of `[0, total)` walked down the weights.
pub fn weighted(potentials: &[(u8, i32)], rng: &mut LegacyRandom) -> Option<u8> {
    let total: i32 = potentials.iter().map(|p| p.1).sum();
    if total <= 0 {
        return potentials.first().map(|p| p.0);
    }
    let mut r = rng.next_int_bound(total).ok()?;
    for &(id, wt) in potentials {
        if r < wt {
            return Some(id);
        }
        r -= wt;
    }
    None
}

/// `WeightedList.getRandom` over plain weights: one draw of `[0, total)` walked down them; the index drawn, none when the total is 0.
pub fn weighted_index(weights: &[i32], rng: &mut LegacyRandom) -> Option<usize> {
    let total: i32 = weights.iter().sum();
    if total <= 0 {
        return None;
    }
    let mut r = rng.next_int_bound(total).ok()?;
    for (i, &wt) in weights.iter().enumerate() {
        if r < wt {
            return Some(i);
        }
        r -= wt;
    }
    None
}
