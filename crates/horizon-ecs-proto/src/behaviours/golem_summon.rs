//! Golem de fer appelé quand des villageois s'accordent (`villagers.rs:134-148`, `villager.rs:855-974`) : à la rencontre, si
//! `NEEDED` villageois de la boîte (10 blocs autour de a) ont dormi dans le jour et n'ont pas vu de golem, un golem apparaît
//! sur une colonne trouvée près de a (10 essais, `summon_golem`), et ces villageois ne veulent plus d'autre golem 599 ticks.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::*;
use crate::goals::{self, Events};
use horizon_client_lib::mobs::{
    data::{SOLID, STURDY_TOP},
    spawn_rules::{is_valid_empty_spawn, SpawnLevel},
    Species,
};

pub const NEEDED: usize = 5;
const BOX: f64 = 10.0;
const SLEPT: u64 = 24000;
const CALM: u64 = 599;
/// Son d'apparition : `sounds[0]` de `minecraft:iron_golem` (donnée de l'espèce, socle ; vide tant qu'il n'est pas branché).
pub const GOLEM_SOUND: &str = "";

/// Dernière nuit dormie (écrite par le comportement de sommeil, à brancher).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct LastSlept(pub u64);

pub static GOLEM_DETECTED: MemKey = MemKey::new("horizon:golem_detected", Shape::Unit);

fn wants(w: &World, e: Entity, now: u64) -> bool {
    w.get::<LastSlept>(e).is_some_and(|s| now.saturating_sub(s.0) < SLEPT) && !present(w, e, &GOLEM_DETECTED)
}

/// `LEGACY_IRON_GOLEM` : un sol qui n'est ni cactus, ni verre, ni feuilles, etc.
fn floor_ok(name: &str) -> bool {
    !matches!(
        name,
        "minecraft:cobweb"
            | "minecraft:cactus"
            | "minecraft:glass_pane"
            | "minecraft:conduit"
            | "minecraft:ice"
            | "minecraft:tnt"
            | "minecraft:glowstone"
            | "minecraft:beacon"
            | "minecraft:sea_lantern"
            | "minecraft:frosted_ice"
            | "minecraft:tinted_glass"
            | "minecraft:glass"
    ) && !name.ends_with("_stained_glass_pane")
        && !name.ends_with("_stained_glass")
        && !name.ends_with("leaves")
}

/// Une colonne tirée près de `start`, descendue depuis 6 au-dessus pour trouver un sol plein sous de l'air ou un liquide.
fn attempt<L: SpawnLevel>(l: &L, start: [i32; 3], rng: &mut crate::cow::Rng) -> Option<[i32; 3]> {
    let x = start[0] + rng.next_int(17) - 8;
    let z = start[2] + rng.next_int(17) - 8;
    let mut y = start[1] + 6;
    let mut above = l.state(x, y, z);
    for _ in 0..=12 {
        y -= 1;
        let here = l.state(x, y, z);
        let above_name = &*l.facts().get(above).name;
        let open = l.facts().is_air(above) || above_name == "minecraft:water" || above_name == "minecraft:lava";
        let name = &*l.facts().get(here).name;
        let solid = l.mob_data().state(here).flags & SOLID != 0 || name == "minecraft:powder_snow";
        if floor_ok(name) && open && solid {
            return Some([x, y + 1, z]);
        }
        above = here;
    }
    None
}

/// `IronGolem.checkSpawnObstruction` : un sol au dessus plein, les pieds libres, et les deux blocs au-dessus libres.
fn fits<L: SpawnLevel>(l: &L, [x, y, z]: [i32; 3]) -> bool {
    let floor = l.mob_data().state(l.state(x, y - 1, z));
    let stands = floor.flags & STURDY_TOP != 0 && floor.top >= 1.0;
    let feet = is_valid_empty_spawn(Species::IronGolem, l, x, y, z) || &*l.facts().get(l.state(x, y, z)).name == "minecraft:water";
    stands
        && feet
        && is_valid_empty_spawn(Species::IronGolem, l, x, y + 1, z)
        && is_valid_empty_spawn(Species::IronGolem, l, x, y + 2, z)
}

/// Appelé après une rencontre de `a`. Rend si un golem a été appelé.
pub fn check(w: &mut World, a: Entity, needed: usize) -> bool {
    let now = now_of(w, a);
    if !wants(w, a, now) {
        return false;
    }
    let at = w.get::<Pos>(a).unwrap().0;
    let sid = *w.get::<SpeciesId>(a).unwrap();
    let near: Vec<Entity> = {
        let mut q = w.query::<(Entity, &Pos, &SpeciesId)>();
        q.iter(w)
            .filter(|(_, p, s)| **s == sid && (p.0.x - at.x).abs() <= BOX && (p.0.y - at.y).abs() <= BOX && (p.0.z - at.z).abs() <= BOX)
            .map(|(e, _, _)| e)
            .collect()
    };
    let wanting = near.iter().filter(|&&j| wants(w, j, now)).take(needed).count();
    if wanting < needed {
        return false;
    }
    let mw = w.resource::<Level>().mw.0;
    let start = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
    let found = goals::with_rng(w, a, |r| (0..10).find_map(|_| attempt(mw, start, r).filter(|p| fits(mw, *p))));
    let Some([x, y, z]) = found else { return false };
    let golem = w.resource::<SpeciesTable>().0.iter().copied().find(|s| s.name == "minecraft:iron_golem").expect("iron golem species");
    let seed = goals::with_rng(w, a, |r| r.next_u32() as u64);
    let g = spawn_armed(w, &golem, f64::from(x) + 0.5, f64::from(z) + 0.5, seed);
    w.get_mut::<Pos>(g).unwrap().0.y = f64::from(y);
    if !GOLEM_SOUND.is_empty() {
        let at = [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5];
        w.resource_mut::<Events>().host.sound(GOLEM_SOUND, at, 1.0, 1.0);
    }
    for j in near {
        set(w, j, &GOLEM_DETECTED, MemValue::Unit, Some(CALM));
    }
    true
}
