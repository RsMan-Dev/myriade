use crate::blocks::Fluid;
use crate::health::DamageLevel;
use horizon_core::{
    geometry::{Vec3, trig},
    random::RandomSource,
};
use horizon_world::move_gate::MoveGate;



/// `Turtle.TurtleGoHomeGoal`: a mother with an egg heads home; any adult
/// that strayed 64 blocks away does now and then.
#[derive(Clone, Debug, Default)]
struct GoHome {
    stuck: bool,
    close_to_home_try_ticks: i32,
}


/// `Turtle.TurtleTravelGoal`: a turtle in water swims towards a spot up to
/// 512 blocks away, a leg at a time.
#[derive(Clone, Debug, Default)]
struct Travel {
    stuck: bool,
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{
        Species,
        path::{Path, PathNode, PathType},
    };
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    #[test]
    fn walks_on_land_at_a_third_of_its_movement_speed() {
        // TurtleMoveControl.tick on the ground: updateSpeed makes it
        // max(speed / 2, 0.06) = 0.06, then Mth.lerp(0.125F, 0.06, 0.25),
        // every tick. The default MoveControl walked it at the full 0.25, so
        // the walk animation (and TurtleModel's 8 * walkSpeed leg swing) ran
        // about nine times too fast (#1776).
        let mut m = strolling_turtle();
        let land = 0.06_f32 + 0.125 * (0.25 - 0.06_f32);
        assert_eq!(speeds(&mut m, 40), vec![land; 40]);
        // Facing the wanted point, the body with it; it never jumps.
        assert_eq!(m.body_yaw, m.yaw);
        assert!(crate::mobs::living::wrap(m.yaw).abs() < 1.0e-3);
        assert!(!m.ai.jump);
        assert_eq!(m.ai.mov.operation, Operation::MoveTo);
        assert_eq!(m.velocity.y, 0.0);
    }

    #[test]
    fn the_leg_animation_runs_at_vanilla_s_rate() {
        // LivingEntity.updateWalkAnimation turns the distance a tick into
        // min(distance * 4, 1), and TurtleModel.setupAnim swings the legs
        // by 8 * that speed, so the rate of the walk animation is the rate
        // of the walk. The turtle walks at its own pace, so the animation
        // runs at vanilla's rate instead of at the maximum (#1776).
        let mut m = strolling_turtle();
        let mut last = m.position;
        let mut distance = 0.0_f64;
        for _ in 0..40 {
            // Entity.tick keeps the position the tick started at.
            m.previous = last;
            // TurtleMoveControl.tick: the forward input.
            move_control(&mut m);
            // LivingEntity.travelInAir on grass: the friction influenced
            // speed is the input speed (grass' friction is 0.6, so it is
            // not above it), the move uses the motion as it is, and the
            // motion is then scaled by the friction (0.6) times the air
            // drag (0.91). The move itself needs a world, so the flat
            // ground is stepped here.
            m.move_relative(m.ai.speed, Vec3::new(0.0, 0.0, 1.0));
            m.position.x += m.velocity.x;
            m.position.z += m.velocity.z;
            let drag = f64::from(0.6_f32 * 0.91_f32);
            m.velocity.x *= drag;
            m.velocity.z *= drag;
            // LivingEntity.updateWalkAnimation.
            m.update_walk_animation();
            distance = (m.position.x - m.previous.x).hypot(m.position.z - m.previous.z);
            last = m.position;
        }
        // The ground speed: the input (0.06 + 0.125 * (0.25 - 0.06) =
        // 0.08375) over one minus the drag (0.546) = 0.18447 blocks a tick.
        assert!((distance - 0.083_75 / (1.0 - 0.546)).abs() < 1.0e-6);
        // min(distance * 4, 1) for that pace: 0.73789. The generic
        // MoveControl walked the turtle at the full 0.25, which put it at
        // 0.55066 blocks a tick and saturated the walk animation at 1.0.
        assert!((m.limb_speed - 0.737_885_5).abs() < 1.0e-6);
    }

    #[test]
    fn water_speed_is_cut_away_from_home_and_for_babies() {
        // updateSpeed in water near home: no cut, the lerp climbs towards
        // 0.25, and it lifts the turtle 0.005 a tick. (Tests only the move
        // control's speed; Turtle.travelInWater is ported in travel().)
        let mut m = strolling_turtle();
        m.on_ground = false;
        m.fluids.in_water = true;
        let s = speeds(&mut m, 60);
        assert_eq!(s[0], 0.125 * 0.25);
        assert!(s.windows(2).all(|w| w[1] > w[0]) && s[59] > 0.24 && s[59] < 0.25);
        assert!((m.velocity.y - 60.0 * 0.005).abs() < 1.0e-9);
        // More than 16 blocks from home: max(speed / 2, 0.08) first.
        let mut m = strolling_turtle();
        m.on_ground = false;
        m.fluids.in_water = true;
        m.extra.home = Some([0, 0, 40]);
        let far = 0.08_f32 + 0.125 * (0.25 - 0.08_f32);
        assert_eq!(speeds(&mut m, 20), vec![far; 20]);
        // A baby: max(speed / 3, 0.06).
        let mut m = strolling_turtle();
        m.on_ground = false;
        m.fluids.in_water = true;
        m.extra.age = -24000;
        let young = 0.06_f32 + 0.125 * (0.25 - 0.06_f32);
        assert_eq!(speeds(&mut m, 20), vec![young; 20]);
    }

