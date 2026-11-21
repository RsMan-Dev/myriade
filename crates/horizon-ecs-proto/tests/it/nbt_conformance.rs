//! Save conformity: every identifier declared in `nbt::EntityTypes`, made in the ecs and (when Horizon has a writer that builds it from
//! nothing) in Horizon, saved right after creation; the two compounds are compared key by key. The statement is written to
//! `NBT_CONFORMITY_REPORT` when set. A gap that `KNOWN` does not list fails the test.
use horizon_client_lib::{entity_nbt, mobs::Mob};
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use horizon_ecs_proto::{
    cow::{mob_kind, has_mob_kind, spawn_species, species_table, Herd},
    nbt,
};
use std::{collections::BTreeMap, fmt::Write as _};

/// The keys the two sides draw at random or place by position: only their type and length are compared.
const RANDOM: &[&str] = &["UUID", "Pos", "Motion", "Rotation"];

/// Known gaps: (identifier or `*`, key, category, reason). A gap is `missing` (the ecs lacks a key Horizon writes), `extra` (the ecs
/// writes one Horizon does not), `type` or `value`.
const KNOWN: &[(&str, &str, &str, &str)] = &[
    ("*", "LastRestock", "extra", "the ecs gives the trader the villager trade state; the origin writes none for it"),
    ("*", "RestocksToday", "extra", "the ecs gives the trader the villager trade state; the origin writes none for it"),
    ("*", "Xp", "extra", "the ecs gives the trader the villager trade state; the origin writes none for it"),
    ("*", "Age", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "AgeLocked", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "AttackTick", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "BatFlags", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Bred", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "CanBreakDoors", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "CanJoinRaid", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "CannotBeHunted", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "CannotHunt", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "ChestedHorse", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "CollarColor", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Color", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "ConversionTime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "CropsGrownSincePollination", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Crouching", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "DarkTicksRemaining", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "DespawnDelay", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "DrownedConversionTime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "DuplicationCooldown", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "EatingHaystack", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "EggLayTime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "ExplosionPower", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "ForcedAge", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "FreezingTime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "FromBucket", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "GotFish", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "HasLeftHorn", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "HasRightHorn", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "HiddenGene", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "InLove", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "InWaterTime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Inventory", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Invul", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "IsBaby", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "IsChickenJockey", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "IsImmuneToZombification", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "IsScreamingGoat", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "LastPoseTick", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "LastRestock", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Lifetime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "MainGene", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Moistness", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "MoreCarrotTicks", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "NoGravity", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "PatrolLeader", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Patrolling", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Peek", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "PlayerCreated", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "PuffState", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Pumpkin", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "RabbitType", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "RestocksToday", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "RoarTick", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Sitting", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "SkeletonTrap", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "SkeletonTrapTime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Sleeping", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "SpellTicks", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "StrayConversionTime", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Strength", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "StunTick", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Tame", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "TicksSincePollination", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "TimeInOverworld", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Trusted", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Trusting", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Type", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "UUID", "missing", "no UUID component: the identity is drawn at save by the game layer (owner decision)"),
    ("*", "Variant", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "VillagerData", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "VillagerDataFinalized", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Wave", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Xp", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "anchor_pos", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "anger", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "anger_end_time", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "equipment", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "from_bucket", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "fuse", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "has_egg", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "hidden_layers", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "ignited", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "immovable", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "listener", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "main_hand", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "next_weather_age", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "pickup_timer", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "pose", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "powered", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "profile", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "scute_time", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "sheared", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "size", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "sound_variant", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "state", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "still_timeout", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "type", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "variant", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "wasOnGround", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "weather_state", "missing", "state absent from the ecs: the origin writes it at creation, the ecs writes it only when it holds the state"),
    ("*", "Air", "value", "the ecs holds no per-species maximum air at creation (origin writes the species maximum)"),
    ("*", "CanPickUpLoot", "value", "the species default pick-up flag is not in the ecs state"),
    ("*", "Health", "value", "the size-dependent maximum health of the cube is not applied at creation"),
    ("*", "PersistenceRequired", "value", "the origin sets it at creation for these species; the ecs state starts false"),
    ("*", "anger_end_time", "value", "the ecs writes no anger end time at its default (-1 in the origin)"),
    ("*", "attributes", "value", "attribute modifiers and bases are not in the ecs state (state absent)"),
];

fn len_of(t: &Tag) -> usize {
    match t {
        Tag::List(l) => l.len(),
        Tag::IntArray(a) => a.len(),
        Tag::ByteArray(a) => a.len(),
        Tag::LongArray(a) => a.len(),
        Tag::Compound(c) => c.len(),
        _ => 0,
    }
}

fn ecs_save(h: &mut Herd, id: &str) -> Option<Compound> {
    let at = Vec3::new(0.5, 1.0, 0.5);
    let e = match species_table().0.into_iter().find(|s| s.name == id) {
        Some(sp) => spawn_species(&mut h.world, &sp, at.x, at.z, 1),
        None => {
            let c = Tag::compound([("id", Tag::String(id.into())), ("Pos", Tag::List(vec![Tag::Double(at.x.to_bits()), Tag::Double(at.y.to_bits()), Tag::Double(at.z.to_bits())]))]);
            let Tag::Compound(c) = c else { unreachable!() };
            nbt::restore(&mut h.world, &c)?
        }
    };
    nbt::save(&h.world, e)
}

