//! The variant of a creature (`Fox.Type`: 0 red, 1 snow), chosen once at the spawn: the spawn group's value, else the biome tag
//! `spawns_snow_foxes` (`finalize.rs:2167-2179`). Kept as a component; the species' conditions read it through `is_snow`.
use super::*;

pub use crate::rules::variant::{SNOW, draw};
use crate::cow::{AttackDamage, SpeciesId, SpeciesTable};
use crate::nbt::{accept, owner, Field};
use crate::rules::variant::Table;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::borrow::Cow;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Variant(pub u8);

/// The variant table of the species of `e` (`Def::variant`); None for a species that draws none.
pub fn table_of(w: &World, e: Entity) -> Option<&'static Table> {
    let id = usize::from(w.get::<SpeciesId>(e)?.0);
    let name = w.get_resource::<SpeciesTable>()?.0.get(id)?.name;
    crate::species::registry().by_name(name).ok()?.variant
}

/// Gives `e` its variant and what the variant changes (`Rabbit.setVariant`): the base armour, the added attack (recomputed from the
/// species' own attack, so another variant takes it away) and the default name of an unnamed creature.
pub fn set_variant(w: &mut World, e: Entity, v: u8) {
    w.entity_mut(e).insert(Variant(v));
    let Some(t) = table_of(w, e) else { return };
    let Some(sp) = w.get::<SpeciesId>(e).and_then(|s| w.resource::<SpeciesTable>().0.get(usize::from(s.0)).copied()) else { return };
    if t.traits.iter().any(|x| x.damage_add != 0.0) {
        let add = t.traits.iter().find(|x| x.variant == v).map_or(0.0, |x| x.damage_add);
        w.entity_mut(e).insert(AttackDamage(sp.attack_damage + add));
    }
    let Some(tr) = t.traits.iter().find(|x| x.variant == v) else { return };
    if let Some(a) = tr.armor {
        // The variant sets the attribute's base: `refresh_armor` keeps it with the pieces and modifiers.
        let mut mods = w.get::<crate::attr_mod::AttrMods>(e).cloned().unwrap_or_default();
        match mods.0.iter_mut().find(|x| x.0 == "armor") {
            Some(x) => x.1 = f64::from(a),
            None => mods.0.push(("armor".to_owned(), f64::from(a), Vec::new())),
        }
        w.entity_mut(e).insert(mods);
        crate::attr_mod::touched(w, e, "armor");
    }
    if let Some(key) = tr.name
        && crate::custom_name::get(w, e).is_none()
    {
        crate::custom_name::set(w, e, Some(format!("{{\"translate\":\"{key}\"}}")));
    }
}

/// `f` with the place of `at` as the draws read it, built once for the spawn: the biome `Spawner` holds, its tags, the structures of
/// the host (`StructureHost`: a piece holding the position) and the moon of the clock's day (`DimensionType.moonPhase`).
pub fn with_place<T>(w: &mut World, mw: &horizon_client_lib::mobs::MobWorld<'_>, at: [f64; 3], f: impl FnOnce(&mut World, &crate::rules::variant::Place) -> T) -> T {
    let biome = w.get_resource::<crate::spawning::Spawner>().map_or_else(String::new, |s| s.biome.clone());
    let has = |tag: &str| mw.data.biome_has(tag, &biome);
    let pos = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
    let host = w.get_resource::<crate::spawn_structures::StructureHost>().cloned();
    let inside = |id: &str| host.as_ref().is_some_and(|h| h.has_piece_at(pos, id));
    let day = w.get_resource::<crate::cow::Clock>().map_or(0, |c| c.game_now()).div_euclid(24_000);
    let moon = horizon_client_lib::mobs::spawning::MOON_BRIGHTNESS[day.rem_euclid(8) as usize];
    f(w, &crate::rules::variant::Place { biome: &biome, biome_has: &has, in_structure: &inside, moon })
}

/// `getBreedOffspring`'s variant: the baby takes the variant of one parent by the table's `Inherit` (`rules::variant::inherited`), a
/// roll that names neither draws afresh. The baby is born where no biome is known to the bridge: the fresh draw has an empty place,
/// so only a table whose draw reads none (`Pick::Uniform`) may ask for it.
pub fn inherit_baby(w: &mut World, baby: Entity, a: Entity, b: Entity) {
    let Some(t) = table_of(w, baby).filter(|t| !matches!(t.inherit, crate::rules::variant::Inherit::None)) else { return };
    let (Some(va), Some(vb)) = (w.get::<Variant>(a).copied(), w.get::<Variant>(b).copied()) else { return };
    let seed = crate::goals::with_rng(w, a, |r| r.next_u32());
    let mut r = horizon_core::random::LegacyRandom::new(i64::from(seed));
    let v = crate::rules::variant::inherited(t, va.0, vb.0, &mut r).unwrap_or_else(|| {
        let place = crate::rules::variant::Place { biome: "", biome_has: &|_| false, in_structure: &|_| false, moon: 0.0 };
        crate::rules::variant::draw_from(t, None, &place, &mut r)
    });
    set_variant(w, baby, v);
}

