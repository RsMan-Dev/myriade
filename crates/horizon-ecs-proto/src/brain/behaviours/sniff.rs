//! The seed hunt (`SnifferAi`, `animal_brain.rs:1263-1345` starts, `:1737-1790` ticks, `:1852-1905` stops, `Sniffer.rs` the state): an animal
//! with a keen nose scents, sniffs, walks to a diggable place, digs a seed up and is happy. Six behaviours share one state (`State`, with the
//! explored places) and the memories `sniffer_digging`, `sniffer_sniffing_target`, `sniffer_happy` and `sniff_cooldown`; three activities
//! pick between them (`dig`, `sniff`, `idle`: configuration). Every behaviour is timed and PASSIVE: the brain wakes at the end of its time
//! and at each memory write, which is when anything can change (`can_sniff` / `can_dig` are read then). The seed drops 120 ticks into the
//! dig: a memory that lasts that long wakes the brain, where it is dropped. Not done: the sounds of the transitions (no sound event in the
//! frame), the path test of `can_dig_at` (`create_path_to(.., 1).reached`: the frame has no path query), the leash and the head pitch of
//! `head_block`.
use crate::behaviours::breed::Love;
use crate::behaviours::Cap;
use crate::brain::*;
use crate::goals::{is_occupied, OCC_MOUNTED};
use crate::rules::sniff as rules;

pub static SNIFFER_DIGGING: MemKey = MemKey::new("minecraft:sniffer_digging", Shape::Bool);
pub static SNIFFER_SNIFFING_TARGET: MemKey = MemKey::new("minecraft:sniffer_sniffing_target", Shape::Block);
pub static SNIFFER_HAPPY: MemKey = MemKey::new("minecraft:sniffer_happy", Shape::Bool);
pub static SNIFF_COOLDOWN: MemKey = MemKey::new("minecraft:sniff_cooldown", Shape::Unit);
/// Present until the seed drops (120 ticks into the dig).
pub static SEED_PENDING: MemKey = MemKey::new("minecraft:seed_pending", Shape::Bool);

pub const IDLING: u8 = 0;
pub const FEELING_HAPPY: u8 = 1;
pub const SCENTING: u8 = 2;
pub const SNIFFING: u8 = 3;
pub const SEARCHING: u8 = 4;
pub const DIGGING: u8 = 5;
pub const RISING: u8 = 6;

/// `Sniffer.canMate`'s state test: only a calm state can mate.
pub fn can_mate_state(s: u8) -> bool {
    matches!(s, IDLING | SCENTING | FEELING_HAPPY)
}
/// The state of the animal (`Sniffer.getState`) and the places it dug (`SNIFFER_EXPLORED_POSITIONS`, newest first, at most `explored_max`).
#[derive(Component, Clone, Debug)]
pub struct State {
    pub state: u8,
    pub explored: Vec<[i32; 3]>,
    /// The seed of the dig in progress is not yet dropped.
    seed_due: bool,
    /// `DATA_DROP_SEED_AT_TICK`: the own time (`tickCount`) of the last dig's start plus the seed delay, kept after the drop; 0 before any dig.
    pub seed_at: i32,
}
/// `Sniffer.DATA_DROP_SEED_AT_TICK` for the synchronisation (0 is the default).
pub fn seed_at(w: &World, e: Entity) -> i32 {
    w.get::<State>(e).map_or(0, |s| s.seed_at)
}
/// `Sniffer.canMate`'s use of the state, as `make_love`'s `ready` reads it.
pub fn calm(w: &World, e: Entity) -> bool {
    w.get::<State>(e).is_none_or(|s| can_mate_state(s.state))
}
pub fn state(w: &World, e: Entity) -> u8 {
    w.get::<State>(e).map_or(IDLING, |s| s.state)
}

/// The species' values: the cooldown after a dig (9600 ticks), the walk to the sniffed place (speed 1.25), the delay of the seed (120), the
/// loot table of the dig, the tag of the blocks that can be dug, how many explored places are kept (20).
pub struct Params {
    pub cooldown: u64,
    pub speed: f32,
    pub seed_delay: u64,
    pub table: &'static str,
    pub diggable: &'static str,
    pub explored_max: usize,
}
impl Params {
    pub const DEFAULT: Params = Params { cooldown: 9600, speed: 1.25, seed_delay: 120, table: "minecraft:gameplay/sniffer_digging", diggable: "minecraft:sniffer_diggable_block", explored_max: 20 };
}

