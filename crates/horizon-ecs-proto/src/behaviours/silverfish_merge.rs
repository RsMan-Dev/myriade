//! `Silverfish.SilverfishMergeWithStoneGoal` (fiche 34, `mobs/hostile.rs:171-245`): the silverfish's stroll, which with mob
//! griefing now and then hides in a stone block beside it (the block turns infested, the silverfish goes). This module replaces
//! the stroll of the species (the stroll module cannot be wrapped). Each ask of the Mob (every 2 ticks) draws the merge (1 in 5,
//! then one of the six directions) or else the stroll (1 in 5); here the wait of the next success is drawn once, with the same
//! odds, and the outcome at that success.
use super::*;
use crate::{cow::*, goals::*, spots::{Pick, spotter}};
use crate::targeting::Target;
use horizon_client_lib::mobs::spawn_selection::infested_host;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed of the walk.
    pub speed: f64,
    /// `Stroll.interval`: one ask in `reduced(interval)` succeeds.
    pub interval: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, interval: 10 };
}
/// What the last success chose, read by `start`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    None,
    Merge([i32; 3]),
    Stroll([f64; 3]),
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Merge {
    pub speed: f64,
    pub interval: i32,
    pub choice: Choice,
}
/// The stroll walk runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Strolling;

/// `RandomStrollGoal`'s odds per ask: `reduced(10)` = 5.
const ODDS: f64 = 0.2;
const DIRECTIONS: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

