//! Règles du lot 3 A, reprises des tests d'origine de `horizon-client` (valeurs vanilla 26.3 vérifiées au jar) : la portée de balade de
//! l'allay, le halo du calmar lumineux, les sons de gonflement du poisson-globe, l'absence de cri du poisson-globe, la portée d'accouplement
//! des vaches et le tirage de taille des cubes.
use common::*;
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::behaviours::breed::{Breeding, Love};
use horizon_ecs_proto::behaviours::glow_particles::DarkTicks;
use horizon_ecs_proto::behaviours::pufferfish_puff::set_puff;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::bridge::host_state::no_action_time;
use horizon_ecs_proto::despawn::{admit, rate_light, DespawnDef};
use horizon_ecs_proto::locomotion::swim::{Dry, SwimParams};
use horizon_ecs_proto::rules::spawn_draw::{roll, Ctx, Roll};
use horizon_ecs_proto::spots::{pick_for, Pick};
use horizon_world::dimension_type::DimensionFacts;

/// Les sons de l'événement d'hôte, dans l'ordre.
fn sounds(events: &[HostEvent]) -> Vec<std::borrow::Cow<'static, str>> {
    events.iter().filter_map(|ev| if let HostEvent::Sound { name, .. } = ev { Some(name.clone()) } else { None }).collect()
}

/// `AllayAi`'s idle `RandomStroll.fly`: the spot reaches ten blocks out, not eight (`AirAndWaterRandomPos(10, 7, -2)`).
#[test]
fn an_allay_wanders_up_to_ten_blocks_away() {
    if !have_data() || wrong_species(&["minecraft:allay"]) {
        return;
    }
    let mut farthest = 0.0_f64;
    for seed in 0..40 {
        let mut h = Herd::with_slab_awake(60, true, false);
        let e = spawn_species(&mut h.world, species(), 0.5, 0.5, seed);
        // The stroll's first draw is 1 in 60: give each allay 300 ticks.
        for _ in 0..300 {
            h.tick();
            if let Some(s) = h.world.get::<Stroll>(e).filter(|s| s.walking) {
                farthest = farthest.max((s.tx - 0.5).abs().max((s.tz - 0.5).abs()));
                break;
            }
        }
    }
    assert!((9.0..=10.5).contains(&farthest), "an allay wanders to spots up to ten blocks out, not eight: {farthest}");
}