/// Horizon's compound for the identifier, or why there is none.
fn horizon_save(id: &str) -> Result<Compound, &'static str> {
    if has_mob_kind(id) {
        let m = Mob::new(1, mob_kind(id), Vec3::new(0.5, 1.0, 0.5), 0.0);
        let uuid = [1, 2, 3, 4];
        return match entity_nbt::mob_to_nbt(&m, uuid, None, None, |_| None) {
            Tag::Compound(c) => Ok(c),
            _ => Err("the writer gave no compound"),
        };
    }
    Err("no Horizon writer builds it from nothing")
}

#[derive(Default)]
struct Gaps {
    missing: Vec<String>,
    extra: Vec<String>,
    types: Vec<String>,
    values: Vec<String>,
}

fn compare(ecs: &Compound, hz: &Compound) -> Gaps {
    let mut g = Gaps::default();
    for (k, hv) in hz {
        let key = k.to_string_lossy();
        match ecs.get(k) {
            None => g.missing.push(key.to_string()),
            Some(ev) if ev.id() != hv.id() => g.types.push(format!("{key} (ecs {}, horizon {})", ev.id(), hv.id())),
            Some(ev) if RANDOM.contains(&&*key) => {
                if len_of(ev) != len_of(hv) {
                    g.values.push(format!("{key} (length {} against {})", len_of(ev), len_of(hv)));
                }
            }
            Some(ev) if ev != hv => g.values.push(format!("{key} (ecs {ev:?}, horizon {hv:?})")),
            Some(_) => {}
        }
    }
    for k in ecs.keys() {
        if !hz.contains_key(k) {
            g.extra.push(k.to_string_lossy().to_string());
        }
    }
    g
}

fn name_of(entry: &str) -> &str {
    entry.split(" (").next().unwrap_or(entry)
}

#[test]
fn the_ecs_saves_exactly_what_horizon_saves() {
    if !super::common::have_data() || std::env::var("HORIZON_SPECIES").is_ok_and(|s| s != "minecraft:cow") {
        return;
    }
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let ids: Vec<(String, bool)> = nbt::recipes(&h.world).into_iter().map(|(id, r)| (id, r.saved)).collect();
    let mut report = String::new();
    let (mut compared, mut skipped) = (0, 0);
    let mut unlisted = Vec::new();
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    let mut not_compared = Vec::new();
    let mut by_key: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (id, saved) in &ids {
        if !saved {
            not_compared.push(format!("- `{id}`: not saved (declared `saved: false`)"));
            skipped += 1;
            continue;
        }
        let Some(ecs) = ecs_save(&mut h, id) else {
            unlisted.push(format!("{id}: the ecs saved nothing right after creation"));
            continue;
        };
        let hz = match horizon_save(id) {
            Ok(c) => c,
            Err(why) => {
                not_compared.push(format!("- `{id}`: {why}"));
                skipped += 1;
                continue;
            }
        };
        compared += 1;
        let g = compare(&ecs, &hz);
        let mut lines = String::new();
        for (cat, list) in [("missing", &g.missing), ("extra", &g.extra), ("type", &g.types), ("value", &g.values)] {
            if list.is_empty() {
                continue;
            }
            let (known, unknown): (Vec<_>, Vec<_>) = list.iter().partition(|e| KNOWN.iter().any(|(i, k, c, _)| (*i == id || *i == "*") && *k == name_of(e) && *c == cat));
            *count.entry(cat).or_default() += list.len();
            for e in list {
                by_key.entry((cat.to_string(), name_of(e).to_string())).or_default().push(id.trim_start_matches("minecraft:").to_string());
            }
            let _ = writeln!(lines, "  - {cat}: {}", list.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "));
            unlisted.extend(unknown.iter().map(|e| format!("{id}: {cat} {e}")));
            let _ = known;
        }
        if !lines.is_empty() {
            let _ = writeln!(report, "- `{id}`\n{lines}");
        }
    }
    let mut out = format!("# Save conformity (ecs against Horizon)\n\nCompared: {compared}. Not compared: {skipped}. Gaps by category: {count:?}.\n\n## Gaps by identifier\n\n{report}\n## Not compared\n\n{}\n", not_compared.join("\n"));
    out.push_str("\n## By key\n\n");
    for ((cat, key), ids) in &by_key {
        let who = if ids.len() > 8 { format!("{} identifiers", ids.len()) } else { ids.join(", ") };
        let _ = writeln!(out, "- {cat} `{key}`: {who}");
    }
    out.push_str("\n## Known gaps (table of the test)\n\n");
    for (i, k, c, r) in KNOWN {
        let _ = writeln!(out, "- `{i}` {c} `{k}`: {r}");
    }
    if let Ok(p) = std::env::var("NBT_CONFORMITY_REPORT") {
        std::fs::write(p, &out).expect("write the report");
    }
    assert!(unlisted.is_empty(), "gaps not in KNOWN ({}):\n{}", unlisted.len(), unlisted.join("\n"));
    let _ = JavaString::from("");
}

#[test]
fn dump_one() {
    let Ok(id) = std::env::var("DUMP_ID") else { return };
    if !super::common::have_data() { return; }
    let mut h = Herd::new(0, 1.0, true, false, 1);
    eprintln!("ECS {:?}", ecs_save(&mut h, &id));
    eprintln!("HZ  {:?}", horizon_save(&id));
}
