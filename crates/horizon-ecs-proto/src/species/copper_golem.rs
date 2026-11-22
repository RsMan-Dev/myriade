//! `CopperGolem` and `CopperGolemAi`: two sensors, the core and idle activities, the door (`interact_with_door`, core 3), the transport between
//! chests (`transport_items`, idle 0) and the generic behaviours. The transport's cooldown is a memory that expires by its deadline (the
//! constructor's value, then 140 after a trip that finds no chest); the stroll waits for it (as the `Mob` does), so a golem walks while it lasts.
//! The gaze countdown (`count_down_cooldown_ticks`, core 4) runs only while its memory is set; the golem's brain never sets it (the writer,
//! `RandomLookAround`, is in the camel's and armadillo's brains only), as in vanilla.
use super::Def;
use crate::behaviours::shear::ShearDef;
use crate::behaviours::Cap;
use crate::goals::Slot;
use crate::brain::behaviours::{count_down_cooldown_ticks, do_nothing, interact_with_door, look_at_player_sometimes, look_at_target_sink, move_to_target_sink, panic, run_one, stroll, transport_items};
use crate::brain::sensors::{hurt_by, living};
use crate::brain::*;

const DOOR: interact_with_door::Params = interact_with_door::Params { cooldown: 0, ..interact_with_door::Params::DEFAULT };

/// `CopperGolem.shear`, `readyForShearing`: the antenna holds an item of the shear tag; it leaves the slot and falls 1.5 above the feet.
pub static SHEARING: ShearDef = ShearDef {
    equip: |_, _| {},
    ready: |w, e| {
        let item = crate::goals::equipped(w, e, Slot::Saddle);
        item != 0 && crate::items::is("#minecraft:shearable_from_copper_golem", item)
    },
    apply: |w, e| {
        let item = crate::equipment::take_slot(w, e, Slot::Saddle);
        let (Some(name), Some(at)) = (crate::items::item_name(item).filter(|_| item != 0), w.get::<crate::cow::Pos>(e).map(|p| p.0)) else { return false };
        crate::behaviours::shear::drop_stacks(w, vec![horizon_client_lib::items::ItemStack::new(name, 1)], [at.x, at.y + 1.5, at.z]);
        true
    },
    table: |_, _| None,
    sound: "minecraft:entity.copper_golem.shear",
    height: Some(0.0),
    puff: None,
};

pub static DEF: Def = Def {
    name: "minecraft:copper_golem",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain]),
    brain: BrainDef {
        sensors: &[SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&hurt_by::SENSOR, 20)],
        activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }, ActivityDef { name: "idle", slot: 5, requires: &[] }],
        pick: &["idle"],
        initial: &[Initial { key: &TRANSPORT_ITEMS_COOLDOWN_TICKS, base: 60, range: 40 }],
        behaviours: &[
            BEntry { behaviour: &panic::BEHAVIOUR, activity: "core", priority: 0, params: &panic::Params { speed: 1.5, threat: None }, needs: &[], duration: Some((100, 120)), kids: &[] },
            BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
            BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "idle", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((150, 250)), kids: &[] },
            BEntry { behaviour: &transport_items::BEHAVIOUR, activity: "idle", priority: 0, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &interact_with_door::BEHAVIOUR, activity: "core", priority: 3, params: &DOOR, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &count_down_cooldown_ticks::BEHAVIOUR, activity: "core", priority: 4, params: &count_down_cooldown_ticks::Params { key: &GAZE_COOLDOWN_TICKS }, needs: &[(&GAZE_COOLDOWN_TICKS, Need::Present)], duration: None, kids: &[] },
            BEntry {
                behaviour: &run_one::QUIET,
                activity: "idle",
                priority: 2,
                params: &(),
                needs: &[(&WALK_TARGET, Need::Absent), (&TRANSPORT_ITEMS_COOLDOWN_TICKS, Need::Present)],
                duration: None,
                kids: &[
                    Kid { behaviour: &stroll::BEHAVIOUR, weight: 1, params: &stroll::Params { speed: 1.0, h: 2, v: 2, dry_only: false }, needs: &[], duration: None },
                    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
                ],
            },
        ],
    },
    rules: &[
        crate::rules::RuleEntry::new(&crate::coating::COAT, &crate::coating::CoatParams::COPPER),
        crate::rules::RuleEntry::new(&crate::behaviours::shear::SHEAR, &SHEARING),
        crate::rules::RuleEntry::new(&crate::hand_over::HAND_OVER, &crate::hand_over::HandOverParams::THROWS_BACK),
    ],
    ..Def::EMPTY
};
