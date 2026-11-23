//! ClientboundUpdateRecipesPacket: what RecipeManager.finalizeRecipeLoading
//! synchronizes, the recipe property sets (the items each furnace, smithing
//! table slot and brewing stand slot accepts) and the stonecutter's recipes,
//! read from the data pack's recipe files.
use super::data::SyncData;
use horizon_protocol::buf::Writer;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

/// RecipeManager.RECIPE_PROPERTY_SETS: each set and the ingredients it takes
/// from a recipe of the given type.
pub(crate) const PROPERTY_SETS: &[(&str, &str, &[&str])] = &[
    ("smithing_addition", "smithing", &["addition"]),
    ("smithing_base", "smithing", &["base"]),
    ("smithing_template", "smithing", &["template"]),
    ("furnace_input", "minecraft:smelting", &["ingredient"]),
    ("blast_furnace_input", "minecraft:blasting", &["ingredient"]),
    ("smoker_input", "minecraft:smoking", &["ingredient"]),
    (
        "campfire_input",
        "minecraft:campfire_cooking",
        &["ingredient"],
    ),
    ("brewing_input", "minecraft:brewing", &["input"]),
    ("brewing_reagent", "minecraft:brewing", &["reagent"]),
];

/// An ingredient as its HolderSet: a tag, or the items listed.
enum Ingredient {
    Tag(String),
    Items(Vec<String>),
}

fn ingredient(v: &Value) -> Option<Ingredient> {
    match v {
        Value::String(s) => Some(match s.strip_prefix('#') {
            Some(tag) => Ingredient::Tag(tag.to_owned()),
            None => Ingredient::Items(vec![s.clone()]),
        }),
        Value::Array(a) => Some(Ingredient::Items(
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect(),
        )),
        // A brewing slot: the item predicate's `item` ingredient.
        Value::Object(o) => ingredient(o.get("item")?),
        _ => None,
    }
}

/// The recipes by id in vanilla's registration order (path, then namespace),
/// the order `RecipeManager` numbers groups and lists the stonecutter's
/// recipes in, and so the order the host's own tables keep.
fn registration_order<V>(all: &BTreeMap<String, V>) -> Vec<&V> {
    let mut ordered: Vec<_> = all.iter().collect();
    ordered.sort_by_cached_key(|(id, _)| myriade_engine::crafting::registration_key(id));
    ordered.into_iter().map(|(_, v)| v).collect()
}

/// The network copy of the already-resolved load cohort. Only recipes accepted
/// by the canonical loader participate in display numbering, keeping client
/// display ids aligned with the server's recipe index. Accepted ingredients
/// become flat holder sets, including empty sets, without interpreting custom metadata.
pub(super) fn network_recipes(
    resources: &Path,
    overlay: &horizon_world::loot::PackOverlay,
    defaults: &horizon_world::item_defaults::ItemDefaults,
    sources: &myriade_engine::crafting::ResolvedSources,
    damage: myriade_data::registries::RegistryIds,
) -> crate::Result<(BTreeMap<String, Box<str>>, HashMap<String, i32>)> {
    // The resolved copy: the load's `ingredient` answers are in place, and
    // none of them leaves it (`forget_answers` below).
    let items = myriade_engine::items::Items::catalog_with_damage_registry(
        resources, overlay, defaults, damage,
    )?;
    let accepted =
        myriade_engine::crafting::Recipes::from_resolved(resources, &items, overlay, sources)?;
    let accepted: std::collections::HashSet<&str> = accepted
        .by_type
        .values()
        .flat_map(|recipes| {
            recipes
                .iter()
                .filter(|recipe| recipe.has_placement_candidates())
                .map(|recipe| recipe.id.as_str())
        })
        .collect();
    let mut all: BTreeMap<_, _> = sources
        .recipes
        .iter()
        .filter_map(|recipe| {
            let id = myriade_engine::crafting::qualified(&recipe.name);
            accepted
                .contains(id.as_str())
                .then(|| (id, (*recipe.resolved).clone()))
        })
        .collect();
    all.retain(|_, recipe| normalize_ingredients(recipe, &items));
    all.values_mut().for_each(|r| {
        myriade_engine::crafting::Ingredient::forget_answers(r);
    });
    // RecipeManager.unpackRecipeInfo's group numbers, while the files are
    // still parsed.
    let mut groups = HashMap::new();
    for r in registration_order(&all) {
        if let Some(g) = r["group"].as_str().filter(|g| !g.is_empty()) {
            let next = groups.len() as i32;
            groups.entry(g.to_owned()).or_insert(next);
        }
    }
    // Each file as its compact JSON text: a book entry parses the one it
    // needs, and the parsed files would keep about 5 MB alive for the
    // server's whole run.
    let all = all
        .into_iter()
        .map(|(id, r)| (id, r.to_string().into_boxed_str()))
        .collect();
    Ok((all, groups))
}