/// The variant the spawn of `e` draws from the table of its species (`finalizeSpawn`): the group's value, else by the biome's tags.
pub fn draw_at_spawn(w: &mut World, e: Entity, group: Option<u8>, place: &crate::rules::variant::Place, r: &mut impl horizon_core::random::RandomSource) -> Option<u8> {
    let t = table_of(w, e)?;
    let v = crate::rules::variant::draw_from(t, group, place, r);
    set_variant(w, e, v);
    Some(v)
}

/// The sound variant of a creature, by its name (`minecraft:classic` by default, `Pig.defineSynchedData`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SoundVariant(pub String);

/// The default sound variant, every species' (`CowSoundVariants.CLASSIC`, `PigSoundVariants.CLASSIC`...).
const DEFAULT_SOUND: &str = "minecraft:classic";

/// The resolved species of `e`.
fn resolved_of(w: &World, e: Entity) -> Option<&'static crate::species::Resolved> {
    let id = usize::from(w.get::<SpeciesId>(e)?.0);
    let name = w.get_resource::<SpeciesTable>()?.0.get(id)?.name;
    crate::species::registry().by_name(name).ok()
}

/// The sound variants of the species of `e` (`Def::sounds`); None for a species with none.
fn sounds_of(w: &World, e: Entity) -> Option<&'static [&'static str]> {
    resolved_of(w, e)?.sounds
}

/// A bred baby's sound (`Wolf.getBreedOffspring`): a fresh draw from the parent's stream, where the species draws one; the others keep
/// `classic`, the default.
pub fn breed_sound(w: &mut World, a: Entity, baby: Entity) {
    if !resolved_of(w, baby).is_some_and(|x| x.sound_on_breed) {
        return;
    }
    let seed = crate::goals::with_rng(w, a, |r| r.next_u32());
    draw_sound(w, baby, &mut horizon_core::random::LegacyRandom::new(i64::from(seed)));
}

/// The sound variant drawn at the spawn, after the variant (`PigSoundVariants.pickRandomSoundVariant`): uniform over the species' list.
pub fn draw_sound(w: &mut World, e: Entity, r: &mut impl horizon_core::random::RandomSource) {
    let Some(names) = sounds_of(w, e) else { return };
    let i = r.next_int_bound(names.len() as i32).unwrap_or(0) as usize;
    w.entity_mut(e).insert(SoundVariant(names.get(i).copied().unwrap_or(names[0]).to_owned()));
}

/// Written for every creature whose species has sounds, `minecraft:classic` when it has no drawn one (`addAdditionalSaveData`).
fn write_sound(w: &World, e: Entity, c: &mut Compound) {
    if sounds_of(w, e).is_none() {
        return;
    }
    let name = w.get::<SoundVariant>(e).map_or(DEFAULT_SOUND, |s| s.0.as_str());
    c.insert("sound_variant".into(), Tag::String(JavaString::from(name)));
}

/// A saved name read back: a name the species does not list is the first one, `minecraft:classic`.
fn read_sound(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(names), Tag::String(s)) = (sounds_of(w, e), t) else { return false };
    let want = s.to_string_lossy();
    let name = names.iter().find(|n| **n == want).copied().unwrap_or(DEFAULT_SOUND);
    w.entity_mut(e).insert(SoundVariant(name.to_owned()));
    true
}

/// The saved variants: `RabbitType` (an int) and `Type` (a string), each only for the species whose table has that key.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("sound_variant"), write: write_sound, read: read_sound, accepts: accept::string, owner: None },
    Field { key: Cow::Borrowed("RabbitType"), write: write_rabbit_type, read: read_rabbit_type, accepts: accept::int, owner: Some(owner::<Variant>) },
    Field { key: Cow::Borrowed("Type"), write: write_type, read: read_type, accepts: accept::string, owner: Some(owner::<Variant>) },
    Field { key: Cow::Borrowed("variant"), write: write_variant, read: read_variant, accepts: accept::string, owner: Some(owner::<Variant>) },
    Field { key: Cow::Borrowed("Variant"), write: write_variant_int, read: read_variant_int, accepts: accept::int, owner: Some(owner::<Variant>) },
    Field { key: Cow::Borrowed("type"), write: write_salmon_type, read: read_salmon_type, accepts: accept::string, owner: Some(owner::<Variant>) },
    Field { key: Cow::Borrowed("Variant"), write: write_wide, read: read_wide, accepts: accept::int, owner: Some(owner::<super::spawn_draw::Wide>) },
];

