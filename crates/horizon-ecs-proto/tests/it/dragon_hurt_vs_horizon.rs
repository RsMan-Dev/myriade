//! The hurts of the dragon against Horizon's (`hurt_dragon_part`, `hurt_from`, the clocks of `tick_dragon`): every phase takes the same
//! blows, the perch's damage takes it off, the killing blow, and the clocks run down with the ticks. Golden: `dragon_golden/hurt.txt`.
use crate::dragon_support::*;
use horizon_ecs_proto::dragon::hurt::DragonHurt;

const BLOWS: [(usize, f32, char); 8] = [(0, 5.0, 'K'), (3, 9.0, 'K'), (0, 12.0, 'K'), (0, 5.0, 'A'), (0, 0.001, 'K'), (0, 4.0, 'F'), (0, 4.0, 'M'), (2, 30.0, 'E')];

#[test]
fn hurts_by_phase_and_clocks_match_horizon() {
    if !crate::common::have_data() {
        return;
    }
    let mut out = Vec::new();
    let mut h = herd();
    for phase in 0..=10u8 {
        let e = dragon(&mut h, phase, 200.0);
        for (i, (part, dmg, k)) in BLOWS.into_iter().enumerate() {
            let r = blow(&mut h, e, part, dmg, k);
            out.push(format!("hurt {phase} {i} {} {}", show(r), snap(&h, e)));
        }
        h.world.despawn(e);
        let e = dragon(&mut h, phase, 200.0);
        for (i, dmg) in [40.0_f32, 20.0].into_iter().enumerate() {
            let r = blow(&mut h, e, 0, dmg, 'K');
            out.push(format!("perch {phase} {i} {} {}", show(r), snap(&h, e)));
            let last = h.world.get::<DragonHurt>(e).copied().unwrap_or_default().last;
            h.world.entity_mut(e).insert(DragonHurt { window: 0, time: 0, last });
        }
        h.world.despawn(e);
        let e = dragon(&mut h, phase, 3.0);
        let r = blow(&mut h, e, 0, 9.0, 'K');
        out.push(format!("kill {phase} 0 {} {}", show(r), snap(&h, e)));
        h.world.despawn(e);
    }
    let e = dragon(&mut h, 0, 200.0);
    let r = blow(&mut h, e, 0, 5.0, 'K');
    out.push(format!("clock 0 {} {}", show(r), snap(&h, e)));
    for t in 1..=24 {
        h.tick();
        out.push(format!("clock {t} - {}", snap(&h, e)));
        if t == 12 || t == 24 {
            let r = blow(&mut h, e, 0, 5.0, 'K');
            out.push(format!("clock {t} {} {}", show(r), snap(&h, e)));
        }
    }
    compare("hurt", out);
}