fn equip(w: &mut World, e: Entity, _: &Ctx) {
    if w.get::<State>(e).is_none() {
        w.entity_mut(e).insert(State { state: IDLING, explored: Vec::new(), seed_due: false, seed_at: 0 });
    }
}
fn transition(w: &mut World, e: Entity, to: u8) {
    if let Some(mut s) = w.get_mut::<State>(e) {
        s.state = to;
    }
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
fn baby(w: &World, e: Entity) -> bool {
    w.get::<GrowAt>(e).is_some()
}
/// `Sniffer.canSniff`.
pub fn can_sniff(w: &World, e: Entity) -> bool {
    !present(w, e, &TEMPTING_PLAYER)
        && !present(w, e, &IS_PANICKING)
        && !in_water(w, e)
        && w.get::<Love>(e).is_none_or(|l| l.0 <= now_of(w, e))
        && w.get::<Contact>(e).is_some_and(|c| c.on_ground)
        && !is_occupied(w, e, OCC_MOUNTED)
}
/// `Sniffer.getHeadBlock`: 2.25 blocks along the facing, at its feet plus 0.2 (the head pitch is not read).
fn head_block(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
    rules::head_cell(p, yaw)
}
/// `Sniffer.getOnPos`: the block under its feet.
fn on_pos(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    rules::on_cell(p)
}
/// `Sniffer.canDig(BlockPos)` without the path test: a diggable block that is not explored.
fn can_dig_at(w: &World, e: Entity, p: [i32; 3], tag: &str) -> bool {
    let mw = w.resource::<Level>().mw.0;
    mw.has_tag(tag, mw.block_name(p[0], p[1], p[2])) && w.get::<State>(e).is_some_and(|s| !s.explored.contains(&p))
}
/// `Sniffer.canDig()`.
fn can_dig(w: &World, e: Entity, tag: &str) -> bool {
    if present(w, e, &IS_PANICKING) || present(w, e, &TEMPTING_PLAYER) || baby(w, e) || in_water(w, e) || !w.get::<Contact>(e).is_some_and(|c| c.on_ground) || is_occupied(w, e, OCC_MOUNTED) {
        return false;
    }
    let h = head_block(w, e);
    can_dig_at(w, e, [h[0], h[1] - 1, h[2]], tag)
}
/// `Sniffer.calculateDigPosition`: the first of five ever wider land positions whose block below can be dug.
fn dig_position(w: &mut World, e: Entity, tag: &str) -> Option<[i32; 3]> {
    for idx in 0..5 {
        let Some(p) = pick(w, e, crate::spots::Pick::Land { h: 10 + 2 * idx, v: 3 }) else { continue };
        let below = rules::below_cell(p);
        if can_dig_at(w, e, below, tag) {
            return Some(below);
        }
    }
    None
}
/// `SnifferAi.resetSniffing`.
fn reset_sniffing(w: &mut World, e: Entity) {
    erase(w, e, &SNIFFER_DIGGING);
    erase(w, e, &SNIFFER_SNIFFING_TARGET);
    transition(w, e, IDLING);
}
fn calm_mem(w: &World, e: Entity) -> bool {
    !present(w, e, &IS_PANICKING)
}

// ---- FeelingHappy -----------------------------------------------------------------------------------------------------
pub static FEELING_HAPPY_B: Behaviour = Behaviour { name: "minecraft:feeling_happy", mems: &[&SNIFFER_HAPPY], requires: &[Cap::Brain, Cap::Body], passive: true, equip, start: start_happy, stop: stop_happy, ..Behaviour::DEFAULT };
fn start_happy(w: &mut World, e: Entity, _: &Ctx) {
    transition(w, e, FEELING_HAPPY);
}
fn stop_happy(w: &mut World, e: Entity, _: &Ctx) {
    transition(w, e, IDLING);
    erase(w, e, &SNIFFER_HAPPY);
}

// ---- Scenting ---------------------------------------------------------------------------------------------------------
pub static SCENTING_B: Behaviour = Behaviour { name: "minecraft:scenting", mems: &[&SNIFFER_DIGGING, &SNIFFER_SNIFFING_TARGET, &SNIFFER_HAPPY, &BREED_TARGET, &IS_PANICKING, &TEMPTING_PLAYER], requires: &[Cap::Brain, Cap::Body], passive: true, equip, can_start: can_scent, start: start_scent, stop: stop_scent, ..Behaviour::DEFAULT };
fn can_scent(w: &mut World, e: Entity, _: &Ctx) -> bool {
    !(present(w, e, &IS_PANICKING) || present(w, e, &SNIFFER_DIGGING) || present(w, e, &SNIFFER_SNIFFING_TARGET) || present(w, e, &SNIFFER_HAPPY) || present(w, e, &BREED_TARGET) || present(w, e, &TEMPTING_PLAYER))
}
fn start_scent(w: &mut World, e: Entity, _: &Ctx) {
    transition(w, e, SCENTING);
}
fn stop_scent(w: &mut World, e: Entity, _: &Ctx) {
    transition(w, e, IDLING);
}

// ---- Sniffing ---------------------------------------------------------------------------------------------------------
pub static SNIFFING_B: Behaviour = Behaviour { name: "minecraft:sniffing", mems: &[&WALK_TARGET, &SNIFFER_SNIFFING_TARGET, &SNIFF_COOLDOWN], requires: &[Cap::Brain, Cap::Body], passive: true, equip, can_start: can_sniff_start, start: start_sniff, tick: tick_sniff, stop: stop_sniff, ..Behaviour::DEFAULT };
fn can_sniff_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    !(present(w, e, &WALK_TARGET) || present(w, e, &SNIFFER_SNIFFING_TARGET) || present(w, e, &SNIFF_COOLDOWN)) && !baby(w, e) && can_sniff(w, e)
}
fn start_sniff(w: &mut World, e: Entity, _: &Ctx) {
    transition(w, e, SNIFFING);
}
fn tick_sniff(w: &mut World, e: Entity, _: &Ctx) -> bool {
    can_sniff(w, e)
}
fn stop_sniff(w: &mut World, e: Entity, c: &Ctx) {
    let finished = timed_out(w, e, c);
    transition(w, e, IDLING);
    if finished && let Some(p) = dig_position(w, e, c.get::<Params>().diggable) {
        set(w, e, &SNIFFER_SNIFFING_TARGET, MemValue::Block(p), None);
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Block(p), speed: c.get::<Params>().speed, close: 0 }), None);
    }
}