/// `GlowSquid.aiStep`: the dark timer counts down, and one glow particle comes each tick, AI or not.
#[test]
fn a_glow_squid_glows_each_tick_and_counts_its_dark_down() {
    if !have_data() || wrong_species(&["minecraft:glow_squid"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 3);
    h.world.get_mut::<DarkTicks>(e).unwrap().0 = 3;
    h.drain_host_out();
    for _ in 0..10 {
        h.tick();
    }
    let glows = h.drain_host_out().iter().filter(|ev| matches!(ev, HostEvent::Particles { kind: "glow", .. })).count();
    assert_eq!(glows, 10, "one glow a tick");
    assert_eq!(h.world.get::<DarkTicks>(e).unwrap().0, 0, "the dark timer stops at zero");
}

/// `Pufferfish.tick`: `blow_up` as it grows, `blow_out` as it shrinks; a silent puffer makes no sound (its pitch is still drawn).
#[test]
fn the_puffer_blows_up_on_growth_and_out_on_shrink() {
    if !have_data() || wrong_species(&["minecraft:pufferfish"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.drain_host_out();
    set_puff(&mut h.world, e, 1);
    assert_eq!(sounds(&h.drain_host_out()), ["entity.puffer_fish.blow_up"]);
    set_puff(&mut h.world, e, 2);
    assert_eq!(sounds(&h.drain_host_out()), ["entity.puffer_fish.blow_up"]);
    set_puff(&mut h.world, e, 1);
    assert_eq!(sounds(&h.drain_host_out()), ["entity.puffer_fish.blow_out"]);
    h.world.entity_mut(e).insert(horizon_ecs_proto::behaviours::elder_curse::Silent);
    set_puff(&mut h.world, e, 0);
    assert!(sounds(&h.drain_host_out()).is_empty(), "a silent puffer makes no sound");
}

/// The puffer never sings its sting as AI: at size 2 it ticks for 20 ticks and no `sting` sound comes.
#[test]
fn the_puffer_at_size_two_never_stings_by_its_ai() {
    if !have_data() || wrong_species(&["minecraft:pufferfish"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 6);
    set_puff(&mut h.world, e, 2);
    h.drain_host_out();
    for _ in 0..20 {
        h.tick();
    }
    assert!(sounds(&h.drain_host_out()).iter().all(|name| !name.contains("sting")), "no sting by the AI");
}

/// `Cow.mate` range: a mate eight blocks out (dist² 64) is in, one a little further is out; of two, the nearest wins.
#[test]
fn a_cow_in_love_mates_with_the_nearest_partner_within_eight_blocks() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let cow = ecs_cow(&mut h, 0.5, 1.0, 0.5, 1);
    h.world.entity_mut(cow).insert(Love(u64::MAX));
    let far = ecs_cow(&mut h, 7.5, 1.0, 5.5, 2);
    h.world.entity_mut(far).insert(Love(u64::MAX));
    h.tick();
    assert!(h.world.get::<Breeding>(cow).is_none(), "dist² 74 is out of range");
    let out = ecs_cow(&mut h, 8.5, 1.0, 1.1, 3);
    h.world.entity_mut(out).insert(Love(u64::MAX));
    let near = ecs_cow(&mut h, 5.5, 1.0, 5.5, 4);
    h.world.entity_mut(near).insert(Love(u64::MAX));
    h.tick();
    assert_eq!(h.world.get::<Breeding>(cow).map(|b| b.partner), Some(near), "dist² 50 is in, the nearest wins");
}

/// `CubeMob.setSpawnSize`: `1 << nextInt(3)`, one more power when that is under 2 and `nextFloat() < 0.5 * special`.
#[test]
fn a_cube_grows_one_power_on_a_lucky_draw_only_when_the_multiplier_allows() {
    let no_tag = |_: &str| false;
    let draw = Roll::Pow2(&Roll::Grow { bound: 3, below: 2, chance: 0.5 });
    for seed in 0..3000 {
        let mut none = LegacyRandom::new(seed);
        let size = roll(&draw, &Ctx { biome: "minecraft:plains", biome_has: &no_tag, special: 0.0, baby: false }, &mut none);
        let mut check = LegacyRandom::new(seed);
        let scale = check.next_int_bound(3) as u32;
        assert_eq!(size, 1 << scale, "no multiplier, no bump (seed {seed})");
        let mut full = LegacyRandom::new(seed);
        let size = roll(&draw, &Ctx { biome: "minecraft:plains", biome_has: &no_tag, special: 1.0, baby: false }, &mut full);
        let mut expect = LegacyRandom::new(seed);
        let mut scale = expect.next_int_bound(3) as u32;
        if scale < 2 && expect.next_float() < 0.5 {
            scale += 1;
        }
        assert_eq!(size, 1 << scale, "the lucky draw bumps the size (seed {seed})");
    }
}

/// `Cod.getFlopSound` / `Salmon` / `Tadpole`: a fish out of the water hops with its own sound, at a pitch of `makeSound`.
#[test]
fn a_fish_out_of_water_flops_with_its_own_sound() {
    const FLOPS: [(&str, &str); 3] = [("minecraft:cod", "entity.cod.flop"), ("minecraft:salmon", "entity.salmon.flop"), ("minecraft:tadpole", "entity.tadpole.flop")];
    if !have_data() || wrong_species(&FLOPS.map(|f| f.0)) {
        return;
    }
    let want = FLOPS.iter().find(|f| f.0 == species().name).unwrap().1;
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 9);
    assert_eq!(h.world.get::<SwimParams>(e).unwrap().flop, Some(want));
    h.drain_host_out();
    h.world.entity_mut(e).insert(Dry);
    h.world.get_mut::<Contact>(e).unwrap().on_ground = true;
    h.tick();
    let pitch = h.drain_host_out().iter().find_map(|ev| if let HostEvent::Sound { name, pitch, .. } = ev { (*name == want).then_some(*pitch) } else { None });
    let pitch = pitch.expect("the flop sound");
    assert!((0.8..=1.2).contains(&pitch), "the pitch of makeSound: {pitch}");
}

/// A bolting horse, `RunAroundLikeCrazyGoal`: `DefaultRandomPos(5, 4)`, the spot five blocks about and four up or down.
#[test]
fn a_bolting_horse_picks_a_spot_five_blocks_about() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 1);
    let start = h.world.get::<Pos>(e).unwrap().0;
    let mut picked = 0;
    for seed in 0..200u64 {
        h.world.get_mut::<Rng>(e).unwrap().0 = seed;
        let Some(p) = pick_for(&mut h.world, e, Pick::Default { h: 5, v: 4 }) else { continue };
        picked += 1;
        assert!((p[0] - start.x).abs() <= 5.5 && (p[2] - start.z).abs() <= 5.5, "five blocks about (seed {seed}): {p:?}");
        assert!((p[1] - start.y).abs() <= 4.5, "four blocks up or down (seed {seed}): {p:?}");
    }
    assert!(picked > 100, "most of the ten tries find a spot: {picked}");
}

