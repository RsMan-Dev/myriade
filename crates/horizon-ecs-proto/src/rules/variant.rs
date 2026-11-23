//! The variant's choice at the spawn: the snow kind, the default red kind.

/// The variant of the snow kind (`Fox.Type.SNOW`).
pub const SNOW: u8 = 1;

/// The id of `Rabbit.Variant.EVIL` (the killer bunny; its saved `RabbitType` is 99 in the jar).
pub const RABBIT_EVIL: u8 = 99;

/// The spawn's choice: the group's variant when it gives one, else snow in a biome with the tag.
pub fn draw(group: Option<u8>, biome_has_snow_tag: bool) -> u8 {
    group.unwrap_or(if biome_has_snow_tag { SNOW } else { 0 })
}

/// A line of a species' variant table: in a biome with `tag` (None: any biome) the draw gives the value of the first `cuts` entry
/// whose bound it is under, else `last`.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub tag: Option<&'static str>,
    pub cuts: &'static [(i32, u8)],
    pub last: u8,
}

/// How a species' variant is drawn at its spawn (`getRandomRabbitVariant`, `Fox.finalizeSpawn`): the first `rows` whose biome tag the
/// place has, else `otherwise`. With `bound` above 0 one `nextInt(bound)` is drawn first, before any tag test, as the jar does.
#[derive(Clone, Copy, Debug)]
pub struct Table {
    pub bound: i32,
    pub rows: &'static [Row],
    pub otherwise: Row,
    /// The saved key (`RabbitType`, `Type`) and the values it may hold (any other reads as 0).
    pub key: &'static str,
    /// How the value is drawn (`bound`, `rows` and `otherwise` serve `Pick::Rows` only).
    pub pick: Pick,
    /// Drawn even when the spawn group already gives a value (`Rabbit.finalizeSpawn` draws first, the group overrides); false: the group
    /// value is taken without a draw (`Horse`, `Llama`).
    pub redraw: bool,
    pub ids: &'static [u8],
    /// Saved as the string at the value's index in `ids` (`Fox.Type`: red, snow) when not empty, else as an int.
    pub names: &'static [&'static str],
    /// The serialized names of the data component `minecraft:<species>/variant` (`getSerializedName`), at the `ids` position; not saved.
    pub component_names: &'static [&'static str],
    pub traits: &'static [Trait],
    /// A spawn into a group that already holds this many members is a baby, with no roll (`Fox.finalizeSpawn`: group size 2 or more).
    pub group_baby: Option<u32>,
    /// How a baby's variant follows its parents (`getBreedOffspring`).
    pub inherit: Inherit,
}
/// The component name (`component_names`) of the value `id` of a table.
pub fn component_name(t: &Table, id: u8) -> Option<&'static str> {
    let i = t.ids.iter().position(|x| *x == id)?;
    t.component_names.get(i).copied()
}
impl Table {
    /// No draw, nothing saved, no inheritance: the base of a literal (`..Table::EMPTY`).
    pub const EMPTY: Self = Self {
        bound: 0,
        rows: &[],
        otherwise: Row { tag: None, cuts: &[], last: 0 },
        key: "",
        pick: Pick::Rows,
        redraw: false,
        ids: &[],
        names: &[],
        component_names: &[],
        traits: &[],
        group_baby: None,
        inherit: Inherit::None,
    };
}

/// How a baby's variant follows its parents: one `nextInt(total)`; under `own` the first parent's value, under `own + other` the
/// second's, else a fresh draw of the table (`Cat`, `Wolf`: 2, 1, 1, `Horse`: 9, 4, 4).
#[derive(Clone, Copy, Debug)]
pub enum Inherit {
    None,
    Blend { total: i32, own: i32, other: i32 },
}

/// The baby's variant from its parents' (`a` the mother, `b` the other parent), or None when the table does not inherit or the roll
/// asks for a fresh draw.
pub fn inherited(t: &Table, a: u8, b: u8, r: &mut impl horizon_core::random::RandomSource) -> Option<u8> {
    let Inherit::Blend { total, own, other } = t.inherit else { return None };
    let roll = r.next_int_bound(total).unwrap_or(0);
    if roll < own {
        Some(a)
    } else if roll < own + other {
        Some(b)
    } else {
        None
    }
}

