//! `StopAttackingIfTargetInvalid` (`monster_brain.rs` `B::StopAttacking`, `:678-725`, creaking packages `:1556`): a one-shot of
//! the Fight activity that clears the attack target when it is no longer valid: dead or gone, not attackable, unreachable for
//! more than `tired_ticks`, or (creaking, `still_visible`) no longer in the visible targetable players. The breeze's branch
//! (`Sensor.wasEntityAttackableLastNTicks(100)`) needs a sensor that the frame does not have: not ported (DOUTE).
use crate::behaviours::Cap;
use crate::brain::*;
use crate::targeting::{self, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:stop_attacking_if_target_invalid",
    mems: &[&ATTACK_TARGET, &CANT_REACH_WALK_TARGET_SINCE, &NEAREST_VISIBLE_TARGETABLE_PLAYERS],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// `cant_reach_walk_target_since` older than this (in the entity's own ticks) makes the target tired.
    pub tired_ticks: i64,
    /// The creaking's `isAttackTargetStillReachable`: the target must still be in `NEAREST_VISIBLE_TARGETABLE_PLAYERS`.
    pub still_visible: bool,
    /// The breeze's `Sensor.wasEntityAttackableLastNTicks(window)`: the target is dropped when it was last attackable (`sensors::attackable`'s
    /// `LastAttackable`) more than `window` ticks ago or never; the sensor's wake at the deadline brings the brain here.
    pub attackable: Option<i64>,
    /// A target that plays dead is not a target (the axolotl).
    pub not_playing_dead: bool,
    /// Losing the target starts the hunting cooldown for this many ticks (the axolotl's fight ends: 2400).
    pub hunting_cooldown: Option<u64>,
}
impl Params {
    pub const DEFAULT: Params = Params { tired_ticks: 200, still_visible: false, attackable: None, not_playing_dead: false, hunting_cooldown: None };
}