fn read_wide(_: &mut World, _: Entity, _: &Tag) -> bool {
    false
}

fn write_variant(w: &World, e: Entity, c: &mut Compound) {
    write_key("variant", w, e, c);
}

fn write_variant_int(w: &World, e: Entity, c: &mut Compound) {
    write_key("Variant", w, e, c);
    // A packed int (`Horse`: colour | markings << 8): the bits above the colour are the `Wide` component.
    if let (Some(wide), Some(Tag::Int(i))) = (w.get::<super::spawn_draw::Wide>(e), c.get(&"Variant".into())) {
        let packed = *i | (wide.0 as i32);
        c.insert("Variant".into(), Tag::Int(packed));
    }
}

/// The saved `Variant` int of a species with no variant table, held whole by `Wide` (`TropicalFish`).
fn write_wide(w: &World, e: Entity, c: &mut Compound) {
    if table_of(w, e).is_none()
        && let Some(wide) = w.get::<super::spawn_draw::Wide>(e)
    {
        c.insert("Variant".into(), Tag::Int(wide.0 as i32));
    }
}

fn read_variant(w: &mut World, e: Entity, t: &Tag) -> bool {
    read_key("variant", w, e, t)
}

fn read_variant_int(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(i) = t else { return read_key("Variant", w, e, t) };
    if table_of(w, e).is_some_and(|x| x.key == "Variant") {
        let handled = read_key("Variant", w, e, &Tag::Int(*i & 0xFF));
        if *i >> 8 != 0 {
            w.entity_mut(e).insert(super::spawn_draw::Wide((*i as u32) & !0xFF));
        }
        return handled;
    }
    if super::spawn_draw::draws_of(w, e).is_some() {
        w.entity_mut(e).insert(super::spawn_draw::Wide(*i as u32));
        return true;
    }
    false
}

fn write_key(key: &str, w: &World, e: Entity, c: &mut Compound) {
    let (Some(v), Some(t)) = (w.get::<Variant>(e), table_of(w, e)) else { return };
    if t.key != key {
        return;
    }
    let tag = match t.names.is_empty() {
        true => Tag::Int(i32::from(v.0)),
        false => Tag::String(JavaString::from(t.names[t.ids.iter().position(|x| *x == v.0).unwrap_or(0)])),
    };
    c.insert(key.into(), tag);
}

fn write_rabbit_type(w: &World, e: Entity, c: &mut Compound) {
    write_key("RabbitType", w, e, c);
}

fn write_type(w: &World, e: Entity, c: &mut Compound) {
    write_key("Type", w, e, c);
}

/// A saved value read back: a value the table does not know is 0 (`Rabbit.Variant.byId`, `Fox.Type.byName`).
fn read_key(key: &str, w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(table) = table_of(w, e).filter(|x| x.key == key) else { return false };
    let id = match t {
        Tag::Int(i) if table.names.is_empty() => u8::try_from(*i).ok().filter(|x| table.ids.contains(x)),
        Tag::String(s) if !table.names.is_empty() => {
            let bare = |n: &str| n.strip_prefix("minecraft:").unwrap_or(n).to_owned();
            let want = bare(&s.to_string_lossy());
            table.names.iter().position(|n| bare(n) == want).map(|i| table.ids[i])
        }
        _ => return false,
    };
    set_variant(w, e, id.unwrap_or(0));
    true
}

fn write_salmon_type(w: &World, e: Entity, c: &mut Compound) {
    write_key("type", w, e, c);
}

fn read_salmon_type(w: &mut World, e: Entity, t: &Tag) -> bool {
    read_key("type", w, e, t)
}

fn read_rabbit_type(w: &mut World, e: Entity, t: &Tag) -> bool {
    read_key("RabbitType", w, e, t)
}

fn read_type(w: &mut World, e: Entity, t: &Tag) -> bool {
    read_key("Type", w, e, t)
}

/// The predicate of the species' conditions (registered in `species/predicates.rs` as `variant:snow`).
pub fn is_snow(w: &World, e: Entity) -> bool {
    w.get::<Variant>(e).is_some_and(|v| v.0 == SNOW)
}
