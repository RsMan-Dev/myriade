//! The extra draws of a species' `finalizeSpawn` (`Def::draws`): marks, packed variants, genes, wool colour, villager type, cube size. Each draw
//! is a `Roll` (`rules::spawn_draw`) whose value a `Store` puts in the component that holds it; a draw with a `Shared` kind also reads and
//! makes the values of the spawn group (`Group::Pack`). Nothing here knows a species: the configuration says what is drawn, in what order,
//! before or after the class's own draws (`spawn_finalize::finalize`).
use super::*;
use crate::behaviours::panda_state::Gene;
use crate::cow::{Body, Health, MaxHealth, SpeciesId, SpeciesTable};
use crate::rules::spawn_draw::{roll, Ctx, Roll};
use crate::rules::spawn_finalize::{Group, BABY_CHANCE};
use horizon_core::random::RandomSource;

/// The part of a saved `Variant` int the `Variant` component (one byte) does not hold: for a species with a variant table the bits above
/// the first byte (`Horse`: the markings), else the whole value (`TropicalFish`: pattern, colours).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wide(pub u32);

/// The genes of a panda (`Panda.mainGene`, `hiddenGene`); the gene it shows is the component `Gene`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Genes {
    pub main: Gene,
    pub hidden: Gene,
}

/// A creature whose `Shared` draw made no group: a loner (`TropicalFish.isSchool` false), read by the spawner.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Loner;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Before the class's own `finalizeSpawn` draws (the species draws first, then calls `super`).
    Before,
    /// After them.
    After,
}

/// Where a drawn value goes.
#[derive(Clone, Copy, Debug)]
pub enum Store {
    Wide,
    /// The `Variant` component (`behaviours::variant::set_variant`: what the variant changes comes with it).
    Variant,
    GeneMain,
    GeneHidden,
    WoolColor,
    MerchantVariant,
    Size,
    /// A write the action's module owns (a marker, a memory's date): the value goes to its function.
    Call(fn(&mut World, Entity, u32)),
}

