//! The spear use of a mob against a real `Mob` in the same scene (`HORIZON_TEST_RESOURCES`, process species with a melee goal
//! carrying `spear_use` on the ECS side): with a wooden spear in the main hand and the player as the target, the approach, the
//! tick the charge starts, the stab's damage and its tick. The mounted case is not in this scene (see the notes).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::{
    items::ItemStack,
    mobs::{spear_use::is_using_item, MobEvent, Who as MWho},
};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::spear_use::{self, Phase, Spear},
    cow::*,
    goals::*,
    hostile::PlayerState,
    targeting::*,
};

/// The player 8 blocks east of the mob, standing still and immortal (its health is read, not spent).
const PLAYER: Vec3 = Vec3 { x: 8.5, y: 1.0, z: 0.5 };
const MOB: Vec3 = Vec3 { x: 0.5, y: 1.0, z: 0.5 };

fn spear() -> ItemStack {
    ItemStack { item: "minecraft:wooden_spear".into(), count: 1, damage: 0, extra: None }
}

#[test]
fn a_mob_with_a_spear_charges_and_stabs_its_player_target_as_a_mob_does() {
    let (mut r, mut h) = flat_pair();
    let i = r.add(MOB);
    r.mobs[i].extra.set_main_hand(Some("minecraft:wooden_spear".into()));
    r.mobs[i].ai.target = Some(MWho::Player);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let e = spawn_species(&mut h.world, &sp, MOB.x, MOB.z, 93);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: PLAYER.x, y: PLAYER.y, z: PLAYER.z, main: 0, off: 0 });
    h.world.resource_mut::<PlayerState>().immortal = true;
    set_main_hand(&mut h.world, e, Some(spear()));
    set_target(&mut h.world, e, Target::player(1, 30));
    let view = player_view(PLAYER, None);

    let (mut m_charge, mut e_charge): (Option<i64>, Option<i64>) = (None, None);
    let (mut m_at, mut e_at): (Option<f64>, Option<f64>) = (None, None);
    let (mut m_hit, mut e_hit): (Option<(i64, f32)>, Option<(i64, f32)>) = (None, None);
    let mut hits_seen = 0usize;
    for t in 1..=300i64 {
        let from = r.events.len();
        r.step(Some(&view));
        if m_charge.is_none() && is_using_item(&r.mobs[i]) {
            m_charge = Some(t);
            m_at = Some(xz(r.mobs[i].position, PLAYER));
        }
        for ev in &r.events[from..] {
            if let MobEvent::DamagePlayer { amount, .. } = ev {
                m_hit.get_or_insert((t, *amount));
            }
        }
        h.tick();
        if e_charge.is_none() && h.world.get::<Spear>(e).is_some_and(|s| s.phase == Phase::Charge) {
            e_charge = Some(t);
            e_at = Some(xz(h.world.get::<Pos>(e).unwrap().0, PLAYER));
        }
        let next = h.world.resource::<PlayerState>().hits.get(hits_seen).map(|hit| hit.amount);
        if let Some(amount) = next {
            e_hit.get_or_insert((t, amount));
            hits_seen += 1;
        }
        if m_hit.is_some() && e_hit.is_some() {
            break;
        }
    }
    let (m_charge, e_charge) = (m_charge.expect("the Mob never started its spear"), e_charge.expect("the ECS never started its spear"));
    let ((m_t, m_amt), (e_t, e_amt)) = (m_hit.expect("the Mob never stabbed"), e_hit.expect("the ECS never stabbed"));
    eprintln!("charge: mob {m_charge}, ecs {e_charge}; approach: mob {m_at:?}, ecs {e_at:?}; stab: mob {m_t} {m_amt}, ecs {e_t} {e_amt}");
    // The approach: the charge starts at about the same tick, from about the same distance (the approach distance is 10).
    assert!((m_charge - e_charge).abs() <= 3, "charge: mob {m_charge}, ecs {e_charge}");
    let (m_at, e_at) = (m_at.unwrap(), e_at.unwrap());
    assert!(m_at <= 10.5 && e_at <= 10.5, "the approach ends within the distance: mob {m_at:.2}, ecs {e_at:.2}");
    assert!((m_at - e_at).abs() <= 1.0, "the approach: mob {m_at:.2}, ecs {e_at:.2}");
    // The stab: the damage is the attack's (no speed: a still target and a still mob give no bonus) and it comes within a few
    // ticks of the charge start on both sides.
    assert!((m_amt - e_amt).abs() < 0.01, "damage: mob {m_amt}, ecs {e_amt}");
    assert!(((m_t - m_charge) - (e_t - e_charge)).abs() <= 3, "stab after the charge: mob {}, ecs {}", m_t - m_charge, e_t - e_charge);
}
