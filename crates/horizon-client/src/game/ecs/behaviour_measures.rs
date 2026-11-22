//! The behaviour measures of one run and their aggregation over the repetitions, defined as `tools/vanilla-probe/Measures.java`.
pub(super) const X: usize = 0;
pub(super) const Y: usize = 1;
pub(super) const Z: usize = 2;
pub(super) const VX: usize = 3;
pub(super) const VY: usize = 4;
pub(super) const VZ: usize = 5;
pub(super) const BODY: usize = 6;
pub(super) const HEALTH: usize = 7;
pub(super) const ALIVE: usize = 8;
pub(super) const TARGET: usize = 9;
pub(super) const HEAD: usize = 11;
const MOVING: f64 = 0.001;
const PLAYER: (f64, f64) = (4.5, 0.5);

/// Position relative to the parcel, per-tick displacement, body yaw, health, alive, target, yaw, head yaw.
pub(super) type Row = [f64; 12];
pub(super) type Measures = Vec<(&'static str, Option<f64>)>;

fn wrap(d: f64) -> f64 {
    let r = d.rem_euclid(360.0);
    r.min(360.0 - r)
}

fn turn(rows: &[Row], i: usize) -> f64 {
    rows.windows(2).map(|w| wrap(w[1][i] - w[0][i])).sum()
}

fn hdist(r: &Row) -> f64 {
    (r[X] - PLAYER.0).hypot(r[Z] - PLAYER.1)
}

pub(super) fn measures(rows: &[Row], with_player: bool) -> Measures {
    let (first, last) = (&rows[0], &rows[rows.len() - 1]);
    let ticks = &rows[1..];
    let n = ticks.len() as f64;
    let speed: Vec<f64> = ticks.iter().map(|r| r[VX].hypot(r[VZ])).collect();
    let moving: Vec<f64> = speed.iter().copied().filter(|s| *s > MOVING).collect();
    let mean_moving = if moving.is_empty() { 0.0 } else { moving.iter().sum::<f64>() / moving.len() as f64 };
    let lo = rows.iter().map(|r| r[Y]).fold(f64::INFINITY, f64::min);
    let hi = rows.iter().map(|r| r[Y]).fold(f64::NEG_INFINITY, f64::max);
    let path: f64 = rows.windows(2).map(|w| (w[1][X] - w[0][X]).hypot(w[1][Z] - w[0][Z])).sum();
    let hops = rows.windows(2).filter(|w| w[0][VY] <= 0.0 && w[1][VY] > 0.1).count() as f64;
    let first_target = ticks.iter().position(|r| r[TARGET] == 1.0).map(|i| i as f64 + 1.0);
    let target_ticks = ticks.iter().filter(|r| r[TARGET] == 1.0).count() as f64;
    let mut m: Measures = vec![
        ("path", Some(path)),
        ("net", Some((last[X] - first[X]).hypot(last[Z] - first[Z]))),
        ("moving", Some(moving.len() as f64 / n)),
        ("speed", Some(mean_moving)),
        ("top_speed", Some(speed.iter().copied().fold(0.0, f64::max))),
        ("y_range", Some(hi - lo)),
        ("hops", Some(hops)),
        ("head_turn", Some(turn(rows, HEAD))),
        ("body_turn", Some(turn(rows, BODY))),
        ("target_ever", Some(f64::from(u8::from(first_target.is_some())))),
        ("target_ticks", Some(target_ticks / n)),
        ("target_first", first_target),
        ("health_end", Some(last[HEALTH])),
        ("alive_end", Some(last[ALIVE])),
    ];
    if with_player {
        m.push(("player_dist_end", Some(hdist(last))));
        m.push(("player_dist_min", Some(rows.iter().map(hdist).fold(f64::INFINITY, f64::min))));
    }
    m
}

/// One TSV line per measure, `species scenario metric mean min max n`, over the runs that have the value.
pub(super) fn aggregate(species: &str, scenario: &str, runs: &[Measures]) -> String {
    let Some(first) = runs.first() else { return String::new() };
    let mut out = String::new();
    for (k, (name, _)) in first.iter().enumerate() {
        let v: Vec<f64> = runs.iter().filter_map(|r| r[k].1).collect();
        out.push_str(&format!("{species}\t{scenario}\t{name}\t"));
        if v.is_empty() {
            out.push_str("-\t-\t-\t0\n");
            continue;
        }
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let (mn, mx) = (v.iter().copied().fold(f64::INFINITY, f64::min), v.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        out.push_str(&format!("{mean}\t{mn}\t{mx}\t{}\n", v.len()));
    }
    out
}
