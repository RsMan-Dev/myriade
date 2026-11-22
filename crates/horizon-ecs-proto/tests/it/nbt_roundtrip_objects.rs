//! Non-creature entities (items, orbs, projectiles, falling blocks, rockets, tnt, boats, carts, frames, paintings, crystals, clouds, stands):
//! no Horizon writer builds them from nothing, so the reference compound is built here from the keys the writers produce (`entity_nbt`:
//! `Base::tag`, `armor_stand_to_nbt`, `orb_to_nbt`, `arrow_to_nbt`, `fireball_to_nbt`, `throwable_to_nbt`, `spit_or_bullet_to_nbt`,
//! `falling_block_to_nbt`, `tnt_to_nbt`, `item_frame_to_nbt`...). Loaded into the ecs then saved, it must come back with the same keys, types
//! and values. The statement is written to `NBT_ROUNDTRIP_REPORT` when set; a difference `KNOWN` does not list fails the test.
use horizon_core::nbt::{Compound, Tag};
use horizon_ecs_proto::{cow::Herd, nbt};
use std::fmt::Write as _;

/// (identifier, key): known differences, with the reason in the report of the branch.
const KNOWN: &[(&str, &str)] = &[];

/// Identifiers not compared by round trip, with why.
const SKIPPED: &[(&str, &str)] = &[
    ("minecraft:ender_dragon", "no Horizon writer: its keys are the ecs's own (`dragon::save`: DragonPhase and the living ones)"),
    ("minecraft:fishing_bobber", "not persisted by the game (a hook is discarded with its chunk); the ecs has no builder from a compound"),
    ("minecraft:leash_knot", "the ecs has no builder from a compound (the knot is a block position of a lead)"),
];

fn d(v: f64) -> Tag {
    Tag::Double(v.to_bits())
}
fn f(v: f32) -> Tag {
    Tag::Float(v.to_bits())
}
fn doubles(v: [f64; 3]) -> Tag {
    Tag::List(v.iter().map(|x| d(*x)).collect())
}
fn s(v: &str) -> Tag {
    Tag::String(v.into())
}
fn item(id: &str, n: i32) -> Tag {
    Tag::compound([("id", s(id)), ("count", Tag::Int(n))])
}
fn state(name: &str) -> Tag {
    Tag::compound([("id", s(name))])
}

/// The keys `Base::tag` writes for any non-creature entity, then the ones of its own writer.
fn reference(id: &str) -> Vec<(&'static str, Tag)> {
    let mut v: Vec<(&'static str, Tag)> = vec![
        ("UUID", Tag::IntArray(vec![1, 2, 3, 4])),
        ("Motion", doubles([0.25, -0.5, 0.125])),
        ("Rotation", Tag::List(vec![f(30.0), f(15.0)])),
        ("OnGround", Tag::Byte(0)),
        ("fall_distance", d(0.0)),
        ("Fire", Tag::Short(0)),
        ("Air", Tag::Short(300)),
        ("Invulnerable", Tag::Byte(0)),
        ("PortalCooldown", Tag::Int(0)),
    ];
    let own: Vec<(&'static str, Tag)> = match id {
        "minecraft:armor_stand" => vec![
            ("DisabledSlots", Tag::Int(0)),
            ("Health", f(20.0)),
            ("AbsorptionAmount", f(0.0)),
            ("DeathTime", Tag::Short(0)),
            ("HurtTime", Tag::Short(0)),
            ("FallFlying", Tag::Byte(0)),
            ("current_impulse_context_reset_grace_time", Tag::Int(0)),
            ("attributes", Tag::List(vec![])),
            ("Brain", Tag::compound([("memories", Tag::Compound(Compound::new()))])),
            ("Pose", Tag::Compound(Compound::new())),
            ("Small", Tag::Byte(0)),
            ("ShowArms", Tag::Byte(0)),
            ("NoBasePlate", Tag::Byte(0)),
            ("Invisible", Tag::Byte(0)),
        ],
        "minecraft:item" => vec![("Item", item("minecraft:stone", 3)), ("Age", Tag::Short(5)), ("PickupDelay", Tag::Short(10)), ("Health", Tag::Short(5))],
        "minecraft:experience_orb" => vec![("Health", Tag::Short(5)), ("Age", Tag::Short(7)), ("Value", Tag::Short(3)), ("Count", Tag::Int(1))],
        "minecraft:arrow" | "minecraft:trident" => vec![("item", item("minecraft:arrow", 1)), ("HasBeenShot", Tag::Byte(1)), ("inGround", Tag::Byte(0)), ("crit", Tag::Byte(0)), ("life", Tag::Short(0)), ("shake", Tag::Byte(0)), ("pickup", Tag::Byte(1)), ("damage", d(2.0)), ("PierceLevel", Tag::Byte(0)), ("DealtDamage", Tag::Byte(0))],
        "minecraft:fireball" => vec![("HasBeenShot", Tag::Byte(1)), ("acceleration_power", d(0.1)), ("ExplosionPower", Tag::Byte(1))],
        "minecraft:wither_skull" => vec![("HasBeenShot", Tag::Byte(1)), ("acceleration_power", d(0.1)), ("dangerous", Tag::Byte(0))],
        "minecraft:small_fireball" | "minecraft:dragon_fireball" => vec![("HasBeenShot", Tag::Byte(1)), ("acceleration_power", d(0.1))],
        "minecraft:egg" | "minecraft:snowball" | "minecraft:ender_pearl" | "minecraft:experience_bottle" | "minecraft:splash_potion" | "minecraft:lingering_potion" => vec![("HasBeenShot", Tag::Byte(1)), ("Item", item("minecraft:snowball", 1))],
        "minecraft:wind_charge" | "minecraft:breeze_wind_charge" => vec![("HasBeenShot", Tag::Byte(1)), ("acceleration_power", d(0.1))],
        "minecraft:llama_spit" | "minecraft:shulker_bullet" => vec![("HasBeenShot", Tag::Byte(1))],
        "minecraft:falling_block" => vec![("BlockState", state("minecraft:stone")), ("Time", Tag::Int(4)), ("DropItem", Tag::Byte(1)), ("HurtEntities", Tag::Byte(0)), ("FallHurtAmount", f(0.0)), ("FallHurtMax", Tag::Int(40)), ("CancelDrop", Tag::Byte(0))],
        "minecraft:tnt" => vec![("fuse", Tag::Short(80)), ("block_state", state("minecraft:tnt")), ("explosion_power", f(4.0))],
        "minecraft:firework_rocket" => vec![("Life", Tag::Int(10)), ("LifeTime", Tag::Int(30)), ("ShotAtAngle", Tag::Byte(0))],
        "minecraft:item_frame" | "minecraft:glow_item_frame" => vec![("Facing", Tag::Byte(2)), ("Fixed", Tag::Byte(0)), ("Invisible", Tag::Byte(0)), ("ItemDropChance", f(1.0)), ("ItemRotation", Tag::Byte(0)), ("block_pos", Tag::IntArray(vec![0, 1, 0]))],
        "minecraft:painting" => vec![("Facing", Tag::Byte(2)), ("variant", s("minecraft:kebab")), ("block_pos", Tag::IntArray(vec![0, 1, 0]))],
        "minecraft:end_crystal" => vec![("ShowBottom", Tag::Byte(1))],
        "minecraft:block_display" | "minecraft:item_display" | "minecraft:text_display" => vec![("width", f(0.0)), ("height", f(0.0))],
        "minecraft:interaction" => vec![("width", f(0.0)), ("height", f(0.0)), ("response", Tag::Byte(0))],
        "minecraft:ominous_item_spawner" => vec![("item", item("minecraft:stone", 1)), ("spawn_item_after_ticks", Tag::Long(100))],
        _ if id.ends_with("chest_boat") || id.ends_with("chest_raft") => vec![("Items", Tag::List(vec![]))],
        _ => vec![],
    };
    v.extend(own);
    v
}