/// An ingredient object's flat item list: the shape NeoForge's standard
/// kinds resolve to, or a brewing predicate's `item`. `None`: the object is
/// a kind the loader does not know, a mod's own.
fn flat(ing: &Value, items: &myriade_engine::items::Items) -> Option<Value> {
    if !ing.is_object() {
        return None;
    }
    let value = if ing
        .get("type")
        .or_else(|| ing.get("neoforge:ingredient_type"))
        .is_some()
    {
        ing
    } else {
        // Only an untyped brewing wrapper gives `item` a structural meaning.
        ing.get("item").unwrap_or(ing)
    };
    let parsed = myriade_engine::crafting::Ingredient::parse(value, items)?;
    Some(Value::Array(
        parsed
            .order
            .iter()
            .map(|i| Value::String(i.clone()))
            .collect(),
    ))
}

/// Replace each of a recipe's ingredient objects with its flat item list,
/// so `holder_set` writes the items instead of an empty list. `false`: the
/// recipe keeps an object the loader cannot parse.
fn normalize_ingredients(r: &mut Value, items: &myriade_engine::items::Items) -> bool {
    let known = r["type"]
        .as_str()
        .is_some_and(|t| t.starts_with("minecraft:"));
    if !known {
        return true;
    }
    let mut parsed = true;
    // Both ingredient maps use the canonical load's role list.
    for key in crate::crafting::INGREDIENT_MAPS {
        let Some(map) = r.get_mut(key).and_then(Value::as_object_mut) else {
            continue;
        };
        for ing in map.values_mut() {
            if let Some(replacement) = flat(ing, items) {
                *ing = replacement;
            } else if myriade_engine::crafting::Ingredient::parse(ing, items).is_none() {
                parsed = false;
            }
        }
    }
    // The list and single-ingredient keys, the smithing trio, the brewing
    // pair.
    for key in crate::crafting::INGREDIENT_KEYS {
        let Some(ing) = r.get_mut(key) else { continue };
        if let Some(replacement) = flat(ing, items) {
            *ing = replacement;
            continue;
        }
        if key == "ingredients" {
            let Some(entries) = ing.as_array_mut() else {
                parsed = false;
                continue;
            };
            for entry in entries {
                if let Some(replacement) = flat(entry, items) {
                    *entry = replacement;
                } else if myriade_engine::crafting::Ingredient::parse(entry, items).is_none() {
                    parsed = false;
                }
            }
        } else {
            let value = ing.get("item").unwrap_or(ing);
            parsed &= myriade_engine::crafting::Ingredient::parse(value, items).is_some();
        }
    }
    parsed
}