/// The way a species' variant is drawn at its spawn.
#[derive(Clone, Copy, Debug)]
pub enum Pick {
    /// `bound`, `rows` and `otherwise` of the table (rabbit, fox).
    Rows,
    /// `VariantUtils.selectVariantToSpawn`: the entries of the highest priority whose condition holds, one drawn (cat, wolf, frog, pig,
    /// cow, chicken, zombie nautilus). The entries are in the registry's order.
    Registry(&'static [Choice]),
    /// `Util.getRandom(values)`: one `nextInt(n)` over the first `n` ids (parrot, llama, horse colour).
    Uniform(i32),
    /// `WeightedList.getRandom`: one `nextInt(total)` walked over the weights (salmon).
    Weighted(&'static [(u8, i32)]),
}

/// A condition of a registry entry (`spawn_conditions`): the biome list (`#tag` for a tag), the structures (`#tag` too), the moon.
#[derive(Clone, Copy, Debug)]
pub enum Need {
    Biomes(&'static [&'static str]),
    Structures(&'static [&'static str]),
    MoonAtLeast(f32),
}

/// One `spawn_conditions` entry of a registry variant: its value, its priority and its condition (None: always true).
#[derive(Clone, Copy, Debug)]
pub struct Choice {
    pub id: u8,
    pub priority: i32,
    pub need: Option<Need>,
}

/// Where the spawn is, as the draw reads it.
pub struct Place<'a> {
    pub biome: &'a str,
    pub biome_has: &'a dyn Fn(&str) -> bool,
    pub in_structure: &'a dyn Fn(&str) -> bool,
    pub moon: f32,
}

/// What a variant changes in the creature (`Rabbit.setVariant(EVIL)`): its base armour, the added attack (a transient modifier of the
/// jar: not saved, gone with another variant) and its default name (a translation key, only when it has no custom name).
#[derive(Clone, Copy, Debug)]
pub struct Trait {
    pub variant: u8,
    pub armor: Option<f32>,
    pub damage_add: f32,
    pub name: Option<&'static str>,
}

/// The variant a spawn gets: the spawn group's value when it has one (it draws nothing), else the table's row for the biome's tags.
pub fn draw_from(t: &Table, group: Option<u8>, place: &Place, r: &mut impl horizon_core::random::RandomSource) -> u8 {
    if let (Some(g), false) = (group, t.redraw) {
        return g;
    }
    let v = match t.pick {
        Pick::Rows => {
            let roll = if t.bound > 0 { r.next_int_bound(t.bound).unwrap_or(0) } else { 0 };
            let row = t.rows.iter().find(|x| x.tag.is_some_and(place.biome_has)).unwrap_or(&t.otherwise);
            row.cuts.iter().find(|c| roll < c.0).map_or(row.last, |c| c.1)
        }
        Pick::Registry(list) => select(list, place, r),
        Pick::Uniform(n) => t.ids.get(r.next_int_bound(n).unwrap_or(0) as usize).copied().unwrap_or(0),
        Pick::Weighted(w) => {
            let mut roll = r.next_int_bound(w.iter().map(|x| x.1).sum()).unwrap_or(0);
            let mut out = w.last().map_or(0, |x| x.0);
            for (id, weight) in w {
                if roll < *weight {
                    out = *id;
                    break;
                }
                roll -= weight;
            }
            out
        }
    };
    group.unwrap_or(v)
}

/// `VariantUtils.selectVariantToSpawn`: among the entries whose condition holds, those of the highest priority; one drawn by
/// `nextInt(count)` (no entry holds: the first, the registry's default).
pub fn select(list: &[Choice], place: &Place, r: &mut impl horizon_core::random::RandomSource) -> u8 {
    let holds = |c: &&Choice| match c.need {
        None => true,
        Some(Need::Biomes(b)) => b.iter().any(|x| match x.strip_prefix('#') {
            Some(tag) => (place.biome_has)(tag),
            None => *x == place.biome,
        }),
        Some(Need::Structures(s)) => s.iter().any(|x| (place.in_structure)(x.trim_start_matches('#'))),
        Some(Need::MoonAtLeast(m)) => place.moon >= m,
    };
    let top = list.iter().filter(holds).map(|c| c.priority).max();
    let picked: Vec<u8> = list.iter().filter(holds).filter(|c| Some(c.priority) == top).map(|c| c.id).collect();
    match picked.len() {
        0 => list.first().map_or(0, |c| c.id),
        n => picked[r.next_int_bound(n as i32).unwrap_or(0) as usize],
    }
}
