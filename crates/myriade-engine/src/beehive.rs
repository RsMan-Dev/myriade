//! Beehives and bee nests (`BeehiveBlockEntity`): up to three bees stored as
//! entity tags, let out when their time inside is up (never at night or in
//! the rain, nor through a blocked front, unless the hive is emptied in an
//! emergency), honey for every bee coming home with nectar. The host (the game
//! or the mob AI probe) loads and adds the bees through [`HiveHost`].
pub use crate::chunk::block_entity::{Beehive, Occupant};
use horizon_core::nbt::{Compound, Tag};
use horizon_core::random::{LegacyRandom, RandomSource};

pub const MAX_OCCUPANTS: usize = 3;
/// `BeehiveBlockEntity.IGNORED_BEE_TAGS`: dropped from a bee going in, and
/// again from the tag it comes out with.
const IGNORED_BEE_TAGS: [&str; 25] = [
    "Air",
    "drop_chances",
    "equipment",
    "Brain",
    "CanPickUpLoot",
    "DeathTime",
    "fall_distance",
    "FallFlying",
    "Fire",
    "HurtTime",
    "LeftHanded",
    "Motion",
    "NoGravity",
    "OnGround",
    "PortalCooldown",
    "Pos",
    "Rotation",
    "sleeping_pos",
    "CannotEnterHiveTicks",
    "TicksSincePollination",
    "CropsGrownSincePollination",
    "hive_pos",
    "Passengers",
    "leash",
    "UUID",
];

/// `BeehiveBlockEntity.BeeReleaseStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    HoneyDelivered,
    BeeReleased,
    Emergency,
}