// ---- Searching --------------------------------------------------------------------------------------------------------
pub static SEARCHING_B: Behaviour = Behaviour { name: "minecraft:searching", mems: &[&WALK_TARGET, &SNIFFER_SNIFFING_TARGET, &SNIFFER_DIGGING, &IS_PANICKING], requires: &[Cap::Brain, Cap::Body], passive: true, equip, can_start: can_search, start: start_search, tick: tick_search, stop: stop_search, ..Behaviour::DEFAULT };
fn can_search(w: &mut World, e: Entity, _: &Ctx) -> bool {
    present(w, e, &WALK_TARGET) && calm_mem(w, e) && present(w, e, &SNIFFER_SNIFFING_TARGET) && can_sniff(w, e)
}
fn start_search(w: &mut World, e: Entity, _: &Ctx) {
    transition(w, e, SEARCHING);
}
fn tick_search(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if !can_sniff(w, e) {
        transition(w, e, IDLING);
        return false;
    }
    let walking = walk_target(w, e).and_then(|t| t.target.block(w));
    let wanted = match get(w, e, &SNIFFER_SNIFFING_TARGET) {
        Some(MemValue::Block(b)) => Some(*b),
        _ => None,
    };
    walking.is_some() && walking == wanted
}
fn stop_search(w: &mut World, e: Entity, c: &Ctx) {
    if can_dig(w, e, c.get::<Params>().diggable) && can_sniff(w, e) {
        set(w, e, &SNIFFER_DIGGING, MemValue::Bool(true), None);
    }
    erase(w, e, &WALK_TARGET);
    erase(w, e, &SNIFFER_SNIFFING_TARGET);
}