/// The packet's payload for the network copy's recipes (`network_recipes`).
pub fn update_recipes(
    all: &BTreeMap<String, Box<str>>,
    sync: &SyncData,
    additions: &myriade_engine::crafting::PropertyAdditions,
) -> Vec<u8> {
    // The copy is already filtered and swapped (`network_recipes`); the
    // filters never run twice, so one swap cannot feed a later one again.
    // Parsing every recipe takes about a millisecond.
    let chosen: Vec<Value> = registration_order(all)
        .into_iter()
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .collect();
    let tags: BTreeMap<&str, &[i32]> = sync
        .tags
        .registries
        .iter()
        .filter(|r| r.registry == "minecraft:item")
        .flat_map(|r| r.tags.iter())
        .map(|t| (t.name.trim_start_matches("minecraft:"), t.ids.as_slice()))
        .collect();
    // An item the registries do not know is left out, not sent as id 0.
    let item = |name: &str| sync.id("minecraft:item", name);
    let items = |i: &Ingredient| -> Vec<i32> {
        match i {
            Ingredient::Tag(t) => tags
                .get(t.trim_start_matches("minecraft:"))
                .map(|ids| ids.to_vec())
                .unwrap_or_default(),
            Ingredient::Items(list) => list.iter().filter_map(|n| item(n)).collect(),
        }
    };
    // A recipe with any required empty ingredient contributes to no property
    // set and never appears in the selectable stonecutter list.
    let placeable = |recipe: &Value| {
        [
            "ingredient",
            "template",
            "base",
            "addition",
            "input",
            "reagent",
        ]
        .into_iter()
        .filter_map(|key| recipe.get(key))
        .all(|value| ingredient(value).is_some_and(|i| !items(&i).is_empty()))
    };
    let mut w = Writer::new();
    let custom: BTreeMap<_, _> = additions.custom().collect();
    w.varint((PROPERTY_SETS.len() + custom.len()) as i32);
    for (key, kind, fields) in PROPERTY_SETS {
        let mut set = BTreeSet::new();
        for recipe in &chosen {
            let t = recipe["type"].as_str().unwrap_or("");
            let matches = if *kind == "smithing" {
                t.starts_with("minecraft:smithing_")
            } else {
                t == *kind
            };
            if matches && placeable(recipe) {
                for field in *fields {
                    if let Some(i) = ingredient(&recipe[*field]) {
                        set.extend(items(&i));
                    }
                }
            }
        }
        // An item the registries do not know is left out (`item`).
        set.extend(
            additions
                .get(key)
                .into_iter()
                .flatten()
                .filter_map(|n| item(n)),
        );
        w.string(&format!("minecraft:{key}"));
        w.varint(set.len() as i32);
        for id in set {
            w.varint(id);
        }
    }
    for (key, names) in custom {
        let ids: BTreeSet<_> = names.iter().filter_map(|name| item(name)).collect();
        w.string(key);
        w.varint(ids.len() as i32);
        for id in ids {
            w.varint(id);
        }
    }
    // SelectableRecipe.SingleInputSet: each stonecutting recipe's input and
    // its result as an ItemStackSlotDisplay.
    let stonecutting: Vec<&Value> = chosen
        .iter()
        .filter(|r| r["type"] == "minecraft:stonecutting" && placeable(r))
        .collect();
    let item_stack = sync
        .id("minecraft:slot_display", "minecraft:item_stack")
        .unwrap_or(0);
    w.varint(stonecutting.len() as i32);
    for r in stonecutting {
        match ingredient(&r["ingredient"]) {
            Some(Ingredient::Tag(t)) => {
                w.varint(0);
                w.string(&t);
            }
            Some(i) => {
                let ids = items(&i);
                w.varint(ids.len() as i32 + 1);
                for id in ids {
                    w.varint(id);
                }
            }
            None => w.varint(1),
        }
        w.varint(item_stack);
        w.varint(
            item(horizon_world::loot::recipe_result_id(&r["result"]).unwrap_or("minecraft:air"))
                .unwrap_or(0),
        );
        w.varint(r["result"]["count"].as_i64().unwrap_or(1) as i32);
        w.varint(0); // no components added
        w.varint(0); // nor removed
    }
    w.data
}