/// How a member takes its value from the group's.
#[derive(Clone, Copy, Debug)]
pub enum Member {
    /// The first value, no draw.
    First,
    /// `nextInt(n)` over the values set.
    OneOf,
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Plain(Roll),
    /// A value shared by a group: a member of a group that has values takes one (`member`); the first member makes them (`start`, one roll
    /// each, in order), after a `nextFloat() < odds` when `odds` is given (else it draws `free` and makes no group). `baby_from`: a baby from
    /// the member with that many before it (`Axolotl`: 2), `should_baby` the group's own babies after.
    Shared { start: &'static [Roll], odds: Option<f32>, member: Member, free: &'static Roll, baby_from: Option<u32>, should_baby: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Draw {
    pub when: When,
    pub kind: Kind,
    pub store: Store,
}

/// The draws of a species and the group a spawn starts with when none is given (`new AgeableMobGroupData(0.2f)`, `(false)`).
#[derive(Clone, Copy, Debug)]
pub struct Draws {
    pub group: Option<Group>,
    pub list: &'static [Draw],
}

/// `AbstractCubeMob.setSpawnSize` after `AgeableMob.finalizeSpawn` (the group of a fresh spawn makes no baby): `1 << nextInt(3)`, one more
/// power when it is under 2 and `nextFloat() < 0.5 * special` (slime, magma cube).
pub static SPAWN_SIZE: Draws = Draws {
    group: Some(Group::Ageable { should_baby: false, size: 0, chance: BABY_CHANCE, variant: None }),
    list: &[Draw {
        when: When::After,
        kind: Kind::Plain(Roll::Pow2(&Roll::Grow { bound: 3, below: 2, chance: 0.5 })),
        store: Store::Size,
    }],
};

/// `SulfurCube.finalizeSpawn`: size 1 for a baby, 2 for an adult, no draw (after the age is set).
pub static SPAWN_SIZE_BY_AGE: Draws = Draws {
    group: None,
    list: &[Draw { when: When::After, kind: Kind::Plain(Roll::ByAge { baby: 1, adult: 2 }), store: Store::Size }],
};

const TYPE_DESERT: Roll = Roll::Const(1);
const TYPE_JUNGLE: Roll = Roll::Const(2);
const TYPE_SAVANNA: Roll = Roll::Const(3);
const TYPE_SNOW: Roll = Roll::Const(4);
const TYPE_SWAMP: Roll = Roll::Const(5);
const TYPE_TAIGA: Roll = Roll::Const(6);

/// `VillagerType.byBiome` (plains 0, desert 1, jungle 2, savanna 3, snow 4, swamp 5, taiga 6; any other biome is plains): the type a villager or
/// a zombie villager gets at its spawn, no draw. It is also the type of a baby villager born in such a biome (`Roll` through `type_at`).
pub static VILLAGER_TYPE: Draws = Draws { group: None, list: &[Draw { when: When::Before, kind: Kind::Plain(VILLAGER_BY_BIOME), store: Store::MerchantVariant }] };

pub const VILLAGER_BY_BIOME: Roll = Roll::ByBiome {
    rows: &[
        ("badlands", &TYPE_DESERT),
        ("desert", &TYPE_DESERT),
        ("eroded_badlands", &TYPE_DESERT),
        ("wooded_badlands", &TYPE_DESERT),
        ("bamboo_jungle", &TYPE_JUNGLE),
        ("jungle", &TYPE_JUNGLE),
        ("sparse_jungle", &TYPE_JUNGLE),
        ("savanna_plateau", &TYPE_SAVANNA),
        ("savanna", &TYPE_SAVANNA),
        ("windswept_savanna", &TYPE_SAVANNA),
        ("deep_frozen_ocean", &TYPE_SNOW),
        ("frozen_ocean", &TYPE_SNOW),
        ("frozen_river", &TYPE_SNOW),
        ("ice_spikes", &TYPE_SNOW),
        ("snowy_beach", &TYPE_SNOW),
        ("snowy_taiga", &TYPE_SNOW),
        ("snowy_plains", &TYPE_SNOW),
        ("grove", &TYPE_SNOW),
        ("snowy_slopes", &TYPE_SNOW),
        ("frozen_peaks", &TYPE_SNOW),
        ("jagged_peaks", &TYPE_SNOW),
        ("swamp", &TYPE_SWAMP),
        ("mangrove_swamp", &TYPE_SWAMP),
        ("old_growth_spruce_taiga", &TYPE_TAIGA),
        ("old_growth_pine_taiga", &TYPE_TAIGA),
        ("windswept_gravelly_hills", &TYPE_TAIGA),
        ("windswept_hills", &TYPE_TAIGA),
        ("taiga", &TYPE_TAIGA),
        ("windswept_forest", &TYPE_TAIGA),
    ],
    otherwise: &Roll::Const(0),
};

/// The villager type of a biome, by the same table (a baby born there takes it; none of the parents' draw is made here).
pub fn type_at(biome: &str, biome_has: &dyn Fn(&str) -> bool, r: &mut impl RandomSource) -> u8 {
    roll(&VILLAGER_BY_BIOME, &Ctx { biome, biome_has, special: 0.0, baby: true }, r) as u8
}

/// The draws of the species of `e` (`Def::draws`).
pub fn draws_of(w: &World, e: Entity) -> Option<&'static Draws> {
    let id = usize::from(w.get::<SpeciesId>(e)?.0);
    let name = w.get_resource::<SpeciesTable>()?.0.get(id)?.name;
    crate::species::registry().by_name(name).ok()?.draws
}

/// Runs the draws of `when` for `e`, in order, on `r`; the group is read and, for a shared draw, replaced.
pub fn run(w: &mut World, e: Entity, d: &Draws, when: When, group: &mut Option<Group>, c: &Ctx, r: &mut impl RandomSource) {
    for x in d.list.iter().filter(|x| x.when == when) {
        let v = match x.kind {
            Kind::Plain(t) => roll(&t, c, r),
            Kind::Shared { start, odds, member, free, baby_from, should_baby } => {
                let take = |vals: [u32; 2], n: u8, r: &mut dyn FnMut(i32) -> i32| match member {
                    Member::First => vals[0],
                    Member::OneOf => vals[r(i32::from(n.max(1))).clamp(0, 1) as usize],
                };
                let mut draw = |n: i32| r.next_int_bound(n).unwrap_or(0);
                match *group {
                    Some(Group::Pack { vals, n, size, .. }) if n > 0 => {
                        if baby_from.is_some_and(|k| size >= k) {
                            crate::spawn_finalize::set_baby(w, e, crate::spawn_finalize::BABY_TICKS);
                        }
                        take(vals, n, &mut draw)
                    }
                    _ => {
                        let make = match odds {
                            Some(p) => r.next_float() < p,
                            None => true,
                        };
                        if make {
                            let mut vals = [0u32; 2];
                            for (i, s) in start.iter().take(2).enumerate() {
                                vals[i] = roll(s, c, r);
                            }
                            let n = start.len().min(2) as u8;
                            *group = Some(Group::Pack { vals, n, size: 0, should_baby, chance: BABY_CHANCE });
                            let mut draw = |n: i32| r.next_int_bound(n).unwrap_or(0);
                            take(vals, n, &mut draw)
                        } else {
                            // `TropicalFish.isSchool = false`: a draw past the odds is a loner, the school ends at it.
                            w.entity_mut(e).insert(Loner);
                            roll(free, c, r)
                        }
                    }
                }
            }
        };
        store(w, e, x.store, v);
    }
}

fn store(w: &mut World, e: Entity, s: Store, v: u32) {
    match s {
        Store::Wide => {
            w.entity_mut(e).insert(Wide(v));
        }
        Store::Variant => super::variant::set_variant(w, e, v as u8),
        Store::GeneMain | Store::GeneHidden => {
            let mut g = w.get::<Genes>(e).copied().unwrap_or_default();
            match s {
                Store::GeneMain => g.main = gene_of(v),
                _ => g.hidden = gene_of(v),
            }
            w.entity_mut(e).insert((g, shown(g)));
            if matches!(s, Store::GeneHidden) {
                set_attributes(w, e);
            }
        }
        Store::WoolColor => match w.get_mut::<crate::farm::Wool>(e) {
            Some(mut x) => x.color = v as u8,
            None => {
                w.entity_mut(e).insert(crate::farm::Wool { sheared: false, color: v as u8 });
            }
        },
        Store::MerchantVariant => {
            if let Some(mut m) = w.get_mut::<super::trade_offers::Merchant>(e) {
                m.variant = v as u8;
            }
        }
        Store::Size => crate::size::set_size(w, e, v as i32),
        Store::Call(f) => f(w, e, v),
    }
}

/// `Panda.Gene` by id: normal, lazy, worried, playful, brown, weak, aggressive.
pub fn gene_of(id: u32) -> Gene {
    match id {
        1 => Gene::Lazy,
        2 => Gene::Worried,
        3 => Gene::Playful,
        4 => Gene::Brown,
        5 => Gene::Weak,
        6 => Gene::Aggressive,
        _ => Gene::Normal,
    }
}

/// `Panda.Gene.getVariantFromGenes`: a recessive main gene shows only when the hidden one is the same, else the normal one.
pub fn shown(g: Genes) -> Gene {
    match g.main {
        Gene::Brown | Gene::Weak if g.main != g.hidden => Gene::Normal,
        m => m,
    }
}

/// `Panda.setAttributes`: the weak one has 10 health (the current health follows a lower maximum), the lazy one walks at 0.07.
pub fn set_attributes(w: &mut World, e: Entity) {
    match w.get::<Gene>(e).copied() {
        Some(Gene::Weak) => {
            w.entity_mut(e).insert(MaxHealth(10.0));
            if let Some(mut h) = w.get_mut::<Health>(e) {
                h.0 = h.0.min(10.0);
            }
        }
        Some(Gene::Lazy) => {
            if let Some(mut b) = w.get_mut::<Body>(e) {
                b.speed = 0.070_000_000_298_023_22;
            }
        }
        _ => {}
    }
}