// ---- Digging ----------------------------------------------------------------------------------------------------------
pub static DIGGING_B: Behaviour = Behaviour { name: "minecraft:digging", mems: &[&IS_PANICKING, &WALK_TARGET, &SNIFFER_DIGGING, &SNIFF_COOLDOWN, &SEED_PENDING], requires: &[Cap::Brain, Cap::Body], passive: true, equip, can_start: can_start_dig, start: start_dig, tick: tick_dig, stop: stop_dig, ..Behaviour::DEFAULT };
fn can_start_dig(w: &mut World, e: Entity, c: &Ctx) -> bool {
    !(present(w, e, &IS_PANICKING) || present(w, e, &WALK_TARGET) || !present(w, e, &SNIFFER_DIGGING) || present(w, e, &SNIFF_COOLDOWN)) && can_sniff(w, e) && {
        let _ = c;
        true
    }
}
fn start_dig(w: &mut World, e: Entity, c: &Ctx) {
    transition(w, e, DIGGING);
    let p = c.get::<Params>();
    let at = (now_of(w, e) as i64 + p.seed_delay as i64) as i32;
    if let Some(mut s) = w.get_mut::<State>(e) {
        s.seed_due = true;
        s.seed_at = at;
    }
    set(w, e, &SEED_PENDING, MemValue::Bool(true), Some(p.seed_delay));
}
fn tick_dig(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let due = w.get::<State>(e).is_some_and(|s| s.seed_due) && !present(w, e, &SEED_PENDING);
    if due {
        w.get_mut::<State>(e).unwrap().seed_due = false;
        drop_seed(w, e, p);
    }
    present(w, e, &SNIFFER_DIGGING) && can_dig(w, e, p.diggable) && w.get::<Love>(e).is_none_or(|l| l.0 <= now_of(w, e))
}
fn stop_dig(w: &mut World, e: Entity, c: &Ctx) {
    if timed_out(w, e, c) {
        set(w, e, &SNIFF_COOLDOWN, MemValue::Unit, Some(c.get::<Params>().cooldown));
    } else {
        reset_sniffing(w, e);
    }
    erase(w, e, &SEED_PENDING);
}
/// `Sniffer.dropSeed`: the loot of the dig at the head block.
fn drop_seed(w: &mut World, e: Entity, p: &Params) {
    if w.get::<State>(e).is_none_or(|s| s.state != DIGGING) {
        return;
    }
    let at = head_block(w, e);
    let (hi, lo) = crate::goals::with_rng(w, e, |r| (r.next_u32(), r.next_u32()));
    let seed = rules::loot_seed(hi, lo);
    for stack in crate::loot::roll_table(w, p.table, seed) {
        crate::item::drop_item(w, stack, horizon_core::geometry::Vec3::new(f64::from(at[0]), f64::from(at[1]), f64::from(at[2])), horizon_core::geometry::Vec3::ZERO, 10);
    }
}

// ---- FinishedDigging --------------------------------------------------------------------------------------------------
pub static FINISHED_DIGGING_B: Behaviour = Behaviour { name: "minecraft:finished_digging", mems: &[&IS_PANICKING, &WALK_TARGET, &SNIFFER_DIGGING, &SNIFF_COOLDOWN, &SNIFFER_HAPPY], requires: &[Cap::Brain, Cap::Body], passive: true, equip, can_start: can_finish, start: start_finish, tick: tick_finish, stop: stop_finish, ..Behaviour::DEFAULT };
fn can_finish(w: &mut World, e: Entity, _: &Ctx) -> bool {
    !(present(w, e, &IS_PANICKING) || present(w, e, &WALK_TARGET) || !present(w, e, &SNIFFER_DIGGING) || !present(w, e, &SNIFF_COOLDOWN))
}
fn start_finish(w: &mut World, e: Entity, _: &Ctx) {
    transition(w, e, RISING);
}
fn tick_finish(w: &mut World, e: Entity, _: &Ctx) -> bool {
    present(w, e, &SNIFFER_DIGGING)
}
fn stop_finish(w: &mut World, e: Entity, c: &Ctx) {
    let finished = timed_out(w, e, c);
    transition(w, e, IDLING);
    if finished {
        let p = on_pos(w, e);
        let max = c.get::<Params>().explored_max;
        if let Some(mut s) = w.get_mut::<State>(e) {
            let list = std::mem::take(&mut s.explored);
            s.explored = rules::explored_after(list, p, max);
        }
    }
    erase(w, e, &SNIFFER_DIGGING);
    set(w, e, &SNIFFER_HAPPY, MemValue::Bool(true), None);
}