/// `DimensionType.ambientLight` (0 overworld, 0.1 nether, 0.25 end) lifts the dark part of `getLightLevelDependentMagicValue`.
#[test]
fn the_dimension_ambient_light_lifts_the_dark_part_of_the_brightness() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let at = |h: &Herd| h.world.resource::<Level>().mw.0.brightness(0, 2, 0);
    let over = at(&h);
    h.set_dimension(DimensionFacts::of_builtin_level(horizon_core::LevelKey::END));
    let end = at(&h);
    assert!((end - (over + (1.0 - over) * 0.25)).abs() < 1e-6, "the end's ambient light 0.25: {end} from {over}");
    h.set_dimension(DimensionFacts::of_builtin_level(horizon_core::LevelKey::NETHER));
    let nether = at(&h);
    assert!((nether - (over + (1.0 - over) * 0.1)).abs() < 1e-6, "the nether's ambient light 0.1: {nether} from {over}");
}

/// The no-action count of a zombie in the end after 100 ticks: `Mob.serverAiStep`'s 1 and `Monster.updateNoActionTime`'s 2 where the spot is
/// brighter than 0.5. The day makes the spot bright (sky 15), the night dark (sky 4, the end's 0.25 does not lift it above 0.5).
fn end_count_after_100(day: bool) -> Option<i32> {
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_dimension(DimensionFacts::of_builtin_level(horizon_core::LevelKey::END));
    h.set_daylight(day);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 4);
    admit(&mut h.world, e, DespawnDef::MOB, 40.0 * 40.0);
    rate_light(&mut h.world, e, false);
    for _ in 0..100 {
        h.tick();
    }
    no_action_time(&h.world, e)
}

#[test]
fn a_zombie_in_the_end_light_counts_three_a_tick_and_one_in_the_dark() {
    if !have_data() || wrong_species(&["minecraft:zombie"]) {
        return;
    }
    assert_eq!(end_count_after_100(true), Some(300), "bright: 3 a tick, sampled at tick 100");
    assert_eq!(end_count_after_100(false), Some(100), "dark: 1 a tick, sampled at tick 100");
}

/// `SpawnStats` of the cubes at the full difficulty: the size is 1, 2 or 4 in shares 1/6, 1/3, 1/2; with no special multiplier, a third each.
#[test]
fn a_cube_is_one_two_or_four_in_the_shares_of_its_difficulty() {
    let no_tag = |_: &str| false;
    let draw = Roll::Pow2(&Roll::Grow { bound: 3, below: 2, chance: 0.5 });
    for (special, shares) in [(1.0_f32, [(0.14, 0.20), (0.30, 0.37), (0.46, 0.53)]), (0.0, [(0.30, 0.37), (0.30, 0.37), (0.30, 0.37)])] {
        let mut count = [0u32; 3];
        for seed in 0..3000 {
            let size = roll(&draw, &Ctx { biome: "minecraft:plains", biome_has: &no_tag, special, baby: false }, &mut LegacyRandom::new(seed));
            count[size.trailing_zeros().min(2) as usize] += 1;
        }
        for (n, (lo, hi)) in count.iter().zip(shares) {
            let share = f64::from(*n) / 3000.0;
            assert!((lo..=hi).contains(&share), "the share {share} of the special {special} is out of {lo}..{hi}");
        }
    }
}