/// What a hive needs from the level it is in.
pub trait HiveHost {
    /// `level.getRandom()`.
    fn random(&mut self) -> &mut LegacyRandom;
    /// `EnvironmentAttributes.BEES_STAY_IN_HIVE` at a position.
    fn bees_stay_in_hive(&self, at: [i32; 3]) -> bool;
    /// A non-empty collision shape at a block.
    fn blocked(&self, at: [i32; 3]) -> bool;
    /// `BlockState.setValue(HONEY_LEVEL, level)` on the hive.
    fn set_honey_level(&mut self, at: [i32; 3], level: i32);
    /// `Occupant.createEntity` + `LevelWriter.addFreshEntity`: makes the entity from the saved tag (it has its `id`, `Pos`, and the
    /// release data), at the hive's exit; `blocked` is the front's collision. The network id, or `None` when the level refuses it.
    fn spawn(&mut self, tag: Tag, at: [i32; 3], facing: [i32; 2], blocked: bool, delivered: bool) -> Option<u32>;
    fn sound(&mut self, name: &'static str, at: [f64; 3]);
    /// `BlockEntity.setChanged`: the hive's saved bees changed.
    fn set_changed(&mut self, at: [i32; 3]);
}

fn strip_ignored(c: &mut Compound) {
    for key in IGNORED_BEE_TAGS {
        c.remove(&key.into());
    }
}
fn int(tag: Option<&Tag>) -> Option<i32> {
    match tag {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    }
}
fn pos(tag: Option<&Tag>) -> Option<[i32; 3]> {
    match tag {
        Some(Tag::IntArray(p)) if p.len() == 3 => Some([p[0], p[1], p[2]]),
        _ => None,
    }
}

/// The bee occupant's codec and constructors, over the data in `chunk::block_entity`.
pub trait OccupantExt: Sized {
    /// `Occupant.of`: the bee's saved tag without the ignored tags, staying
    /// 2400 ticks with nectar, else 600.
    fn of(tag: Tag) -> Self;
    /// `Occupant.create`: a worldgen nest's bee, a bare bee tag.
    fn create(ticks_in_hive: i32) -> Self;
    fn has_nectar(&self) -> bool;
    fn from_nbt(tag: &Tag) -> Option<Self>;
    fn to_nbt(&self) -> Tag;
}
impl OccupantExt for Occupant {
    fn of(mut tag: Tag) -> Self {
        if let Tag::Compound(c) = &mut tag {
            strip_ignored(c);
        }
        let nectar = crate::entity_nbt::flag(&tag, "HasNectar");
        Self {
            entity_data: tag,
            ticks_in_hive: 0,
            min_ticks_in_hive: if nectar { 2400 } else { 600 },
        }
    }
    fn create(ticks_in_hive: i32) -> Self {
        Self {
            entity_data: Tag::compound([("id", Tag::String("minecraft:bee".into()))]),
            ticks_in_hive,
            min_ticks_in_hive: 600,
        }
    }
    fn has_nectar(&self) -> bool {
        crate::entity_nbt::flag(&self.entity_data, "HasNectar")
    }
    fn from_nbt(tag: &Tag) -> Option<Self> {
        let data = tag.get("entity_data")?;
        if !matches!(data, Tag::Compound(c) if c.contains_key(&"id".into())) {
            return None;
        }
        Some(Self {
            entity_data: data.clone(),
            ticks_in_hive: int(tag.get("ticks_in_hive"))?,
            min_ticks_in_hive: int(tag.get("min_ticks_in_hive"))?,
        })
    }
    fn to_nbt(&self) -> Tag {
        Tag::compound([
            ("entity_data", self.entity_data.clone()),
            ("ticks_in_hive", Tag::Int(self.ticks_in_hive)),
            ("min_ticks_in_hive", Tag::Int(self.min_ticks_in_hive)),
        ])
    }
}

/// `BeehiveBlockEntity`'s behaviour, over the data in `chunk::block_entity`.
pub trait BeehiveExt: Sized {
    fn full(&self) -> bool;
    /// `loadAdditional`: `bees` (all or none, as the list codec reads them)
    /// and `flower_pos`.
    fn from_nbt(tag: &Tag) -> Self;
    /// `saveAdditional`.
    fn to_nbt(&self) -> Vec<(&'static str, Tag)>;
    /// `addOccupant` (the bee's own tag, saved by the host): the hive keeps
    /// the bee's flower, or now and then swaps its own for it. The host then
    /// discards the bee.
    fn add_occupant(
        &mut self,
        at: [i32; 3],
        tag: Tag,
        bee_flower: Option<[i32; 3]>,
        host: &mut impl HiveHost,
    ) -> bool;
    /// `serverTick`: the bees whose time is up come out, then the hive
    /// hums now and then while it holds any. `honey` is the hive state's
    /// honey level as the tick starts (`None` without the property);
    /// `facing` its front.
    fn server_tick(
        &mut self,
        at: [i32; 3],
        facing: [i32; 2],
        honey: Option<i32>,
        host: &mut impl HiveHost,
    );
    /// `releaseAllOccupants`: the ids of the bees let out.
    fn release_all(
        &mut self,
        at: [i32; 3],
        facing: [i32; 2],
        honey: Option<i32>,
        status: Release,
        host: &mut impl HiveHost,
    ) -> Vec<u32>;
}
impl BeehiveExt for Beehive {
    fn full(&self) -> bool {
        self.bees.len() == MAX_OCCUPANTS
    }
    fn from_nbt(tag: &Tag) -> Self {
        let bees = match tag.get("bees") {
            Some(Tag::List(list)) => list
                .iter()
                .map(Occupant::from_nbt)
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        Self {
            bees,
            flower: pos(tag.get("flower_pos")),
        }
    }
    fn to_nbt(&self) -> Vec<(&'static str, Tag)> {
        let mut out = vec![(
            "bees",
            Tag::List(self.bees.iter().map(Occupant::to_nbt).collect()),
        )];
        if let Some(f) = self.flower {
            out.push(("flower_pos", Tag::IntArray(f.to_vec())));
        }
        out
    }
    fn add_occupant(
        &mut self,
        at: [i32; 3],
        tag: Tag,
        bee_flower: Option<[i32; 3]>,
        host: &mut impl HiveHost,
    ) -> bool {
        if self.bees.len() >= MAX_OCCUPANTS {
            return false;
        }
        self.bees.push(Occupant::of(tag));
        if let Some(f) = bee_flower
            && (self.flower.is_none() || host.random().next_boolean())
        {
            self.flower = Some(f);
        }
        let corner = at.map(f64::from);
        host.sound("block.beehive.enter", corner);
        host.set_changed(at);
        true
    }
    fn server_tick(
        &mut self,
        at: [i32; 3],
        facing: [i32; 2],
        honey: Option<i32>,
        host: &mut impl HiveHost,
    ) {
        let flower = self.flower;
        let mut released = false;
        let mut i = 0;
        while i < self.bees.len() {
            // BeeData.tick: ticksInHive++ > minTicksInHive.
            let b = &mut self.bees[i];
            let up = b.ticks_in_hive > b.min_ticks_in_hive;
            b.ticks_in_hive += 1;
            if up {
                let status = if b.has_nectar() {
                    Release::HoneyDelivered
                } else {
                    Release::BeeReleased
                };
                let occupant = b.clone();
                if release(host, at, facing, honey, &occupant, status, flower).is_some() {
                    self.bees.remove(i);
                    released = true;
                    continue;
                }
            }
            i += 1;
        }
        // `tickOccupants`: a bee let out is a change to save.
        if released {
            host.set_changed(at);
        }
        if !self.bees.is_empty() && host.random().next_double() < 0.005 {
            let hum = [
                f64::from(at[0]) + 0.5,
                f64::from(at[1]),
                f64::from(at[2]) + 0.5,
            ];
            host.sound("block.beehive.work", hum);
        }
    }
    fn release_all(
        &mut self,
        at: [i32; 3],
        facing: [i32; 2],
        honey: Option<i32>,
        status: Release,
        host: &mut impl HiveHost,
    ) -> Vec<u32> {
        let flower = self.flower;
        let mut out = Vec::new();
        self.bees.retain(
            |b| match release(host, at, facing, honey, b, status, flower) {
                Some(id) => {
                    out.extend(id);
                    false
                }
                None => true,
            },
        );
        // `releaseAllOccupants`: a bee let out is a change to save.
        if !out.is_empty() {
            host.set_changed(at);
        }
        out
    }
}

/// `releaseOccupant`: `None` while the bee stays in, else the id of the bee
/// let out (`Some(None)` when the level refused it). The bee is made again from its saved tag.
fn release(
    host: &mut impl HiveHost,
    at: [i32; 3],
    facing: [i32; 2],
    honey: Option<i32>,
    occupant: &Occupant,
    status: Release,
    flower: Option<[i32; 3]>,
) -> Option<Option<u32>> {
    let emergency = status == Release::Emergency;
    if host.bees_stay_in_hive(at) && !emergency {
        return None;
    }
    let blocked = host.blocked([at[0] + facing[0], at[1], at[2] + facing[1]]);
    if blocked && !emergency {
        return None;
    }
    let mut tag = occupant.entity_data.clone();
    if let Tag::Compound(c) = &mut tag {
        strip_ignored(c);
        let center = [at[0], at[1], at[2]].map(|v| Tag::Double((f64::from(v) + 0.5).to_bits()));
        c.insert("Pos".into(), Tag::List(center.to_vec()));
        release_data(c, at, occupant.ticks_in_hive, flower, status == Release::HoneyDelivered, host);
    }
    let id = host.spawn(tag, at, facing, blocked, status == Release::HoneyDelivered)?;
    host.sound(
        "block.beehive.exit",
        [f64::from(at[0]) + 0.5, f64::from(at[1]) + 0.5, f64::from(at[2]) + 0.5],
    );
    if status == Release::HoneyDelivered
        && let Some(level) = honey.filter(|&l| l < 5)
    {
        let mut increase = if host.random().next_int_bound(100).unwrap_or(1) == 0 { 2 } else { 1 };
        if level + increase > 5 {
            increase -= 1;
        }
        host.set_honey_level(at, level + increase);
    }
    Some(Some(id))
}

/// `Bee.setBeeReleaseData` and the exit's marks, on the saved tag: no gravity, the hive, the time inside counted towards growing up and
/// away from love, the hive's flower when the bee has none (nine times in ten), the nectar given.
fn release_data(
    c: &mut Compound,
    at: [i32; 3],
    ticks: i32,
    flower: Option<[i32; 3]>,
    delivered: bool,
    host: &mut impl HiveHost,
) {
    c.insert("NoGravity".into(), Tag::Byte(1));
    c.insert("hive_pos".into(), Tag::IntArray(at.to_vec()));
    let locked = c.get(&"AgeLocked".into()).is_some_and(|t| matches!(t, Tag::Byte(v) if *v != 0));
    if !locked {
        let age = int(c.get(&"Age".into())).unwrap_or(0);
        let aged = if age < 0 { (age + ticks).min(0) } else if age > 0 { (age - ticks).max(0) } else { age };
        c.insert("Age".into(), Tag::Int(aged));
    }
    let love = int(c.get(&"InLove".into())).unwrap_or(0);
    c.insert("InLove".into(), Tag::Int((love - ticks).max(0)));
    if let Some(f) = flower
        && pos(c.get(&"flower_pos".into())).is_none()
        && host.random().next_float() < 0.9
    {
        c.insert("flower_pos".into(), Tag::IntArray(f.to_vec()));
    }
    if delivered {
        c.insert("HasNectar".into(), Tag::Byte(0));
    }
}

/// `CampfireBlock.isSmokeyPos`: a lit campfire up to five blocks below,
/// unless a block whose collision crosses the smoke's centre post comes
/// first (a lit campfire right under that block still counts).
pub fn smokey(w: &crate::mobs::MobWorld, at: [i32; 3]) -> bool {
    let lit = |y: i32| {
        w.has_tag("minecraft:campfires", w.block_name(at[0], y, at[2]))
            && w.prop(at[0], y, at[2], "lit") == Some("true")
    };
    for i in 1..=5 {
        let y = at[1] - i;
        if lit(y) {
            return true;
        }
        // SHAPE_VIRTUAL_POST: Block.column(4, 0, 16), the middle quarter.
        let blocked = w.collision_boxes(at[0], y, at[2]).iter().any(|b| {
            let (min, max) = (b.min_position(), b.max_position());
            min.x < 0.625 && max.x > 0.375 && min.z < 0.625 && max.z > 0.375
        });
        if blocked {
            return lit(y - 1);
        }
    }
    false
}

/// `HorizontalDirectionalBlock.FACING` as an x/z step.
pub fn facing_step(facing: Option<&str>) -> [i32; 2] {
    match facing {
        Some("south") => [0, 1],
        Some("west") => [-1, 0],
        Some("east") => [1, 0],
        _ => [0, -1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bee_with_nectar_as_any_number_stays_longer() {
        let bee = Occupant::of(Tag::compound([("HasNectar", Tag::Int(1))]));
        assert!(bee.has_nectar());
        assert_eq!(bee.min_ticks_in_hive, 2400);
    }

    #[test]
    fn nbt_round_trips_in_vanilla_order() {
        let tag = Tag::compound([
            (
                "bees",
                Tag::List(vec![
                    Occupant::create(37).to_nbt(),
                    Occupant::create(0).to_nbt(),
                ]),
            ),
            ("flower_pos", Tag::IntArray(vec![1, -60, 3])),
        ]);
        let hive = Beehive::from_nbt(&tag);
        assert_eq!(hive.bees.len(), 2);
        assert_eq!(hive.bees[0].ticks_in_hive, 37);
        assert_eq!(hive.flower, Some([1, -60, 3]));
        assert_eq!(Tag::compound(hive.to_nbt()), tag);
    }

    #[test]
    fn a_bad_bee_drops_the_whole_list() {
        let tag = Tag::compound([(
            "bees",
            Tag::List(vec![
                Occupant::create(1).to_nbt(),
                Tag::compound([("ticks_in_hive", Tag::Int(3))]),
            ]),
        )]);
        assert!(Beehive::from_nbt(&tag).bees.is_empty());
    }

    /// Records the tag a release hands to the level.
    struct Rec {
        random: LegacyRandom,
        spawned: Vec<(Tag, bool)>,
        honey: Vec<i32>,
    }
    impl HiveHost for Rec {
        fn random(&mut self) -> &mut LegacyRandom {
            &mut self.random
        }
        fn bees_stay_in_hive(&self, _: [i32; 3]) -> bool {
            false
        }
        fn blocked(&self, _: [i32; 3]) -> bool {
            false
        }
        fn set_honey_level(&mut self, _: [i32; 3], level: i32) {
            self.honey.push(level);
        }
        fn spawn(&mut self, tag: Tag, _: [i32; 3], _: [i32; 2], _: bool, delivered: bool) -> Option<u32> {
            self.spawned.push((tag, delivered));
            Some(1)
        }
        fn sound(&mut self, _: &'static str, _: [f64; 3]) {}
        fn set_changed(&mut self, _: [i32; 3]) {}
    }

    /// `Bee.setBeeReleaseData` on the saved tag: the 700 ticks inside age a baby and shorten love, the hive and its flower are written,
    /// the nectar is given and the honey level rises.
    #[test]
    fn a_released_bee_comes_back_from_its_saved_tag_aged_and_unladen() {
        let mut hive = Beehive::default();
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:bee".into())),
            ("Age", Tag::Int(-3000)),
            ("InLove", Tag::Int(1000)),
            ("HasNectar", Tag::Byte(1)),
        ]);
        let mut o = Occupant::of(tag);
        o.ticks_in_hive = 2401;
        hive.bees.push(o);
        hive.flower = Some([1, 2, 3]);
        let mut host = Rec { random: LegacyRandom::new(5), spawned: Vec::new(), honey: Vec::new() };
        hive.server_tick([8, 64, 8], [0, -1], Some(1), &mut host);
        assert!(hive.bees.is_empty());
        let (tag, delivered) = &host.spawned[0];
        assert!(delivered);
        assert_eq!(tag.get("Age"), Some(&Tag::Int(-599)));
        assert_eq!(tag.get("InLove"), Some(&Tag::Int(0)));
        assert_eq!(tag.get("HasNectar"), Some(&Tag::Byte(0)));
        assert_eq!(tag.get("hive_pos"), Some(&Tag::IntArray(vec![8, 64, 8])));
        assert!(host.honey[0] > 1, "the honey level rose");
    }

    #[test]
    fn going_in_strips_the_ignored_tags() {
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:bee".into())),
            ("UUID", Tag::IntArray(vec![1, 2, 3, 4])),
            ("Health", Tag::Float(10.0_f32.to_bits())),
            ("HasNectar", Tag::Byte(1)),
            ("hive_pos", Tag::IntArray(vec![0, 0, 0])),
        ]);
        let o = Occupant::of(tag);
        assert_eq!(o.min_ticks_in_hive, 2400);
        assert!(o.entity_data.get("UUID").is_none());
        assert!(o.entity_data.get("hive_pos").is_none());
        assert!(o.entity_data.get("Health").is_some());
    }
}