/// The Horizon channel a server sends its clients the recipes its native mods
/// asked for (`native_datapack_sync`), NeoForge's
/// `OnDatapackSyncEvent.sendRecipes`. Horizon clients only.
pub const CONTENT_CHANNEL: &str = "horizon:recipe_content";
/// `ClientboundCustomPayloadPacket`'s cap on a payload, less room for the
/// envelope.
const CONTENT_MAX: usize = 1_048_576 - 1024;

/// One payload of the recipe content: some recipes as `(id, json)`, and on the
/// last one the recipe types they replace on the client (a type the server has
/// no recipe of still empties the client's).
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContentPart {
    pub recipes: Vec<(String, Value)>,
    pub last: Option<Vec<String>>,
}
impl ContentPart {
    /// Decode a content part only within the same bound used by the sender.
    pub fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() > CONTENT_MAX {
            return Err(format!("recipe content exceeds {CONTENT_MAX} bytes"));
        }
        serde_json::from_slice(data).map_err(|error| error.to_string())
    }
}

/// The payloads for `types`' recipes among `sources` (`recipe_sources::sources`,
/// vanilla's ids bare), ids made whole (`minecraft:<name>`). With no type
/// asked for: nothing, or with `reset` (this server sent recipes before) one
/// last part with no types, which puts its clients back on their own. A
/// recipe too big for a payload of its own is left out.
pub fn encode_content(types: &[String], sources: &[(String, Value)], reset: bool) -> Vec<Vec<u8>> {
    // The last part carries the types: they count against its cap too.
    let types_len = serde_json::to_string(types).map_or(0, |t| t.len()) + 32;
    if types_len > CONTENT_MAX {
        log::warn!("Horizon: too many recipe types to send to clients");
        return Vec::new();
    }
    if types.is_empty() && !reset {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut part: Vec<(String, Value)> = Vec::new();
    let mut size = 0;
    for (name, v) in sources {
        if !crate::crafting::mod_kinds::file_has_type(v, types) {
            continue;
        }
        let id = if name.contains(':') {
            name.clone()
        } else {
            format!("minecraft:{name}")
        };
        let len = id.len() + v.to_string().len() + 8;
        if len > CONTENT_MAX {
            log::warn!("Horizon: recipe {id} is too big to send to clients");
            continue;
        }
        if size + len > CONTENT_MAX {
            let recipes = std::mem::take(&mut part);
            out.push(
                serde_json::to_vec(&ContentPart {
                    recipes,
                    last: None,
                })
                .unwrap_or_default(),
            );
            size = 0;
        }
        size += len;
        part.push((id, v.clone()));
    }
    if size + types_len > CONTENT_MAX {
        let recipes = std::mem::take(&mut part);
        out.push(
            serde_json::to_vec(&ContentPart {
                recipes,
                last: None,
            })
            .unwrap_or_default(),
        );
    }
    let last = ContentPart {
        recipes: part,
        last: Some(types.to_vec()),
    };
    out.push(serde_json::to_vec(&last).unwrap_or_default());
    out
}

/// One connection's selected recipe types and encoded response. Dropped on disconnect.
#[derive(Default)]
pub struct PlayerRecipeSync {
    pub types: Vec<String>,
    pub payloads: Vec<Vec<u8>>,
    sent: bool,
}

impl PlayerRecipeSync {
    pub fn replace(&mut self, horizon: bool, mut types: Vec<String>, sources: &[(String, Value)]) {
        types.sort();
        types.dedup();
        self.payloads = if horizon {
            encode_content(&types, sources, self.sent)
        } else {
            Vec::new()
        };
        self.sent |= !self.payloads.is_empty();
        self.types = types;
    }
}

/// Horizon recipe viewer transfer channel.
pub const TRANSFER_CHANNEL: &str = "horizon:recipe_transfer";

/// Decode a transfer request: container, bounded recipe id, optional all flag.
pub fn decode_transfer_request(data: &[u8]) -> horizon_protocol::Result<(i32, String, bool)> {
    let mut reader = horizon_protocol::buf::Reader::new(data);
    let container = reader.varint()?;
    let id = reader.string(512)?;
    let all = match reader.remaining() {
        0 => false,
        1 => reader.u8()? == 1,
        _ => {
            return Err(horizon_protocol::Error::new(
                "recipe transfer request: trailing bytes",
            ));
        }
    };
    Ok((container, id, all))
}

/// Decode a fixed-shape transfer result, or an empty feature advertisement.
pub fn decode_transfer_reply(data: &[u8]) -> horizon_protocol::Result<Option<(i32, u8)>> {
    if data.is_empty() {
        return Ok(None);
    }
    let mut reader = horizon_protocol::buf::Reader::new(data);
    let reply = (reader.varint()?, reader.u8()?);
    if reader.remaining() != 0 {
        return Err(horizon_protocol::Error::new(
            "recipe transfer reply: trailing bytes",
        ));
    }
    Ok(Some(reply))
}

/// Horizon's check of a viewer transfer, worked out without moving anything
/// (`Menu::check_transfer`); like `TRANSFER_CHANNEL`, an empty clientbound
/// payload advertises it. A request is the menu id, a tag the client picks and
/// the recipe's id; the reply is the menu id, the tag, the result as
/// `TRANSFER_CHANNEL` sends it (`result_byte`) and, for `Missing`, the count
/// of the recipe's cells nothing fills then each cell as a byte.
pub const CHECK_CHANNEL: &str = "horizon:recipe_check";

/// Whether `channel` is one of the recipe viewer's (`TRANSFER_CHANNEL`,
/// `CHECK_CHANNEL`).
pub fn viewer_channel(channel: &str) -> bool {
    channel == TRANSFER_CHANNEL || channel == CHECK_CHANNEL
}

/// Tells a Horizon client the server answers both viewer channels: an empty
/// payload on each.
pub fn advertise_viewer_channels(g: &super::guest::Guest) {
    for channel in [TRANSFER_CHANNEL, CHECK_CHANNEL] {
        g.send(
            &horizon_protocol::packets::play::Clientbound::CustomPayload(
                horizon_protocol::packets::configuration::CustomPayload {
                    channel: channel.into(),
                    data: Vec::new(),
                },
            ),
        );
    }
}

/// A viewer transfer's result as the engine's menu gives it.
pub fn transfer_result(
    result: Result<(), crate::menu::place::TransferError>,
) -> myriade_types::RecipeTransferResult {
    use crate::menu::place::TransferError;
    use myriade_types::RecipeTransferResult as R;
    match result {
        Ok(()) => R::Moves,
        Err(TransferError::Missing) => R::Missing,
        Err(TransferError::NoRoom) => R::NoRoom,
        Err(TransferError::Unavailable) => R::NotIntoMenu,
    }
}

/// A `menu_transfer` part's answer as the engine's transfer result (`transfer_result` back).
pub fn transfer_outcome(
    result: myriade_types::RecipeTransferResult,
) -> Result<(), crate::menu::place::TransferError> {
    use crate::menu::place::TransferError;
    use myriade_types::RecipeTransferResult as R;
    match result {
        R::Moves => Ok(()),
        R::Missing => Err(TransferError::Missing),
        R::NoRoom => Err(TransferError::NoRoom),
        R::NotIntoMenu => Err(TransferError::Unavailable),
    }
}

/// The byte both transfer channels carry a result as.
pub fn result_byte(result: myriade_types::RecipeTransferResult) -> u8 {
    use myriade_types::RecipeTransferResult as R;
    match result {
        R::Moves => 0,
        R::Missing => 1,
        R::NoRoom => 2,
        R::NotIntoMenu => 3,
    }
}

/// The result a transfer channel's byte names; None for any other byte.
pub fn byte_result(byte: u8) -> Option<myriade_types::RecipeTransferResult> {
    use myriade_types::RecipeTransferResult as R;
    Some(match byte {
        0 => R::Moves,
        1 => R::Missing,
        2 => R::NoRoom,
        3 => R::NotIntoMenu,
        _ => return None,
    })
}

/// The recipe content the native mods asked for (stored on this guest),
/// to one Horizon client: none when no mod asked.
pub fn send_content(g: &super::guest::Guest) {
    if g.brand != "horizon" {
        return;
    }
    for data in &g.recipe_sync.payloads {
        g.send(
            &horizon_protocol::packets::play::Clientbound::CustomPayload(
                horizon_protocol::packets::configuration::CustomPayload {
                    channel: CONTENT_CHANNEL.into(),
                    data: data.clone(),
                },
            ),
        );
    }
}

#[cfg(test)]
mod content_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn built_in_ingredient_network_lists_use_membership_not_display() {
        let mut recipe = json!({
            "type": "minecraft:crafting_shapeless",
            "ingredients": [
                {"type":"neoforge:components","items":"minecraft:diamond_sword","components":{"minecraft:damage":5}},
                {"type":"neoforge:custom_display","base":"minecraft:apple","display":{"type":"minecraft:item","item":"minecraft:diamond"}}
            ],
            "result": {"id":"minecraft:emerald"}
        });
        assert!(normalize_ingredients(
            &mut recipe,
            &myriade_engine::items::Items::empty()
        ));
        assert_eq!(
            recipe["ingredients"],
            json!([["minecraft:diamond_sword"], ["minecraft:apple"]])
        );
    }

    #[test]
    fn content_decode_bounds_before_deserializing() {
        let mut body = br#"{"recipes":[],"last":null}"#.to_vec();
        body.resize(1_047_552, b' ');
        assert_eq!(ContentPart::decode(&body).unwrap().recipes, vec![]);
        body.push(b' ');
        assert!(ContentPart::decode(&body).unwrap_err().contains("exceeds"));
    }

}

