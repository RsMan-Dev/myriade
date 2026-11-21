//! The draws of a species' `finalizeSpawn` as data: a `Roll` is a small tree of the random calls the jar makes (`nextInt`, `Util.getRandom`,
//! a weighted list, a chance, a choice by biome), evaluated in the order of the source. What a value means is for the sink that stores it.
use horizon_core::random::RandomSource;

/// Where the spawn is, as a draw reads it: the biome, whether it has a tag (without `#`), the difficulty's special multiplier, and whether
/// the mob is a baby at that point.
pub struct Ctx<'a> {
    pub biome: &'a str,
    pub biome_has: &'a dyn Fn(&str) -> bool,
    pub special: f32,
    pub baby: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Roll {
    /// No draw.
    Const(u32),
    /// `nextInt(bound)`.
    Int(i32),
    /// `Util.getRandom(list)`: one `nextInt(len)`.
    Pick(&'static [u32]),
    /// `nextInt(bound)`, the value of the first cut it is under, else `last`.
    Cuts { bound: i32, cuts: &'static [(i32, u32)], last: u32 },
    /// One `nextInt(total)` walked over the weights; past the list, `rest` (which draws again).
    Weighted { total: i32, list: &'static [(u32, i32)], rest: &'static Roll },
    /// `nextInt(bound)`; below `below`, one `nextFloat() < chance * special` adds one (`AbstractCubeMob.setSpawnSize`).
    Grow { bound: i32, below: i32, chance: f32 },
    /// Two to the power of the inner roll.
    Pow2(&'static Roll),
    /// `nextFloat() < chance` takes `then`, else `otherwise` (no draw for the branch not taken).
    Chance { chance: f32, then: &'static Roll, otherwise: &'static Roll },
    /// No draw: the row whose key the biome matches (`#tag` for a tag, else the biome id without `minecraft:`), else `otherwise`.
    ByBiome { rows: &'static [(&'static str, &'static Roll)], otherwise: &'static Roll },
    /// No draw: `baby` for a baby, `adult` for an adult.
    ByAge { baby: u32, adult: u32 },
    /// The parts drawn in order, each shifted: `sum(part << shift)`.
    Packed(&'static [(Roll, u32)]),
}

pub fn roll(t: &Roll, c: &Ctx, r: &mut impl RandomSource) -> u32 {
    match *t {
        Roll::Const(v) => v,
        Roll::Int(n) => r.next_int_bound(n).unwrap_or(0) as u32,
        Roll::Pick(l) => l.get(r.next_int_bound(l.len() as i32).unwrap_or(0) as usize).copied().unwrap_or(0),
        Roll::Cuts { bound, cuts, last } => {
            let v = r.next_int_bound(bound).unwrap_or(0);
            cuts.iter().find(|x| v < x.0).map_or(last, |x| x.1)
        }
        Roll::Weighted { total, list, rest } => {
            let mut v = r.next_int_bound(total).unwrap_or(0);
            for (id, w) in list {
                if v < *w {
                    return *id;
                }
                v -= w;
            }
            roll(rest, c, r)
        }
        Roll::Grow { bound, below, chance } => {
            let mut v = r.next_int_bound(bound).unwrap_or(0);
            if v < below && r.next_float() < chance * c.special {
                v += 1;
            }
            v as u32
        }
        Roll::Pow2(i) => 1u32 << roll(i, c, r).min(31),
        Roll::Chance { chance, then, otherwise } => {
            if r.next_float() < chance {
                roll(then, c, r)
            } else {
                roll(otherwise, c, r)
            }
        }
        Roll::ByBiome { rows, otherwise } => {
            let own = c.biome.strip_prefix("minecraft:").unwrap_or(c.biome);
            let row = rows.iter().find(|(k, _)| match k.strip_prefix('#') {
                Some(tag) => (c.biome_has)(tag),
                None => *k == own,
            });
            roll(row.map_or(otherwise, |x| x.1), c, r)
        }
        Roll::ByAge { baby, adult } => if c.baby { baby } else { adult },
        Roll::Packed(parts) => parts.iter().fold(0, |acc, (p, shift)| acc | (roll(p, c, r) << shift)),
    }
}
