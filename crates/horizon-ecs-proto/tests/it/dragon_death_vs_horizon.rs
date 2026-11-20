//! The dragon's death against Horizon's: two hundred ticks of rising, the experience given (total value per tick), the removal, then
//! the exit portal, the gateway, the egg and the fight's flags. Golden: `dragon_golden/death.txt`. The orbs' own motion and merging
//! are not compared (they draw from another generator, `ecs-ecarts.md`): the value they carry is.
use crate::dragon_support::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::death::DeathTime;
use horizon_ecs_proto::dragon::fight::{EndFight, FightEvents};
use horizon_ecs_proto::orb::Orb;

pub const GATEWAYS: [i32; 2] = [3, 7];

pub fn gateway_of(g: i32) -> [i32; 3] {
    let a = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * f64::from(g));
    [(96.0 * a.cos()).floor() as i32, 75, (96.0 * a.sin()).floor() as i32]
}

pub fn orbs(h: &mut Herd) -> i64 {
    let w = &mut h.world;
    w.query::<&Orb>().iter(w).map(|o| i64::from(o.value) * i64::from(o.count)).sum()
}

/// 200 ticks of a dragon that is not the fight's (500 experience, nothing after), then the fight's own dragon: the portal, the gateway,
/// the egg and the flags (its experience, 12 000, is not in the golden, which dies out of the End).
fn die(fight: bool) -> (Herd, Vec<String>) {
    let mut h = herd_of(100);
    h.world.insert_resource(EndFight { gateways: GATEWAYS.to_vec(), ..Default::default() });
    h.world.init_resource::<FightEvents>();
    let e = dragon(&mut h, 10, 0.0);
    if fight {
        h.world.resource_mut::<EndFight>().dragon = Some(e);
    }
    let mut out = Vec::new();
    for t in 1..=200 {
        h.tick();
        out.push(match h.world.get_entity(e) {
            Ok(_) => format!("death {t} {} dt{} orbs{}", bits(h.world.get::<Pos>(e).unwrap().0), h.world.get::<DeathTime>(e).map_or(0, |d| d.0), orbs(&mut h)),
            Err(_) => format!("death {t} gone orbs{}", orbs(&mut h)),
        });
    }
    (h, out)
}

#[test]
fn the_death_matches_horizon() {
    if !crate::common::have_data() {
        return;
    }
    let (mut h, mut out) = die(false);
    assert!(!h.world.resource::<EndFight>().killed, "a dragon out of the fight ends nothing");
    let (mut hf, _) = die(true);
    hf.tick();
    hf.tick();
    out.extend(blocks(&hf, gateway_of(GATEWAYS[1])));
    let f = hf.world.resource::<EndFight>();
    out.push(format!("flags killed{} previously{} gateways{:?}", f.killed, f.previously_killed, f.gateways));
    h.tick();
    compare("death", out);
}

#[test]
fn the_fights_first_kill_gives_12000_and_a_later_one_500() {
    if !crate::common::have_data() {
        return;
    }
    let (mut h, _) = die(true);
    let first = orbs(&mut h);
    let mut h2 = herd();
    h2.world.insert_resource(EndFight { previously_killed: true, ..Default::default() });
    h2.world.init_resource::<FightEvents>();
    let e = dragon(&mut h2, 10, 0.0);
    h2.world.resource_mut::<EndFight>().dragon = Some(e);
    for _ in 0..200 {
        h2.tick();
    }
    assert_eq!((first, orbs(&mut h2)), (12000 - 0, 500));
}
