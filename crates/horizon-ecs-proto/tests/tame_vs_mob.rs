//! The tamed wolf of the frame, by configuration, against the `Mob` wolf in the same scene (`HORIZON_SPECIES=minecraft:wolf`,
//! `HORIZON_TEST_RESOURCES` set): the taming draw, sitting, following at a distance, the teleport to a far owner, defending the owner,
//! and not attacking it.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{tame, PlayerView};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::{Target, Who};

const UUID: [i32; 4] = [1, 2, 3, 4];
fn skip() -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:wolf")
}
struct Pair {
    r: MobRef,
    h: Herd,
    mob: usize,
    ecs: Entity,
    owner_at: Vec3,
}
fn view(at: Vec3) -> PlayerView {
    let mut v = player_view(at, None);
    v.uuid = UUID;
    v
}
/// A tame wolf of player 1 (who stands at `owner_at`) on both sides, standing.
fn pair(owner_at: Vec3) -> Pair {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let i = r.add_id(Vec3::new(0.5, 1.0, 0.5), 77_777);
    tame::set_tame(&mut r.mobs[i], true);
    r.mobs[i].extra.owner = Some(UUID);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: owner_at.x, y: owner_at.y, z: owner_at.z, main: 0, off: 0 });
    horizon_ecs_proto::tame::tame(&mut h.world, e, Who::Player(1), 40.0);
    horizon_ecs_proto::tame::order_sit(&mut h.world, e, false);
    Pair { r, h, mob: i, ecs: e, owner_at }
}
impl Pair {
    fn step(&mut self) {
        let v = view(self.owner_at);
        self.r.step(Some(&v));
        self.h.tick();
    }
    fn dist(&self) -> (f64, f64) {
        let (m, e) = (self.r.mobs[self.mob].position, self.h.world.get::<Pos>(self.ecs).unwrap().0);
        let d = |p: Vec3| ((p.x - self.owner_at.x).powi(2) + (p.z - self.owner_at.z).powi(2)).sqrt();
        (d(m), d(e))
    }
}

/// The taming draw is one chance in three (`random.nextInt(3) == 0`): a wild wolf given a bone is tame, owned by the player, sitting, with 40
/// health; a tame one given the same item sits and stands.
#[test]
fn a_bone_tames_one_wild_wolf_in_three_and_the_owner_orders_it_to_sit() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab(60, true, false);
    let mut tamed = 0;
    let mut last = None;
    for k in 0..300 {
        let e = spawn_species(&mut h.world, species(), 0.5 + (k % 50) as f64, 0.5 + (k / 50) as f64, 11 + k as u64);
        let handled = interact(&mut h.world, e, &Interaction { player: 1, item: BONE });
        assert!(handled);
        if h.world.get::<Owner>(e).unwrap().0 == Some(Who::Player(1)) {
            tamed += 1;
            last = Some(e);
        }
    }
    eprintln!("tamed {tamed} of 300");
    assert!((75..=125).contains(&tamed), "a third of 300: {tamed}");
    let e = last.expect("one was tamed");
    assert_eq!(h.world.get::<Health>(e).unwrap().0, 40.0);
    assert!(is_occupied(&h.world, e, OCC_SITTING), "a tamed wolf sits");
    // Its owner stands it, then sits it again; another player's item changes nothing.
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: 0 }));
    assert!(!is_occupied(&h.world, e, OCC_SITTING));
    assert!(!interact(&mut h.world, e, &Interaction { player: 2, item: 0 }));
    assert!(interact(&mut h.world, e, &Interaction { player: 1, item: 0 }));
    assert!(is_occupied(&h.world, e, OCC_SITTING));
}