pub static MODULE: Module = Module {
    name: "silverfish_merge",
    flags: MOVE,
    ends_with_path: true,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Merge { speed: p.speed, interval: p.interval, choice: Choice::None });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    arm_next(w, e, inst.slot);
}
/// Host blocks around the animal (the merge directions the Mob would draw), with the weight of each direction and of the
/// stroll, when no walk runs. Without mob griefing there is no merge.
fn weights(w: &World, e: Entity) -> (Vec<[i32; 3]>, f64, f64) {
    let hosts: Vec<[i32; 3]> = if mob_griefing(w) {
        let p = w.get::<Pos>(e).unwrap().0;
        let mw = w.resource::<Level>().mw.0;
        let base = [p.x.floor() as i32, (p.y + 0.5).floor() as i32, p.z.floor() as i32];
        DIRECTIONS
            .iter()
            .map(|d| [base[0] + d[0], base[1] + d[1], base[2] + d[2]])
            .filter(|q| infested_host(mw.block_name(q[0], q[1], q[2])))
            .collect()
    } else {
        Vec::new()
    };
    let per_dir = ODDS / 6.0;
    let stroll = (1.0 - per_dir * hosts.len() as f64) * ODDS;
    (hosts, per_dir, stroll)
}
/// The wait of the next success: asks until one succeeds, each ask 2 ticks, the success probability being the sum of the
/// merge and stroll odds.
fn arm_next(w: &mut World, e: Entity, slot: u8) {
    let (hosts, per_dir, stroll) = weights(w, e);
    let p = per_dir * hosts.len() as f64 + stroll;
    let asks = with_rng(w, e, |r| {
        let u = r.next_f64().max(1.0e-12);
        (u.ln() / (1.0 - p).ln()).ceil().max(1.0) as i32
    });
    crate::goals::arm(w, e, slot, 2 * asks);
}
/// A deadline that is not the goal's own (the replay of a start) asks `canUse`; a walking animal has none.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Strolling>(e).is_none()
}
/// `canUse`: a success draws its outcome. A target or a path of its own holds the goal back: asked again in 2 ticks while
/// that holds (the Mob asks every 2 ticks; no event tells when it ends).
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if w.get::<Target>(e).is_some() || w.get::<Following>(e).is_some() || w.get::<NeedsPath>(e).is_some() {
        crate::goals::arm(w, e, inst.slot, 2);
        return false;
    }
    let (hosts, per_dir, stroll) = weights(w, e);
    let total = per_dir * hosts.len() as f64 + stroll;
    let u = with_rng(w, e, |r| r.next_f64()) * total;
    let mut acc = 0.0;
    let mut choice = None;
    for h in &hosts {
        acc += per_dir;
        if u < acc {
            choice = Some(Choice::Merge(*h));
            break;
        }
    }
    let choice = match choice {
        Some(c) => c,
        None => {
            let (level, pos, nav, walk, in_water) = {
                let p = w.get::<Pos>(e).unwrap().0;
                let nav = *w.get::<NavParams>(e).unwrap();
                let walk = w.get::<crate::spots::WalkValue>(e).copied().unwrap_or(crate::spots::WalkValue::Zero);
                let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
                (*w.resource::<Level>(), [p.x, p.y, p.z], nav, walk, in_water)
            };
            let mut r = *w.get::<Rng>(e).unwrap();
            let picked = {
                let mut sp = spotter(&level, pos, &nav, walk, &mut r);
                let (h, v) = (10, 7);
                if in_water {
                    sp.pick(Pick::Land { h: h + 5, v }).or_else(|| sp.pick(Pick::Default { h, v }))
                } else if sp.rng.next_float() >= 0.001 {
                    sp.pick(Pick::Land { h, v })
                } else {
                    sp.pick(Pick::Default { h, v })
                }
            };
            *w.get_mut::<Rng>(e).unwrap() = r;
            match picked {
                Some(pt) => Choice::Stroll(pt),
                None => {
                    arm_next(w, e, inst.slot);
                    return false;
                }
            }
        }
    };
    w.get_mut::<Merge>(e).unwrap().choice = choice;
    true
}
/// `start`: a merge infests the host block and the silverfish goes (`m.removed`); a stroll walks to its spot at its speed.
fn start(w: &mut World, e: Entity, _: Inst) {
    let m = *w.get::<Merge>(e).unwrap();
    w.get_mut::<Merge>(e).unwrap().choice = Choice::None;
    match m.choice {
        Choice::Merge(pos) => {
            let host = w.resource::<Level>().mw.0.block_name(pos[0], pos[1], pos[2]);
            if infested_host(host) {
                w.resource_mut::<BlockChanges>().0.push((pos[0], pos[1], pos[2], infested(host)));
                remove(w, e);
            }
        }
        Choice::Stroll(at) => {
            go_to_at(w, e, at, m.speed, 1);
            w.entity_mut(e).insert(Strolling);
        }
        Choice::None => {}
    }
}
/// `stop`: the walk ends; the next success is drawn.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Strolling>();
    stop_nav(w, e);
    arm_next(w, e, inst.slot);
}
/// The infested block of a host (`InfestedBlock`: the same stone with the `infested_` prefix).
fn infested(host: &str) -> &'static str {
    match host {
        "minecraft:cobblestone" => "minecraft:infested_cobblestone",
        "minecraft:stone_bricks" => "minecraft:infested_stone_bricks",
        "minecraft:mossy_stone_bricks" => "minecraft:infested_mossy_stone_bricks",
        "minecraft:cracked_stone_bricks" => "minecraft:infested_cracked_stone_bricks",
        "minecraft:chiseled_stone_bricks" => "minecraft:infested_chiseled_stone_bricks",
        "minecraft:deepslate" => "minecraft:infested_deepslate",
        _ => "minecraft:infested_stone",
    }
}
/// The silverfish leaves the world: its cell leaves the index, then the entity goes.
fn remove(w: &mut World, e: Entity) {
    if let Some(c) = w.get::<Cell>(e).map(|c| c.0)
        && c != u32::MAX
    {
        w.resource_mut::<Index>().remove(e, c);
    }
    w.entity_mut(e).despawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 5, &Params::DEFAULT)]));
        Config { entries }
    }
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        // No silverfish species exists in the ECS yet: the module is species-agnostic, so a zombie carries it.
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let s = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
        h.world.get_mut::<ActiveUntil>(s).unwrap().0 = u64::MAX;
        crate::goals::arm_timers(&mut h.world, s);
        h.tick();
        Some((h, s))
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    fn block_at(h: &Herd, p: [i32; 3]) -> &str {
        h.world.resource::<Level>().mw.0.block_name(p[0], p[1], p[2])
    }
    fn feet(h: &Herd, s: Entity) -> [i32; 3] {
        let p = h.world.get::<Pos>(s).unwrap().0;
        [p.x.floor() as i32, (p.y + 0.5).floor() as i32, p.z.floor() as i32]
    }

    /// Le sol du banc est de la pierre : la direction « bas » est un hôte, le poisson s'y fond (témoin : le bloc du sol
    /// devient infesté et le poisson disparaît). La promenade n'est pas couverte par ce test (DOUTE dans les notes).
    #[test]
    fn it_merges_into_the_stone_below_it_and_the_stone_becomes_infested() {
        let Some((mut h, s)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let p = feet(&h, s);
        let below = [p[0], p[1] - 1, p[2]];
        assert_eq!(block_at(&h, below), "minecraft:stone", "témoin : le sol est un hôte");
        let infested = |h: &Herd| {
            (-30..=30)
                .flat_map(|x| (-30..=30).map(move |z| [x, 0, z]))
                .filter(|q| block_at(h, *q).starts_with("minecraft:infested_"))
                .count()
        };
        assert_eq!(infested(&h), 0, "au départ, aucun bloc infesté");
        for _ in 0..50 {
            ticks(&mut h, 100);
            if h.world.get_entity(s).is_err() {
                break;
            }
        }
        assert!(h.world.get_entity(s).is_err(), "témoin : le poisson s'est fondu dans la pierre");
        ticks(&mut h, 2);
        assert_eq!(infested(&h), 1, "la pierre où il s'est fondu devient infestée");
    }
}
