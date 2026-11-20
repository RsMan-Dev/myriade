//! `GoAndGiveItemsToTarget(speed, range)` (the allay's `give_to_owner`, `Mob` l.~365-380): an animal that holds an item (its `Pocket`)
//! flies to its owner while the owner is within `range` blocks, online and not a spectator. It has no stop distance: it flies to the
//! player while the pocket is full; the throw at arrival (a brain step) and the emptying are outside this module.
//! What wakes it: the owner entering or leaving the radius (`PlayerWatch`), and `pocket_changed` when the pocket is filled.
use super::*;
use crate::{goals::*, hostile::PlayerState, pocket::Pocket, tame};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    pub range: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 2.25, range: 64.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Giving {
    p: &'static Params,
    slot: u8,
    recalc: i32,
    /// The tick the goal started in: its `tick` runs before any `canContinueToUse`.
    fresh: bool,
}
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Carrying;

pub static MODULE: Module = Module {
    name: "give_to_owner",
    flags: MOVE,
    retry_on_end: true,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Tamable, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Giving { p, slot: inst.slot, recalc: 0, fresh: false });
    if w.get::<Pocket>(e).is_none() {
        w.entity_mut(e).insert(Pocket::default());
    }
    if w.get::<PlayerWatch>(e).is_none() {
        w.entity_mut(e).insert(PlayerWatch::new(p.range.min(64.0) as f32));
    }
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    w.resource_mut::<Requests>().0.push(Req::Start(e, inst.slot));
}
/// The pocket was filled or emptied by the code that holds it: the goal is asked again.
pub fn pocket_changed(w: &mut World, e: Entity) {
    crate::goals::wake(w, e, &MODULE);
}
fn tick(w: &mut World) {
    tick_state(w, owner_moved);
    tick_state(w, give_run);
}
/// The owner came within the radius or left it: the goal is asked at the next tick of the selector.
fn owner_moved(ev: Res<Events>, q: Query<(&tame::Owner, &Giving)>, mut reqs: ResMut<Requests>) {
    let moved = ev.player_entered.iter().chain(ev.player_left.iter());
    for (e, id) in moved {
        if let Ok((o, g)) = q.get(e)
            && o.0 == Some(crate::targeting::Who::Player(id))
        {
            reqs.0.push(Req::Start(e, g.slot));
        }
    }
}
/// `canUse`: a held item, an owner online, not a spectator, within `range` blocks.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    can_use_at(w, e)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let mut g = w.get_mut::<Giving>(e).unwrap();
    (g.recalc, g.fresh) = (0, true);
    w.entity_mut(e).insert(Carrying);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Carrying>();
    stop_nav(w, e);
}
fn give_run(q: Query<Entity, With<Carrying>>, mut reqs: ResMut<Requests>) {
    for e in &q {
        reqs.0.push(Req::Call(e, give_step));
    }
}
/// One tick of the goal of one animal: it ends when `canUse` is false (pocket emptied, owner gone or too far), else it makes a path to
/// the owner every 10 ticks.
fn give_step(world: &mut World, e: Entity) {
    let Some(g) = world.get::<Giving>(e).copied() else { return };
    if !g.fresh && !can_use_at(world, e) {
        world.resource_mut::<Requests>().0.push(Req::End(e, g.slot));
        return;
    }
    let Some(owner) = tame::owner_player(world, e) else { return };
    let mut st = world.get_mut::<Giving>(e).unwrap();
    st.fresh = false;
    st.recalc -= 1;
    if st.recalc <= 0 {
        st.recalc = 10;
        go_to_at(world, e, [owner.x, owner.y, owner.z], g.p.speed, 0);
    }
}
/// The same test as `can_use`, for the running goal (its parameters are read from its state).
fn can_use_at(w: &mut World, e: Entity) -> bool {
    let Some(g) = w.get::<Giving>(e).map(|g| g.p) else { return false };
    if w.get::<Pocket>(e).is_none_or(|p| p.held().is_none()) {
        return false;
    }
    let Some(owner) = tame::owner_player(w, e) else { return false };
    if w.resource::<PlayerState>().spectator(owner.id) {
        return false;
    }
    tame::owner_distance_sqr(w, e).is_some_and(|d| d <= g.range * g.range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Pos};
    use crate::species::Config;
    use horizon_client_lib::items::ItemStack;

    /// `n` creatures whose only goal is this one, owned by player 1, who stands `dx` blocks away.
    fn world(n: usize, dx: f64) -> (Herd, Vec<Entity>) {
        let mut h = Herd::with_slab(60, true, false);
        let mut sp = *species();
        let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 2, &Params::DEFAULT)].into_boxed_slice());
        sp.goals = Config { entries };
        let es: Vec<Entity> = (0..n).map(|i| spawn_species(&mut h.world, &sp, 0.5, 0.5, 80 + i as u64)).collect();
        for &e in &es {
            tame::set_owner(&mut h.world, e, crate::targeting::Who::Player(1));
        }
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + dx, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        (h, es)
    }
    /// Fills the pocket of a creature with one cod stack and asks the goal again.
    fn fill(h: &mut Herd, e: Entity) {
        h.world.get_mut::<Pocket>(e).unwrap().0 = Some(ItemStack::new("minecraft:cod", 1));
        pocket_changed(&mut h.world, e);
    }
    /// The horizontal distance in blocks between the creature and its owner.
    fn dist(h: &Herd, e: Entity) -> f64 {
        let p = h.world.get::<Pos>(e).unwrap().0;
        let o = h.world.resource::<Online>().get(1).unwrap();
        ((o.x - p.x).powi(2) + (o.z - p.z).powi(2)).sqrt()
    }
    #[test]
    fn a_full_pocket_flies_to_the_owner_and_an_empty_one_does_not() {
        if crate::cow::skip_unless_walker("give to owner") || crate::cow::skip_unless_caps(&[Cap::Tamable], "give to owner") {
            return;
        }
        let (mut h, es) = world(2, 30.0);
        let (full, empty) = (es[0], es[1]);
        let (full0, empty0) = (dist(&h, full), dist(&h, empty));
        fill(&mut h, full);
        for _ in 0..100 {
            h.tick();
        }
        assert!(dist(&h, full) <= full0 - 5.0, "full: {full0} to {}", dist(&h, full));
        assert!(dist(&h, empty) >= empty0 - 0.5, "empty: {empty0} to {}", dist(&h, empty));
    }
    #[test]
    fn an_owner_beyond_64_blocks_or_a_spectator_is_not_served() {
        if crate::cow::skip_unless_walker("give to owner") || crate::cow::skip_unless_caps(&[Cap::Tamable], "give to owner") {
            return;
        }
        let (mut h, es) = world(1, 80.0);
        fill(&mut h, es[0]);
        for _ in 0..20 {
            h.tick();
        }
        assert!(h.world.get::<Carrying>(es[0]).is_none(), "owner at 80 blocks");
        let (mut h, es) = world(1, 30.0);
        h.world.resource_mut::<PlayerState>().spectator = vec![false, true];
        fill(&mut h, es[0]);
        for _ in 0..20 {
            h.tick();
        }
        assert!(h.world.get::<Carrying>(es[0]).is_none(), "spectator owner");
    }
}
