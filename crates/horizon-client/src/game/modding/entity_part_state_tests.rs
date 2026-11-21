//! Entity Part state (arismoko/horizon#796 stage 3) on real entities: one value each, kept with its entity
//! through edits, transactions, removal, unloading, reloading and changes of level.
use std::panic::AssertUnwindSafe;

use horizon_core::nbt::{Compound, Tag};
use horizon_mod_runtime::attachments::{AttachmentCodec, AttachmentError};
use mod_api::{AttachmentType, EntityPart, EntityPartState, Hooks, Registrar};

use super::Game;

static CHARGE: AttachmentType<i32> = AttachmentType {
    namespace: "part_state",
    name: "charge",
    default: || 0,
    codec: Some(AttachmentCodec {
        encode: |n| Ok([("n".into(), Tag::Int(*n))].into()),
        decode: |saved: &Compound| match saved.get(&"n".into()) {
            Some(Tag::Int(n)) => Ok(*n),
            _ => Err(AttachmentError::Decode),
        },
    }),
    copy_on_death: false,
    sync: false,
};

/// Keeps a charge on each entity of its types.
struct Charge;
#[mod_api::horizon_part]
impl EntityPart for Charge {
    fn register_goals(&self, goals: &mut mod_api::GoalRegistrar<'_>) {
        let _ = goals;
    }
}
impl EntityPartState for Charge {
    type Data = i32;
    const DATA: &'static AttachmentType<i32> = &CHARGE;
}

/// Declares the general type `part_state:robot` and attaches `Charge` with state to two mob
/// types and to the robot; the `refused` case also attaches it to the refused kind `refused`.
struct StateMod {
    refused: Option<&'static str>,
}
#[mod_api::__subscriptions]
impl Hooks for StateMod {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        r.declare_general_entity(
            "robot",
            mod_api::GeneralEntityCategory::Misc,
            0.8,
            1.9,
            mod_api::GeneralSavePolicy::Save,
            true,
            8,
            3,
            1,
            &[],
        )
        .unwrap();
        for kind in ["minecraft:cow", "minecraft:pig", "part_state:robot"]
            .into_iter()
            .chain(self.refused)
        {
            r.entity_part_with_state(kind, Charge).unwrap();
        }
    }
}

/// `test` on a game with `state_mod` installed. The registration phase is scoped around the
/// whole fixture, since the game fixture registers the installed mods again and a general type
/// is only declared while that phase is current.
fn with_mod(state_mod: StateMod, test: impl FnOnce(&mut Game<'_>)) {
    mod_platform::__private::initialize().expect("test registration requires the mod API runtime");
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::Registration);
    let mods: mod_api::Mods =
        vec![mod_api::ModEntry::new("part_state", Box::new(state_mod))].into();
    mod_platform::testing::with_mods(mods, || {
        super::native_entity_refs::tests::with_game(test);
    });
}

#[test]
#[ignore = "daily"]
fn entity_part_state_on_a_player_a_vanilla_family_or_an_unknown_type_refuses_the_load() {
    const NAME: &str = "game::entity_part_state_tests::entity_part_state_on_a_player_a_vanilla_family_or_an_unknown_type_refuses_the_load";
    const CHILD: &str = "HORIZON_ENTITY_PART_STATE_KIND";
    if super::native_entity_refs::tests::run_fresh_cases(
        NAME,
        CHILD,
        &["player", "boat", "minecart", "unknown"],
    ) {
        return;
    }
    let (kind, reason) = match std::env::var(CHILD).unwrap().as_str() {
        "player" => (
            "minecraft:player",
            "players: respawn identity is not decided",
        ),
        "boat" => ("minecraft:oak_boat", "no entity part runs on this kind"),
        "minecart" => ("minecraft:minecart", "no entity part runs on this kind"),
        _ => ("part_state:nothing", "no such entity type"),
    };
    let loaded = std::panic::catch_unwind(AssertUnwindSafe(|| {
        with_mod(
            StateMod {
                refused: Some(kind),
            },
            |_| {},
        );
    }));
    let panic = loaded.expect_err("the load is refused");
    let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
    let refusal = format!(
        "part_state: entity part {}::Charge on {kind}: state part_state:charge of type i32 \
         refused: entity type {kind}: {reason}",
        module_path!()
    );
    assert!(message.contains("Mod parts refused:"), "{message}");
    assert!(message.contains(&refusal), "{message}");
    // The accepted kinds of the same mod are not named.
    for accepted in ["minecraft:cow", "minecraft:pig", "part_state:robot"] {
        assert!(!message.contains(&format!("on {accepted}:")), "{message}");
    }
}