#[cfg(test)]
#[test]
fn empty_ingredient_network_consumers_exclude_whole_unplaceable_recipes() {
    use horizon_protocol::{
        buf::{Codec, Reader},
        packets::recipes::UpdateRecipes,
        registry::Ids,
    };
    use serde_json::json;
    let mut catalog = myriade_engine::items::Items::for_tests(Default::default());
    catalog
        .item_tag_order
        .insert("fixture:empty".into(), vec![]);
    let mut sync = SyncData::empty();
    sync.builtin.registries.insert(
        "minecraft:item".into(),
        Ids::from_names(vec![
            "minecraft:air".into(),
            "minecraft:stone".into(),
            "minecraft:stick".into(),
        ]),
    );
    for (source, retained) in [
        (json!([]), false),
        (json!("#fixture:missing"), false),
        (json!("#fixture:empty"), true),
        (
            json!({"type":"fixture:predicate", "item":"minecraft:stone", "horizon:resolved_items":[]}),
            true,
        ),
    ] {
        let mut all = BTreeMap::new();
        for kind in [
            "minecraft:stonecutting",
            "minecraft:smelting",
            "minecraft:smithing_transform",
        ] {
            let mut recipe = json!({"type":kind, "ingredient":source,
                "base":"minecraft:stone", "addition":source, "result":{"id":"minecraft:stick"}});
            assert_eq!(normalize_ingredients(&mut recipe, &catalog), retained);
            if retained {
                // This copy is only a network HolderSet. The canonical recipe
                // map retains predicates; metadata must not become candidates.
                if source.is_object() {
                    assert_eq!(recipe["ingredient"], json!([]));
                }
                myriade_engine::crafting::Ingredient::forget_answers(&mut recipe);
                all.insert(kind.into(), recipe.to_string().into_boxed_str());
            }
        }
        let bytes = update_recipes(&all, &sync, &Default::default());
        let packet = UpdateRecipes::read(&mut Reader::new(&bytes)).unwrap();
        assert!(packet.stonecutter.is_empty());
        assert!(packet.properties.iter().all(|(_, ids)| ids.is_empty()));
    }
}