fn invalid(w: &mut World, e: Entity, p: &Params) -> bool {
    let target = match get(w, e, &ATTACK_TARGET) {
        Some(MemValue::Who(who)) => *who,
        _ => return false,
    };
    let alive = targeting::info(w, target).is_some();
    let tired = int(w, e, &CANT_REACH_WALK_TARGET_SINCE).is_some_and(|since| now_of(w, e) as i64 - since > p.tired_ticks);
    let still = !p.still_visible || matches!(get(w, e, &NEAREST_VISIBLE_TARGETABLE_PLAYERS), Some(MemValue::Whos(l)) if l.contains(&target));
    // A new target counts as attackable from its first look here (the `Mob`'s counter starts at 100); the sensor's scans keep it up.
    let stale = match p.attackable {
        Some(window) => {
            let now = now_of(w, e);
            match w.get::<crate::brain::sensors::attackable::LastAttackable>(e) {
                Some(l) => now.saturating_sub(l.0) as i64 > window,
                None => {
                    w.entity_mut(e).insert(crate::brain::sensors::attackable::LastAttackable(now));
                    false
                }
            }
        }
        None => false,
    };
    let feigning = p.not_playing_dead && matches!(target, Who::Entity(t) if super::play_dead::is_playing_dead(w, t));
    !super::start_attacking::can_attack(w, target) || tired || !alive || !still || stale || feigning
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    invalid(w, e, c.get::<Params>())
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    w.entity_mut(e).remove::<crate::brain::sensors::attackable::LastAttackable>();
    erase(w, e, &ATTACK_TARGET);
    targeting::clear_target(w, e);
    if let Some(t) = c.get::<Params>().hunting_cooldown {
        set(w, e, &crate::brain::sensors::attackables::HAS_HUNTING_COOLDOWN, MemValue::Bool(true), Some(t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Cap;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::goals::{Online, Player};
    use crate::hostile::PlayerState;
    use crate::targeting::{Target, Who};

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static NEEDS: Needs = &[(&ATTACK_TARGET, Need::Present)];
    static DEFAULT: Params = Params { tired_ticks: 200, still_visible: false, attackable: None, not_playing_dead: false, hunting_cooldown: None };
    static CREAK: Params = Params { tired_ticks: 200, still_visible: true, attackable: None, not_playing_dead: false, hunting_cooldown: None };

    fn brained(h: &mut Herd, params: &'static Params) -> Entity {
        let entries: &'static [BEntry] = Box::leak(Box::new([BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 1, params, needs: NEEDS, duration: None, kids: &[] }]));
        let def = BrainDef {
            sensors: &[],
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: entries,
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        crate::brain::start(&mut h.world, e);
        e
    }
    fn step(h: &mut Herd, e: Entity) {
        wake(&mut h.world, e);
        brain_step(&mut h.world);
    }
    fn aim(h: &mut Herd, e: Entity, who: Who) {
        set(&mut h.world, e, &ATTACK_TARGET, MemValue::Who(who), None);
        targeting::set_target(&mut h.world, e, Target::new(who, u16::MAX, u8::MAX));
    }
    fn has_target(h: &Herd, e: Entity) -> bool {
        present(&h.world, e, &ATTACK_TARGET)
    }

    /// A target that is still alive stays; once it is gone (despawned), the behaviour clears the memory and the target.
    #[test]
    fn it_stops_when_the_target_is_gone() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h, &DEFAULT);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let prey = spawn_species(&mut h.world, &sp, 3.5, 0.5, 92);
        aim(&mut h, e, Who::Entity(prey));
        step(&mut h, e);
        assert!(has_target(&h, e), "témoin : une cible vivante reste");
        h.world.despawn(prey);
        step(&mut h, e);
        assert!(!has_target(&h, e), "une cible disparue est oubliée");
        assert!(h.world.get::<Target>(e).is_none(), "et le ciblage la lâche");
    }

    /// The creaking keeps its player while he is in the visible targetable list; out of it, the target is dropped.
    #[test]
    fn a_creaking_stops_when_its_player_leaves_the_visible_list() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h, &CREAK);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 4.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        aim(&mut h, e, Who::Player(1));
        set(&mut h.world, e, &NEAREST_VISIBLE_TARGETABLE_PLAYERS, MemValue::Whos(vec![Who::Player(1)]), None);
        step(&mut h, e);
        assert!(has_target(&h, e), "témoin : le joueur est dans la liste visible, la cible reste");
        set(&mut h.world, e, &NEAREST_VISIBLE_TARGETABLE_PLAYERS, MemValue::Whos(vec![Who::Player(2)]), None);
        step(&mut h, e);
        assert!(!has_target(&h, e), "hors de la liste visible, la cible est lâchée");
    }

    /// A target unreachable for more than 200 ticks is dropped; at exactly 200 it is kept.
    #[test]
    fn it_stops_a_target_it_could_not_reach_for_over_200_ticks() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h, &DEFAULT);
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let prey = spawn_species(&mut h.world, &sp, 3.5, 0.5, 92);
        aim(&mut h, e, Who::Entity(prey));
        let now = now_of(&h.world, e) as i64;
        set(&mut h.world, e, &CANT_REACH_WALK_TARGET_SINCE, MemValue::Int(now - 200), None);
        step(&mut h, e);
        assert!(has_target(&h, e), "témoin : 200 ticks exactement, la cible reste");
        set(&mut h.world, e, &CANT_REACH_WALK_TARGET_SINCE, MemValue::Int(now - 201), None);
        step(&mut h, e);
        assert!(!has_target(&h, e), "plus de 200 ticks sans atteindre : la cible est lâchée");
    }

    /// A creative player is no valid target any more: the behaviour drops him; a survival player is kept.
    #[test]
    fn it_stops_on_a_creative_player() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h, &DEFAULT);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 4.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        aim(&mut h, e, Who::Player(1));
        step(&mut h, e);
        assert!(has_target(&h, e), "témoin : un joueur en survie reste cible");
        h.world.resource_mut::<PlayerState>().set_creative(1, true);
        step(&mut h, e);
        assert!(!has_target(&h, e), "un joueur passé en créatif n'est plus une cible");
    }
}
