//! `Shulker`: a body fixed to a face of a block, `no_gravity`, that opens, shoots homing projectiles and teleports. The behaviours are generic;
//! every number and sound name of the shulker is here.
use super::Def;
use bevy_ecs::prelude::{Entity, World};
use crate::behaviours::spawn_draw::{Draw, Draws, Store, When};
use crate::rules::spawn_draw::Roll;
use crate::behaviours::{Cap, Entry, attached_face, hurt_by, homing_projectile, homing_shot, look_at_player, nearest_target, peek, random_look, teleport_on};
use crate::effects::Kind;
use crate::projectile::{Damage, Flight, OnHit, Order};
use crate::rules::RuleEntry;
use crate::behaviours::attached_face::Sounds;

const SOUNDS: Sounds = Sounds { open: "entity.shulker.open", close: "entity.shulker.close" };
const SHELLS: &[&str] = &["minecraft:shulker"];

/// The projectile: 4 damage and levitation for 10 s where it hits; its steering sets the velocity (no drag, no frame gravity).
pub const SHOT: homing_projectile::Config = homing_projectile::Config {
    flight: Flight { id: "minecraft:shulker_bullet", gravity: 0.0, inertia: 1.0, order: Order::MoveThenDrag, damage: Damage::Fixed(4.0), size: 0.3125, hit: OnHit::Effect(Kind::Levitation, 0, 200), bullet: true, ..Flight::SNOWBALL },
    gravity: 0.04,
    speed: 0.15,
    grow: 1.025,
    turn: 0.2,
    max_delta: 1.0,
    base_steps: 10,
    step_count: 5,
    step_len: 10,
};

pub static DEF: Def = Def {
    name: "minecraft:shulker",
    max_scale: 3.0,
    gravity: false,
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_types: SHELLS, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { attach_reach: Some(4.5), ..nearest_target::Params::PLAYER }),
    ],
    goals: &[
        Entry::new(&crate::behaviours::move_emission::MODULE, 0, &crate::behaviours::move_emission::Params { events: false, sounds: false, ..crate::behaviours::move_emission::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 1, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&homing_shot::MODULE, 4, &homing_shot::Params {
            reach_sq: 400.0,
            open: 100,
            turn: 180.0,
            wait_base: 20,
            wait_step: 10,
            wait_count: 10,
            shot: &SHOT,
            sound: "entity.shulker.shoot",
            volume: 2.0,
            pitch_spread: 0.2,
            sounds: SOUNDS,
            covered: true,
        }),
        Entry::new(&peek::MODULE, 7, &peek::Params { one_in: 40, open: 30, ticks: 20, spread: 3, sounds: SOUNDS, covered: true }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        Entry::new(&teleport_on::MODULE, 0, &teleport_on::Params {
            shells: SHELLS,
            tag: "minecraft:shulker_does_not_teleport_to",
            sounds: SOUNDS,
            covered: true,
            teleport: "entity.shulker.teleport",
            hurt_below: 0.5,
            hurt_one_in: 4,
            tries: 5,
            radius: 8,
            eject: true,
            copy_on_hit: true,
            crowd_radius: 8.0,
            crowd_divisor: 5.0,
        }),
    ],
    rules: &[RuleEntry::new(&attached_face::RULE, &attached_face::Params { colour: 16, covered: true })],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::FireImmune, Cap::Collidable]),
    sound_source: super::SoundCategory::Hostile,
    draws: Some(&DRAWS),
    ..Def::EMPTY
};

/// `Shulker.finalizeSpawn`: `setYRot(0)` before the golem's finalize, no draw. The spawn's random yaw is overwritten here.
static DRAWS: Draws = Draws { group: None, list: &[Draw { when: When::Before, kind: crate::behaviours::spawn_draw::Kind::Plain(Roll::Const(0)), store: Store::Call(face_zero) }] };

fn face_zero(w: &mut World, e: Entity, _: u32) {
    if let Some(mut y) = w.get_mut::<crate::cow::Yaw>(e) {
        y.0 = 0.0;
    }
}
