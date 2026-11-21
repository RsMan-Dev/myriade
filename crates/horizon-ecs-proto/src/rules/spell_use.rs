use horizon_core::geometry::{trig, Vec3};

/// The ticks until the next ask while a wait of `wait` ticks lasts: the 2-tick phase, at least 2.
pub fn recheck_in(wait: u64) -> i32 {
    (wait.div_ceil(2) * 2).max(2) as i32
}

/// A sheep is in the wololo box: within 16 blocks sideways and 4 vertically (`d2` is the squared distance).
pub fn near_sheep(dx: f64, dy: f64, dz: f64, d2: f64) -> bool {
    dx.abs() <= 16.0 && dy.abs() <= 4.0 && dz.abs() <= 16.0 && d2 <= 256.0
}

/// The spot of one summoned vex: the caster's block shifted by `dx` and `dz`, a block up.
pub fn summon_spot(me: Vec3, dx: i32, dz: i32) -> Vec3 {
    Vec3::new(me.x.floor() + f64::from(dx) + 0.5, me.y.floor() + 1.0, me.z.floor() + f64::from(dz) + 0.5)
}

/// The life of a vex in ticks, from a draw `draw` of 0 to 89: 30 to 120 seconds.
pub fn vex_life(draw: i32) -> i32 {
    20 * (30 + draw)
}

/// The fangs of a cast: a ring of 5 and a ring of 8 (delay 3) when `near`, else a line of 16 toward `target`.
pub fn fang_spots(me: Vec3, target: Vec3, near: bool) -> Vec<(f64, f64, i32)> {
    let angle = (target.z - me.z).atan2(target.x - me.x) as f32;
    let cos = |a: f32| f64::from(trig::cos(f64::from(a)));
    let sin = |a: f32| f64::from(trig::sin(f64::from(a)));
    let mut spots: Vec<(f64, f64, i32)> = Vec::new();
    if near {
        for i in 0..5 {
            let a = angle + i as f32 * std::f32::consts::PI * 0.4;
            spots.push((me.x + cos(a) * 1.5, me.z + sin(a) * 1.5, 0));
        }
        for i in 0..8 {
            let a = angle + i as f32 * std::f32::consts::PI * 2.0 / 8.0 + std::f32::consts::PI * 2.0 / 5.0;
            spots.push((me.x + cos(a) * 2.5, me.z + sin(a) * 2.5, 3));
        }
    } else {
        for i in 0..16 {
            let reach = 1.25 * f64::from(i + 1);
            spots.push((me.x + cos(angle) * reach, me.z + sin(angle) * reach, i));
        }
    }
    spots
}
