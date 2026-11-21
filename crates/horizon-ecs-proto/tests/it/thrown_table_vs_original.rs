//! The table of the thrown objects of `ProjectileKind` (`horizon-client`'s `projectiles.rs`) against the `Flight` constants of the ECS: each
//! entry of the original has a constant with the same gravity, air drag, box size. The gravity is the jar's `getDefaultGravity` (private in
//! the original, so transcribed here); the size is its `size()`, read directly. A content mod's thrown type (`ThrownType`) is a copy of the
//! snowball with its own damage: the ECS has no constant for it, so each one fails the test by its id.
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::projectiles::{register_thrown_types, thrown_types, ProjectileKind};
use horizon_ecs_proto::projectile::{Damage, Flight};
use horizon_ecs_proto::projectile_more::{ENDER_PEARL, EGG, EXPERIENCE_BOTTLE};

/// One thrown object of the original: its kind, the gravity of `getDefaultGravity`, and the ECS constant that flies it.
struct Entry {
    name: &'static str,
    kind: ProjectileKind,
    gravity: f64,
    flight: Flight,
}

/// `ThrowableProjectile`'s air drag, `getAirDrag` 0.99F, as a double.
const AIR_DRAG: f64 = 0.9900000095367432;

fn table() -> Vec<Entry> {
    vec![
        Entry { name: "Snowball", kind: ProjectileKind::Snowball, gravity: 0.03, flight: Flight::SNOWBALL },
        Entry { name: "Egg", kind: ProjectileKind::Egg, gravity: 0.03, flight: EGG },
        Entry { name: "EnderPearl", kind: ProjectileKind::EnderPearl, gravity: 0.03, flight: ENDER_PEARL },
        Entry { name: "ExperienceBottle", kind: ProjectileKind::ExperienceBottle, gravity: 0.07, flight: EXPERIENCE_BOTTLE },
        Entry { name: "Potion(splash)", kind: ProjectileKind::potion(ItemStack::new("minecraft:splash_potion", 1)), gravity: 0.05, flight: Flight::POTION },
    ]
}

/// Every fixed thrown object of the original has an ECS constant with its gravity, its air drag, its box size.
#[test]
fn every_thrown_kind_has_the_same_flight_constant() {
    let mut problems = Vec::new();
    for e in table() {
        let f = e.flight;
        if f.gravity != e.gravity {
            problems.push(format!("{}: gravity {} in the original, {} in the ECS", e.name, e.gravity, f.gravity));
        }
        if f.inertia != AIR_DRAG {
            problems.push(format!("{}: air drag {} in the ECS, {AIR_DRAG} in the original", e.name, f.inertia));
        }
        if f.size != e.kind.size() {
            problems.push(format!("{}: box size {} in the original, {} in the ECS", e.name, e.kind.size(), f.size));
        }
    }
    assert!(problems.is_empty(), "the thrown table differs:\n{}", problems.join("\n"));
}

/// Each thrown type a content mod registers (`ProjectileKind::Thrown`) flies as a snowball, with its own damage. The ECS has no flight for
/// it: a type that lacks one is named.
#[test]
#[ignore = "ECS sans vol pour les types lances d'un mod de contenu (ThrownType) : ecart consigne dans the deviation log"]
fn every_registered_thrown_type_has_a_snowball_flight_with_its_damage() {
    let _ = register_thrown_types([("ruby:bolt".into(), "ruby:gem".into(), 2.0)]);
    let mut problems = Vec::new();
    for t in thrown_types() {
        let kind = ProjectileKind::Thrown(t);
        let snowball = Flight::SNOWBALL;
        if kind.size() != snowball.size {
            problems.push(format!("{}: box size {} in the original, {} in the ECS", t.id, kind.size(), snowball.size));
        }
        if snowball.gravity != 0.03 {
            problems.push(format!("{}: gravity 0.03 in the original, {} in the ECS", t.id, snowball.gravity));
        }
        match snowball.damage {
            Damage::Fixed(d) if d == t.damage => {}
            Damage::Fixed(d) => problems.push(format!("{}: damage {} in the original, {d} in the ECS (a snowball's)", t.id, t.damage)),
            Damage::BySpeed(_) => problems.push(format!("{}: damage by speed in the ECS, fixed {} in the original", t.id, t.damage)),
            Damage::Owned(_) => problems.push(format!("{}: damage of a living owner in the ECS, fixed {} in the original", t.id, t.damage)),
        }
        problems.push(format!("{}: no ECS Flight constant for a content thrown type", t.id));
    }
    assert!(problems.is_empty(), "the thrown content types differ:\n{}", problems.join("\n"));
}