#[test]
fn objects_come_back_as_they_were_loaded() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::new(0, 1.0, true, false, 1);
    h.set_players(vec![(0.5, 0.5)]);
    for _ in 0..20 {
        h.tick();
    }
    let ids: Vec<String> = nbt::recipes(&h.world).into_iter().map(|(k, _)| k).collect();
    let (mut report, mut bad, mut ok, mut lost) = (String::new(), Vec::new(), 0, Vec::new());
    for id in ids {
        if super::nbt_conformance::is_creature(&id) || SKIPPED.iter().any(|(i, _)| *i == id) {
            continue;
        }
        let want: Compound = {
            let mut c: Vec<(&str, Tag)> = Vec::new();
            c.push(("id", s(&id)));
            c.push(("Pos", doubles([0.5, 1.0, 0.5])));
            for (k, t) in reference(&id) {
                c.push((k, t));
            }
            let Tag::Compound(c) = Tag::compound(c) else { unreachable!() };
            c
        };
        let Some(e) = nbt::restore(&mut h.world, &want) else {
            lost.push(id);
            continue;
        };
        let Some(got) = nbt::save(&h.world, e) else {
            lost.push(format!("{id} (restored, not saved)"));
            continue;
        };
        let mut diffs = Vec::new();
        for (k, v) in &want {
            match got.get(k) {
                None => diffs.push(format!("{} missing", k.to_string_lossy())),
                Some(g) if g.id() != v.id() => diffs.push(format!("{} type {} against {}", k.to_string_lossy(), g.id(), v.id())),
                Some(g) if g != v => diffs.push(format!("{} value {g:?} against {v:?}", k.to_string_lossy())),
                _ => {}
            }
        }
        for k in got.keys() {
            if !want.contains_key(k) {
                diffs.push(format!("{} extra {:?}", k.to_string_lossy(), got.get(k).unwrap()));
            }
        }
        if diffs.is_empty() {
            ok += 1;
        } else {
            let _ = writeln!(report, "- `{id}`: {}", diffs.join("; "));
            for dd in &diffs {
                let key = dd.split(' ').next().unwrap_or("");
                if !KNOWN.iter().any(|(i, k)| *i == id && *k == key) {
                    bad.push(format!("{id}: {dd}"));
                }
            }
        }
    }
    let out = format!("# Round trip of the non-creature entities\n\nSame after load and save: {ok}. With differences: {}. Not loadable: {}.\n\n## Differences\n\n{report}\n## Not loadable\n\n{}\n\n## Skipped\n\n{}\n", report.lines().count(), lost.len(), lost.iter().map(|l| format!("- `{l}`")).collect::<Vec<_>>().join("\n"), SKIPPED.iter().map(|(i, w)| format!("- `{i}`: {w}")).collect::<Vec<_>>().join("\n"));
    if let Ok(p) = std::env::var("NBT_ROUNDTRIP_REPORT") {
        std::fs::write(p, &out).expect("write the report");
    }
    assert!(bad.is_empty() && lost.is_empty(), "{bad:?} {lost:?}");
}