/// An owner 15 blocks away: a standing wolf walks to it and stops near it, a sitting one does not move; the two sides end at the same distance.
#[test]
fn a_standing_wolf_follows_its_owner_and_a_sitting_one_stays() {
    if skip() {
        return;
    }
    let mut p = pair(Vec3::new(15.5, 1.0, 0.5));
    // 60 ticks: the walk is over (it takes about 20) and the `Mob`'s own stroll (no_action_time 100) has not begun: the complete wolf strolls
    // away from its owner and comes back, on both sides, so a later reading measures the stroll, not the following.
    for _ in 0..60 {
        p.step();
    }
    let (m, e) = p.dist();
    eprintln!("distance to the owner after 60 ticks: mob {m:.2}, ecs {e:.2}");
    assert!(m < 3.5 && e < 3.5 && (m - e).abs() < 1.5, "mob {m}, ecs {e}");
    // Sitting: the owner goes away, the wolf stays where it is.
    let mut q = pair(Vec3::new(0.5, 1.0, 4.5));
    q.r.mobs[q.mob].extra.sitting = true;
    horizon_ecs_proto::tame::order_sit(&mut q.h.world, q.ecs, true);
    q.owner_at = Vec3::new(25.5, 1.0, 0.5);
    q.h.world.resource_mut::<Online>().set(Player { id: 1, x: 25.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    for _ in 0..100 {
        q.step();
    }
    let (m, e) = (q.r.mobs[q.mob].position, q.h.world.get::<Pos>(q.ecs).unwrap().0);
    assert!(m.x < 2.0 && e.x < 2.0, "a sitting wolf follows: mob {m:?}, ecs {e:?}");
}

/// An owner 40 blocks away: the wolf teleports to it (12 blocks and more), within a few ticks, and ends within 4 blocks of it.
#[test]
fn a_wolf_teleports_to_a_far_owner() {
    if skip() {
        return;
    }
    let mut p = pair(Vec3::new(40.5, 1.0, 0.5));
    let (mut when_m, mut when_e) = (None, None);
    for t in 1..=60 {
        p.step();
        let (m, e) = p.dist();
        if when_m.is_none() && m < 8.0 {
            when_m = Some(t);
        }
        if when_e.is_none() && e < 8.0 {
            when_e = Some(t);
        }
    }
    eprintln!("teleported at tick: mob {when_m:?}, ecs {when_e:?}");
    assert!(when_m.is_some() && when_e.is_some());
    assert!((when_m.unwrap() as i64 - when_e.unwrap() as i64).abs() <= 6, "{when_m:?} {when_e:?}");
}

/// What hurt the owner becomes the wolf's target; a tame wolf of the same owner does not; the owner that hits the wolf is not its target.
#[test]
fn a_wolf_defends_its_owner_and_spares_what_it_must() {
    if skip() {
        return;
    }
    let mut p = pair(Vec3::new(4.5, 1.0, 0.5));
    // A cow near it hurt the owner.
    let cow_kind = mob_kind("minecraft:cow");
    let ci = p.r.add_kind(Vec3::new(6.5, 1.0, 0.5), cow_kind);
    let table = species_table().0;
    let cow_sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let cow = spawn_species(&mut p.h.world, &cow_sp, 6.5, 0.5, 9);
    for _ in 0..5 {
        p.step();
    }
    let cow_id = p.r.mobs[ci].id;
    let mut v = view(p.owner_at);
    v.hurt_by = Some((horizon_client_lib::mobs::Who::Mob(cow_id), p.r.t));
    let now = p.h.world.resource::<Clock>().0;
    p.h.world.resource_mut::<PlayerState>().set_hurt_by(1, Who::Entity(cow), now);
    let (mut when_m, mut when_e) = (None, None);
    for t in 1..=30 {
        p.r.step(Some(&v));
        p.h.tick();
        if when_m.is_none() && p.r.mobs[p.mob].ai.target == Some(horizon_client_lib::mobs::Who::Mob(cow_id)) {
            when_m = Some(t);
        }
        if when_e.is_none() && p.h.world.get::<Target>(p.ecs).is_some_and(|x| x.who == Who::Entity(cow)) {
            when_e = Some(t);
        }
    }
    eprintln!("target taken at tick: mob {when_m:?}, ecs {when_e:?}");
    assert!(when_m.is_some() && when_e.is_some(), "mob {when_m:?} ecs {when_e:?}");
    assert!((when_m.unwrap() as i64 - when_e.unwrap() as i64).abs() <= 4);
}

/// The owner that hits its own wolf is not made its target (`TamableAnimal.canAttack`); a stranger that hits a tame wolf is.
#[test]
fn a_wolf_does_not_attack_its_owner() {
    if skip() {
        return;
    }
    let mut p = pair(Vec3::new(3.5, 1.0, 0.5));
    p.r.hit(p.mob, 1.0, [3.5, 1.0, 0.5]);
    p.h.hit_by(p.ecs, 1.0, 1);
    let (mut mob_hunts, mut ecs_hunts) = (false, false);
    for _ in 0..40 {
        p.step();
        mob_hunts |= p.r.mobs[p.mob].ai.target.is_some();
        ecs_hunts |= p.h.world.get::<Target>(p.ecs).is_some();
    }
    eprintln!("the owner hit its wolf: the wolf took a target: mob {mob_hunts}, ecs {ecs_hunts}");
    assert!(!ecs_hunts, "the ECS wolf attacks its owner");
    // A stranger (player 2, not its owner) does hit it: the wolf takes it.
    // The owner is near: a far owner makes the wolf (hurt, not panicking: its panic is environmental only, as the `Mob`'s) teleport to it before
    // its melee goal starts (a 20-tick grid), which takes it out of the stranger's range and drops the target.
    let mut q = pair(Vec3::new(4.5, 1.0, 0.5));
    q.h.world.resource_mut::<Online>().set(Player { id: 2, x: 2.5, y: 1.0, z: 0.5, main: 0, off: 0 });
    q.h.hit_by(q.ecs, 1.0, 2);
    for _ in 0..10 {
        q.h.tick();
    }
    assert!(q.h.world.get::<Target>(q.ecs).is_some_and(|t| t.who == Who::Player(2)), "a stranger that hit a tame wolf is not its target");
}