    #[test]
    fn stops_when_the_path_is_done_or_the_point_reached() {
        let mut m = strolling_turtle();
        m.ai.nav.stop();
        assert_eq!(speeds(&mut m, 1), vec![0.0]);
        let mut m = strolling_turtle();
        m.ai.mov.set_wanted(0.0, 0.0, 0.0, 1.0);
        assert_eq!(speeds(&mut m, 1), vec![0.0]);
        // updateSpeed runs before the early return: a stopped turtle in
        // water is still lifted 0.005, path done or not moving at all.
        let mut m = strolling_turtle();
        m.fluids.in_water = true;
        m.ai.nav.stop();
        assert_eq!(speeds(&mut m, 1), vec![0.0]);
        assert_eq!(m.velocity.y, 0.005);
        let mut m = strolling_turtle();
        m.fluids.in_water = true;
        m.ai.mov.operation = Operation::Wait;
        assert_eq!(speeds(&mut m, 1), vec![0.0]);
        assert_eq!(m.velocity.y, 0.005);
    }

    #[test]
    fn lifts_towards_a_point_above_by_the_slope_of_the_way_there() {
        // setDeltaMovement(... + getSpeed() * (dy / dist) * 0.1): two up,
        // three across and ten ahead.
        let mut m = strolling_turtle();
        m.ai.mov.set_wanted(3.0, 2.0, 10.0, 1.0);
        let speed = speeds(&mut m, 1)[0];
        let dist = (3.0_f64 * 3.0 + 2.0 * 2.0 + 10.0 * 10.0).sqrt();
        assert_eq!(m.velocity.y, f64::from(speed) * (2.0 / dist) * 0.1);
        assert!(m.velocity.y > 0.0);
    }

    #[test]
    fn turns_at_most_ninety_degrees_a_tick_and_wraps_into_a_turn() {
        // rotlerp(yRot, target, 90): from 150 towards about -16.7 it turns
        // the full 90 to 60, then reaches the target, kept in [0, 360] as
        // about 343.3; the body follows the yaw each tick.
        let mut m = strolling_turtle();
        m.ai.mov.set_wanted(3.0, 0.0, 10.0, 1.0);
        m.yaw = 150.0;
        m.body_yaw = 150.0;
        let target = trig::atan2_degrees(10.0, 3.0) - 90.0;
        assert!((-17.0..-16.0).contains(&target));
        move_control(&mut m);
        assert_eq!((m.yaw, m.body_yaw), (60.0, 60.0));
        move_control(&mut m);
        assert_eq!(m.yaw, 60.0 + (target - 60.0) + 360.0);
        assert_eq!(m.body_yaw, m.yaw);
    }

    /// `Turtle.TurtleLayEggGoal.tick`: the pitch comes from the level's
    /// random, the egg count from the turtle's, and the egg it sets is
    /// followed by `block_place` from the turtle at the new block.
    #[test]
    #[ignore = "daily"]
    fn laying_an_egg_posts_block_place_with_the_level_s_pitch_and_its_own_eggs() {
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            use horizon_core::random::LegacyRandom;
            let mut m = strolling_turtle();
            m.position = Vec3::new(0.5, 0.0, 0.5);
            m.extra.flag = true;
            m.extra.counter = 201;
            m.random = horizon_core::random::EntityRandom::own(LegacyRandom::new(7));
            let mut lr = crate::health::TestLevel::new(11);
            let mut level = lr.random().clone();
            let pitch = 0.9_f32 + level.next_float() * 0.2_f32;
            let mut own = m.random.clone();
            let eggs = own.next_int_bound(4).unwrap() + 1;
            let mut events = Vec::new();
            let mut cx = Cx {
                w,
                level: &mut lr.level(),
                events: &mut events,
            };
            let mut goal = LayEgg(MoveToBlock {
                reached: true,
                block: [0, -1, 0],
                ..Default::default()
            });
            goal.tick(&mut m, &mut cx);
            let base = w.registry.default_state("minecraft:turtle_egg").unwrap();
            let mut state = w.registry.state(base).unwrap().clone();
            state.properties.insert("eggs".into(), eggs.to_string());
            let egg = w.registry.resolve(&state).unwrap();
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    MobEvent::Sound("entity.turtle.lay_egg", _, _, p) if *p == pitch
                )),
                "the sound's pitch is the level random's: {events:?}"
            );
            assert!(
                events.iter().any(
                    |e| matches!(e, MobEvent::SetBlock { pos: [0, 0, 0], state } if *state == egg)
                ),
                "the turtle's random picked the egg count: {events:?}"
            );
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    MobEvent::GameEvent {
                        event: "minecraft:block_place",
                        at: [0.5, 0.5, 0.5],
                        source: 1,
                        state,
                    } if *state == egg
                )),
                "the new egg posts block_place from the turtle: {events:?}"
            );
            assert_eq!(lr.random().state(), level.state());
            assert_eq!(m.random.state(), own.state());
        });
        assert!(ran, "vanilla data is missing");
    }
}
