use horizon_core::random::RandomSource;

/// `Ageable::babies`: the chance of a baby in a group of animals.
pub const BABY_CHANCE: f32 = 0.05;
/// `AgeableMob`'s baby age (-24000) as ticks to grow up.
pub const BABY_TICKS: u64 = 24_000;
/// The deviation of the triangle of the follow-range bonus (`Mob.finalizeSpawn`, `AddMultipliedBase`).
pub const FOLLOW_DEVIATION: f64 = 0.114_850_000_000_000_01;
/// A baby that never grows up (a zombie's): the sentinel of the wheel, as for an egg that is not laid.
pub const NEVER: u64 = u64::MAX;

/// The group a cluster of spawns shares (`AgeableMobGroupData`, `Zombie`'s group).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Group {
    /// `variant`: the value the first member drew (`FoxGroupData`, `HorseGroupData`, `LlamaGroupData`), taken by the next ones.
    Ageable { should_baby: bool, size: u32, chance: f32, variant: Option<u8> },
    /// An ageable group of the species whose variant it shares (`Shape`): the same draws as `Ageable`.
    Shaped { shape: Shape, should_baby: bool, size: u32, chance: f32, variant: Option<u8> },
    /// `tried_camel`: a husk's try to ride a camel husk (`HuskGroupData`); None for the other zombies.
    Zombie { baby: bool, jockey: bool, tried_camel: Option<bool> },
    /// An ageable group that also carries the values its first member drew (`AxolotlGroupData`, `TropicalFishGroupData`): `n` of the
    /// `vals` are set; `size` counts the members already made.
    Pack { vals: [u32; 2], n: u8, size: u32, should_baby: bool, chance: f32 },
    /// `SpiderEffectsGroupData`: the index of the effect in `species::spider::EFFECTS`, None for no effect.
    Spider { effect: Option<u8> },
}
impl Group {
    /// `new AgeableMobGroupData(true)`: a fresh group, the first mob never a baby (its size is 0).
    pub const AGEABLE: Self = Self::Ageable { should_baby: true, size: 0, chance: BABY_CHANCE, variant: None };
}

/// The species whose group carries a variant (the `Shaped` group's shape): each one's group data class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Fox,
    Horse,
    Llama,
    Rabbit,
    Wolf,
}

/// The chicken draws of a baby zombie: an existing free vehicle first, else a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jockey {
    None,
    Free,
    Create,
}

/// The draws of `Mob.finalizeSpawn` that every class makes: the follow-range bonus and the left hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MobRoll {
    pub follow_bonus: f64,
    pub left_handed: bool,
}
pub fn mob_roll(r: &mut impl RandomSource) -> MobRoll {
    let follow_bonus = r.triangle(0.0, FOLLOW_DEVIATION);
    MobRoll { follow_bonus, left_handed: r.next_float() < 0.05 }
}

/// `AgeableMob.finalizeSpawn`: whether this mob is a baby, and the group after it (its size grows by one).
pub fn ageable_roll(group: Option<Group>, r: &mut impl RandomSource) -> (bool, Group) {
    let g = group.unwrap_or(Group::AGEABLE);
    if let Group::Pack { vals, n, size, should_baby, chance } = g {
        let baby = should_baby && size > 0 && r.next_float() <= chance;
        return (baby, Group::Pack { vals, n, size: size + 1, should_baby, chance });
    }
    match g {
        Group::Ageable { should_baby, size, chance, variant } => {
            let baby = should_baby && size > 0 && r.next_float() <= chance;
            (baby, Group::Ageable { should_baby, size: size + 1, chance, variant })
        }
        Group::Shaped { shape, should_baby, size, chance, variant } => {
            let baby = should_baby && size > 0 && r.next_float() <= chance;
            (baby, Group::Shaped { shape, should_baby, size: size + 1, chance, variant })
        }
        _ => (false, g),
    }
}

/// `Zombie.finalizeSpawn`: a new group draws its baby (5 %) and takes a jockey; the chicken draws follow when it is a baby.
pub fn zombie_roll(group: Option<Group>, r: &mut impl RandomSource) -> (Group, Jockey) {
    let g = group.unwrap_or_else(|| Group::Zombie { baby: r.next_float() < 0.05, jockey: true, tried_camel: None });
    let Group::Zombie { baby, jockey, .. } = g else { return (g, Jockey::None) };
    if !baby || !jockey {
        return (g, Jockey::None);
    }
    if f64::from(r.next_float()) < 0.05 {
        (g, Jockey::Free)
    } else if f64::from(r.next_float()) < 0.05 {
        (g, Jockey::Create)
    } else {
        (g, Jockey::None)
    }
}

/// A condition of a variant's `spawn_conditions` (`VariantRegistry`): the biome list (`#tag` for a tag), the structures, the moon.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    Biome(Vec<String>),
    Structure(Vec<String>),
    Moon(Option<f64>, Option<f64>),
}

/// One entry's condition at a priority; `None`: always true.
#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    pub entry: usize,
    pub cond: Option<Cond>,
    pub priority: i32,
}

/// What the spawn's place is: its biome, the biome tags it has (without `#`), the structures with a piece there, the moon's brightness.
pub struct Here<'a> {
    pub biome: &'a str,
    pub biome_tags: &'a [&'a str],
    pub structures: &'a [String],
    pub moon: f64,
}

/// `VariantUtils.selectVariantToSpawn`: the entries of the highest priority whose condition holds, one of them drawn (the
/// priorities are read from the highest, as `VariantRegistry::select` does).
pub fn select_variant(sel: &[Selector], here: &Here, r: &mut impl RandomSource) -> Option<usize> {
    let mut order: Vec<&Selector> = sel.iter().collect();
    order.sort_by_key(|s| std::cmp::Reverse(s.priority));
    let mut highest = i32::MIN;
    let mut picked = Vec::new();
    for s in order {
        if s.priority < highest {
            continue;
        }
        let ok = match &s.cond {
            None => true,
            Some(Cond::Biome(list)) => list.iter().any(|b| match b.strip_prefix('#') {
                Some(tag) => here.biome_tags.contains(&tag),
                None => here.biome == b,
            }),
            Some(Cond::Structure(list)) => here.structures.iter().any(|x| list.contains(x)),
            Some(Cond::Moon(min, max)) => min.is_none_or(|m| here.moon >= m) && max.is_none_or(|m| here.moon <= m),
        };
        if ok {
            highest = s.priority;
            picked.push(s.entry);
        }
    }
    if picked.is_empty() {
        return None;
    }
    Some(picked[r.next_int_bound(picked.len() as i32).unwrap_or(0) as usize])
}
