//! Restes du lot 1 (`suivi/ecs-portage/phase1-listes/regles-mob-vers-ecs-1.md`) : pendants ECS des tests de l'ancien Mob qui n'en avaient pas.
//! Valeurs vanilla 26.3 relues dans le jar, pas dans l'ancien Mob.
use crate::common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::merchant::{ItemCost, MerchantOffer};
use bevy_ecs::prelude::World;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::behaviours::stroll;
use horizon_ecs_proto::cow::{Rng, Stroll};
use horizon_ecs_proto::goals::Slot;
use horizon_ecs_proto::behaviours::food_inventory;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_client_lib::mobs::villager;
use horizon_ecs_proto::behaviours::trade_offers::TradeData;
use horizon_ecs_proto::gliding::{self, FallFlying};
use horizon_ecs_proto::behaviours::trade_offers::{profession_name, Merchant, UnresolvedProfession, UNRESOLVED};
use horizon_ecs_proto::cow::{spawn_species, SpeciesTable};
use horizon_ecs_proto::equip_stack;
use horizon_ecs_proto::nbt::{self, Saved};
use horizon_ecs_proto::behaviours::{elder_curse::Silent, look_for_player, Entry};
use horizon_ecs_proto::cow::Pos;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::species::Config;
use horizon_ecs_proto::{goals, targeting::{self, Eye, Target}};
use bevy_ecs::entity::Entity;
use horizon_ecs_proto::sound_slots::{self, Overrides, Slot as SoundSlot};
use horizon_ecs_proto::behaviours::take_block::Carried;
use horizon_ecs_proto::rules::leave_block::cell_of;
use std::collections::HashMap;

fn heard(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<goals::Events>().host.drain().collect()
}

fn spawn(h: &mut Herd, name: &str) -> bevy_ecs::entity::Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let species = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &species, 4.5, 4.5, 5)
}

/// Retirer la pièce de corps rend la pile entière (dégâts, nom) : `from` du changement.
#[test]
fn removing_body_equipment_keeps_the_whole_stack() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let horse = spawn(&mut h, "minecraft:horse");
    let mut stack = ItemStack::new("minecraft:red_carpet", 3);
    stack.damage = 7;
    assert!(equip_stack::set(&mut h.world, horse, 6, Some(stack.clone())));
    equip_stack::take_changed(&mut h.world);
    assert!(equip_stack::set(&mut h.world, horse, 6, None));
    let from = equip_stack::take_changed(&mut h.world).into_iter().find(|c| c.1 == 6).map(|c| c.2);
    assert_eq!(from, Some(Some(stack)), "the removed piece keeps its whole stack");
    assert_eq!(equip_stack::get(&h.world, horse, 6), None);
}

/// Poser une pièce de même objet remplace la pile : la santé d'usure de l'ancienne ne reste pas.
#[test]
fn replacing_armor_with_same_item_forgets_old_stack() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let horse = spawn(&mut h, "minecraft:horse");
    let mut old = ItemStack::new("minecraft:iron_helmet", 1);
    old.damage = 8;
    assert!(equip_stack::set(&mut h.world, horse, 2, Some(old.clone())));
    let mut replacement = ItemStack::new("minecraft:iron_helmet", 2);
    replacement.damage = 3;
    assert!(equip_stack::set(&mut h.world, horse, 2, Some(replacement.clone())));
    assert_eq!(equip_stack::get(&h.world, horse, 2), Some(replacement));
    let from = equip_stack::take_changed(&mut h.world).into_iter().last().map(|c| c.2);
    assert_eq!(from, Some(Some(old)), "the change names the old stack it replaces");
}

/// La selle garde son nombre d'objets.
#[test]
fn saddle_stack_keeps_count() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let horse = spawn(&mut h, "minecraft:horse");
    let saddle = ItemStack::new("minecraft:saddle", 3);
    assert!(equip_stack::set(&mut h.world, horse, 7, Some(saddle.clone())));
    assert_eq!(equip_stack::get(&h.world, horse, 7), Some(saddle));
}

/// Un cheval porte à la fois un casque (emplacement tête) et son armure de corps.
#[test]
fn a_horse_holds_a_helmet_and_its_body_armour_at_once() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let horse = spawn(&mut h, "minecraft:horse");
    let helmet = ItemStack::new("minecraft:iron_helmet", 1);
    let armour = ItemStack::new("minecraft:diamond_horse_armor", 1);
    assert!(equip_stack::set(&mut h.world, horse, 2, Some(helmet.clone())));
    assert!(equip_stack::set(&mut h.world, horse, 6, Some(armour.clone())));
    assert_eq!(equip_stack::get(&h.world, horse, 2), Some(helmet));
    assert_eq!(equip_stack::get(&h.world, horse, 6), Some(armour));
}

fn offer() -> MerchantOffer {
    MerchantOffer {
        base_cost_a: ItemCost { stack: ItemStack::new("minecraft:emerald", 3) },
        cost_b: None,
        result: ItemStack::new("minecraft:iron_ingot", 2),
        uses: 2,
        max_uses: 12,
        reward_exp: true,
        special_price_diff: -1,
        demand: 4,
        price_multiplier: 0.2,
        xp: 7,
    }
}

/// Les noms de métiers vanille aux lignes 0, 5 et 14 de la table (aucun, fermier, armurier... weaponsmith est la dernière).
#[test]
fn the_vanilla_professions_name_their_rows() {
    if !have_data() {
        return;
    }
    let h = Herd::with_slab_awake(8, true, false);
    assert_eq!(profession_name(&h.world, 0), "none");
    assert_eq!(profession_name(&h.world, 5), "farmer");
    assert_eq!(profession_name(&h.world, 14), "weaponsmith");
}

/// Les offres d'un villageois, son xp et son niveau survivent à une sauvegarde puis un chargement.
#[test]
fn villager_saved_offers_survive_reload() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let v = h.world.spawn((Saved, Merchant::new(false, 5, 2))).id();
    {
        let mut m = h.world.get_mut::<Merchant>(v).unwrap();
        m.trade.xp = 37;
        m.trade.unset = false;
        m.trade.offers = vec![offer()];
        m.level = 3;
    }
    let saved = nbt::save(&h.world, v).unwrap();
    let back = h.world.spawn((Saved, Merchant::new(false, 0, 0))).id();
    assert!(nbt::load(&mut h.world, back, &saved).is_some());
    let m = h.world.get::<Merchant>(back).unwrap();
    assert_eq!(m.trade.offers, vec![offer()]);
    assert_eq!(m.trade.xp, 37);
    assert_eq!(m.level, 3);
}

/// Un zombie villageois écrit ses offres comme le villageois (`ZombieVillager.addAdditionalSaveData`, clé `Offers`).
#[test]
fn zombie_villager_saved_offers_survive_reload() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let v = spawn(&mut h, "minecraft:zombie_villager");
    h.world.entity_mut(v).insert((Saved, Merchant::new(false, 5, 2)));
    h.world.get_mut::<Merchant>(v).unwrap().trade.offers = vec![offer()];
    h.world.get_mut::<Merchant>(v).unwrap().trade.unset = false;
    let saved = nbt::save(&h.world, v).unwrap();
    assert!(saved.get(&JavaString::from("Offers")).is_some(), "the offers are written");
    let back = spawn(&mut h, "minecraft:zombie_villager");
    h.world.entity_mut(back).insert((Saved, Merchant::new(false, 0, 0)));
    assert!(nbt::load(&mut h.world, back, &saved).is_some());
    assert_eq!(h.world.get::<Merchant>(back).unwrap().trade.offers, vec![offer()]);
}

/// Un métier qu'aucun mod ne fournit garde son nom sauvé et ses offres : le marchand reste opaque, sans index.
#[test]
fn villager_missing_mod_save_and_restore_preserves_name_and_offers() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let v = h.world.spawn((Saved, Merchant::new(false, UNRESOLVED, 2), UnresolvedProfession("test:engineer".into()))).id();
    h.world.get_mut::<Merchant>(v).unwrap().trade.offers = vec![offer()];
    h.world.get_mut::<Merchant>(v).unwrap().trade.unset = false;
    let saved = nbt::save(&h.world, v).unwrap();
    let Some(Tag::Compound(data)) = saved.get(&JavaString::from("VillagerData")) else { panic!("VillagerData") };
    assert_eq!(data.get(&JavaString::from("profession")), Some(&Tag::String(JavaString::from("test:engineer"))));
    let back = h.world.spawn((Saved, Merchant::new(false, 0, 0))).id();
    assert!(nbt::load(&mut h.world, back, &saved).is_some());
    let m = h.world.get::<Merchant>(back).unwrap();
    assert_eq!(m.profession, UNRESOLVED);
    assert_eq!(m.trade.offers, vec![offer()]);
    assert_eq!(h.world.get::<UnresolvedProfession>(back).map(|u| u.0.clone()), Some("test:engineer".to_string()));
}

/// Même règle pour le zombie-villageois réel : son métier inconnu et ses offres sont gardés au rechargement.
#[test]
fn zombie_villager_missing_mod_save_and_restore_preserves_name_and_offers() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let v = spawn(&mut h, "minecraft:zombie_villager");
    h.world.entity_mut(v).insert((Saved, Merchant::new(false, UNRESOLVED, 2), UnresolvedProfession("test:engineer".into())));
    h.world.get_mut::<Merchant>(v).unwrap().trade.offers = vec![offer()];
    h.world.get_mut::<Merchant>(v).unwrap().trade.unset = false;
    let saved = nbt::save(&h.world, v).unwrap();
    let back = spawn(&mut h, "minecraft:zombie_villager");
    h.world.entity_mut(back).insert((Saved, Merchant::new(false, 0, 0)));
    assert!(nbt::load(&mut h.world, back, &saved).is_some());
    let m = h.world.get::<Merchant>(back).unwrap();
    assert_eq!(m.profession, UNRESOLVED);
    assert_eq!(m.trade.offers, vec![offer()]);
    assert_eq!(h.world.get::<UnresolvedProfession>(back).map(|u| u.0.clone()), Some("test:engineer".to_string()));
}

/// Fin du rayon ou coup : `RandomStrollGoal.trigger` demande la marche sans attendre (`force`). Les appels sont dans
/// `guardian_beam::stop` et `guardian_thorns::blow`.
#[test]
fn a_guardian_strolls_at_once_when_its_beam_ends_or_it_is_hurt() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let g = spawn(&mut h, "minecraft:guardian");
    assert_eq!(h.world.get::<Stroll>(g).map(|s| s.force), Some(false));
    stroll::trigger(&mut h.world, g);
    assert_eq!(h.world.get::<Stroll>(g).map(|s| s.force), Some(true), "the next ask skips the wait");
}

/// `FallFlying` chargé : 1 pose le vol, 0 et absent ne le posent pas (`readAdditionalSaveData`).
#[test]
fn a_loaded_mob_keeps_its_saved_glide_flag() {
    if !have_data() {
        return;
    }
    for (flag, glides) in [(Some(1i8), true), (Some(0), false), (None, false)] {
        let mut h = Herd::with_slab_awake(8, true, false);
        let e = h.world.spawn(Saved).id();
        let mut c = Compound::new();
        if let Some(v) = flag {
            c.insert("FallFlying".into(), Tag::Byte(v));
        }
        let _ = nbt::load(&mut h.world, e, &c);
        assert_eq!(h.world.get::<FallFlying>(e).is_some(), glides, "FallFlying {flag:?}");
    }
}

/// `FallFlying` écrit toujours, avec l'état courant : 0 hors vol, 1 pendant le vol (`addAdditionalSaveData`).
#[test]
fn a_saved_mob_writes_its_current_glide_flag() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = h.world.spawn(Saved).id();
    let flag = |w: &World| nbt::save(w, e).unwrap().get(&JavaString::from("FallFlying")).cloned();
    assert_eq!(flag(&h.world), Some(Tag::Byte(0)), "not gliding");
    gliding::set(&mut h.world, e, true);
    assert_eq!(flag(&h.world), Some(Tag::Byte(1)), "gliding");
    gliding::set(&mut h.world, e, false);
    assert_eq!(flag(&h.world), Some(Tag::Byte(0)), "landed again");
}

/// `Util.getRandom(slotsWithGliders, random)` : une seule pioche du générateur du mob sur un jet de l'usure (`nextInt(1)`), qui prend le torse.
#[test]
fn the_wear_draws_once_from_the_mobs_own_random() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = spawn(&mut h, "minecraft:horse");
    let mut mirror = *h.world.get::<Rng>(e).unwrap();
    let _ = mirror.next_int(1);
    assert_eq!(gliding::pick_glider_slot(&mut h.world, e), Some(Slot::Chest));
    assert_eq!(h.world.get::<Rng>(e).unwrap().0, mirror.0, "exactly one draw");
}

/// `HarvestFarmland` : le fermier sème la graine de son inventaire dans la terre labourée libre, le bloc posé est celui de l'objet
/// (`items.get(item).block`), une graine consommée, et l'événement de pose et le son de plantation sont postés avec le villageois.
#[test]
fn planting_posts_block_place_with_villager_and_crop_state() {
    if !have_data() {
        return;
    }
    let farmer = villager::profession_index("farmer").unwrap();
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn(&mut h, "minecraft:villager");
    let (items, enchantments, trades) = registries();
    let state = items.get("minecraft:wheat_seeds").and_then(|i| i.block).unwrap();
    h.world.insert_resource(TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    h.set_block(1, 0, 0, "minecraft:farmland");
    h.world.entity_mut(e).insert((Merchant::new(false, farmer, 0), food_inventory::Stock(vec![("minecraft:wheat_seeds".to_string(), 2)])));
    let mut placed = Vec::new();
    for _ in 0..600 {
        h.tick();
        placed.extend(heard(&mut h).into_iter().filter(|ev| matches!(ev, HostEvent::BlockPlace { entity, .. } if *entity == e)));
        if !placed.is_empty() {
            break;
        }
    }
    assert_eq!(placed.len(), 1, "one seed sown on the bare farmland");
    assert!(matches!(placed[0], HostEvent::BlockPlace { state: s, .. } if s == state), "the planted block is the seed item's block");
    assert_eq!(h.world.get::<food_inventory::Stock>(e).unwrap().0, [("minecraft:wheat_seeds".to_string(), 1)], "one seed consumed");
}

/// `Enderman.playStareSound` dans le monde : le son de regard vient aux yeux, une fois en 400 ticks, et un regard silencieux compte son temps.
#[test]
fn the_stare_sound_is_throttled_silenced_and_at_the_eyes() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(30, false, false);
    goals::set_lazy_look(&mut h.world, false);
    let e = enderman(&mut h, 0.5, 0.5);
    h.tick();
    player(&mut h, e, 6.0, true);
    let mut sounds = stare_run(&mut h, e, 40);
    assert_eq!(sounds.len(), 1, "the stare that makes it a target sounds once");
    let eye = h.world.get::<Eye>(e).unwrap().0;
    let p = h.world.get::<Pos>(e).unwrap().0;
    let (name, at, volume) = match sounds.remove(0) {
        HostEvent::Sound { name, at, volume, .. } => (name, at, volume),
        _ => unreachable!(),
    };
    assert_eq!(name, "entity.enderman.stare");
    assert_eq!(at, [p.x, p.y + eye, p.z], "at the eyes");
    assert_eq!(volume, 2.5);
    targeting::clear_target(&mut h.world, e);
    h.tick();
    player(&mut h, e, 6.0, true);
    assert!(stare_run(&mut h, e, 40).is_empty(), "a second stare within 400 ticks is silent");
    h.world.entity_mut(e).insert(Silent);
    for _ in 0..400 {
        h.tick();
    }
    targeting::clear_target(&mut h.world, e);
    h.tick();
    player(&mut h, e, 6.0, true);
    assert!(stare_run(&mut h, e, 40).is_empty(), "a silenced stare plays nothing");
    h.world.entity_mut(e).remove::<Silent>();
    targeting::clear_target(&mut h.world, e);
    h.tick();
    player(&mut h, e, 6.0, true);
    assert!(stare_run(&mut h, e, 40).is_empty(), "the silenced stare also counts its time");
}

/// Ticks until `e` has a target (at most `max`), and returns the stare sounds the host heard meanwhile.
fn stare_run(h: &mut Herd, e: Entity, max: usize) -> Vec<HostEvent> {
    let mut sounds = Vec::new();
    for _ in 0..max {
        h.tick();
        sounds.extend(heard(h).into_iter().filter(|ev| matches!(ev, HostEvent::Sound { name, .. } if *name == "entity.enderman.stare")));
        if h.world.get::<Target>(e).is_some() {
            break;
        }
    }
    sounds
}

/// A cow body renamed enderman, whose only target goal is the stare (as in the look-for-player tests).
fn enderman(h: &mut Herd, x: f64, z: f64) -> Entity {
    static ENTRIES: [Entry; 1] = [Entry::new(&look_for_player::MODULE, 1, &look_for_player::Params::DEFAULT)];
    let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
    sp.name = "minecraft:enderman";
    sp.goals = Config { entries: &ENTRIES };
    let e = spawn_species(&mut h.world, &sp, x, z, 7);
    h.world.flush();
    e
}

/// The player at `d` blocks east of the enderman, looking at it or not.
fn player(h: &mut Herd, e: Entity, d: f64, looks: bool) {
    let p = h.world.get::<Pos>(e).unwrap().0;
    let eye = h.world.get::<Eye>(e).unwrap().0;
    let (px, py, pz) = (p.x + d, p.y, p.z);
    let at = [p.x, p.y + eye, p.z];
    let from = [px, py + 1.62, pz];
    let dir = [at[0] - from[0], at[1] - from[1], at[2] - from[2]];
    let n = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let gaze = if looks { [dir[0] / n, dir[1] / n, dir[2] / n] } else { [-dir[0] / n, -dir[1] / n, -dir[2] / n] };
    h.world.resource_mut::<PlayerState>().set_gaze(0, gaze);
    h.world.resource_mut::<goals::Online>().set(goals::Player { id: 0, x: px, y: py, z: pz, main: 0, off: 0, ..Default::default() });
}

/// Sons déclarés par un mod pour un type : le son ambiant du type est remplacé, rendu muet, ou gardé quand le mod ne déclare que le pas.
/// La table est par type et remplacée au rechargement ; l'entité ne porte rien.
fn ambient_heard(h: &mut Herd, ticks: usize) -> Vec<String> {
    let mut names = Vec::new();
    for _ in 0..ticks {
        h.tick();
        for ev in heard(h) {
            if let HostEvent::Sound { name, .. } = ev {
                names.push(name.to_string());
            }
        }
    }
    names
}

#[test]
fn a_declared_ambient_replaces_the_species_sound_and_a_muted_one_is_silent() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let (items, enchantments, trades) = registries();
    h.world.insert_resource(TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    let _hoglin = spawn(&mut h, "minecraft:hoglin");
    sound_slots::replace(&mut h.world, HashMap::from([("minecraft:hoglin", Overrides { ambient: SoundSlot::Id("bird:call"), ..Default::default() })]));
    let heard_calls = ambient_heard(&mut h, 600);
    assert!(heard_calls.iter().any(|n| n == "bird:call"), "the declared ambient plays: {heard_calls:?}");
    assert!(!heard_calls.iter().any(|n| n.contains("hoglin")), "the species sound is replaced");
    sound_slots::replace(&mut h.world, HashMap::from([("minecraft:hoglin", Overrides { ambient: SoundSlot::Mute, ..Default::default() })]));
    assert!(ambient_heard(&mut h, 600).is_empty(), "a muted ambient plays nothing");
}

#[test]
fn a_step_only_declaration_keeps_the_species_ambient() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let (items, enchantments, trades) = registries();
    h.world.insert_resource(TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    let _hoglin = spawn(&mut h, "minecraft:hoglin");
    sound_slots::replace(&mut h.world, HashMap::from([("minecraft:hoglin", Overrides { step: SoundSlot::Mute, ..Default::default() })]));
    let names = ambient_heard(&mut h, 600);
    assert!(names.iter().any(|n| n.contains("hoglin")), "the species ambient stays: {names:?}");
}

#[test]
fn the_enderman_itself_never_blocks_a_placement() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = enderman(&mut h, 0.5, 0.5);
    h.world.entity_mut(e).insert(Carried(Some("minecraft:stone")));
    let p = h.world.get::<Pos>(e).unwrap().0;
    let feet = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
    // The seed is searched on the three draws alone: one whose first cell is the enderman's own feet cell.
    let seed = (1..100_000u64)
        .find(|&s| {
            let mut r = Rng(s);
            cell_of(p, [r.next_f64(), r.next_f64(), r.next_f64()]) == feet
        })
        .expect("a seed draws the feet cell");
    h.world.entity_mut(e).insert(Rng(seed));
    let mut placed = false;
    for _ in 0..1200 {
        h.tick();
        placed |= heard(&mut h).iter().any(|ev| matches!(ev, HostEvent::BlockPlace { at, .. } if at.map(|v| v.floor() as i32) == feet));
    }
    assert!(placed, "the block is set down in the enderman's own cell: it does not block its placement");
}

#[test]
fn a_declared_hurt_or_death_voice_plays_and_a_muted_one_does_not() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = spawn(&mut h, "minecraft:hoglin");
    let table = HashMap::from([("minecraft:hoglin", Overrides { hurt: SoundSlot::Id("bird:hurt"), death: SoundSlot::Mute, step: SoundSlot::Id("bird:step"), ..Default::default() })]);
    sound_slots::replace(&mut h.world, table);
    sound_slots::hurt(&mut h.world, e);
    sound_slots::death(&mut h.world, e);
    let names: Vec<String> = heard(&mut h).into_iter().filter_map(|ev| match ev {
        HostEvent::Sound { name, .. } => Some(name.to_string()),
        _ => None,
    }).collect();
    assert_eq!(names, ["bird:hurt"], "the hurt plays, the muted death does not");
    assert_eq!(sound_slots::sound(&h.world, e, |o| o.step, None), Some("bird:step"), "the step slot is read by the movement module");
}

#[test]
fn a_hurt_cow_plays_its_voice_and_a_baby_is_higher_and_a_killed_cow_plays_none_here() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = spawn(&mut h, "minecraft:cow");
    sound_slots::replace(&mut h.world, HashMap::from([("minecraft:cow", Overrides { hurt: SoundSlot::Id("minecraft:entity.cow.hurt"), ..Default::default() })]));
    sound_slots::hurt(&mut h.world, e);
    let adult: Vec<(String, f32, f32)> = heard(&mut h).into_iter().filter_map(|ev| match ev {
        HostEvent::Sound { name, volume, pitch, .. } => Some((name.to_string(), volume, pitch)),
        _ => None,
    }).collect();
    assert_eq!(adult.len(), 1, "one hurt voice");
    assert_eq!(adult[0].0, "minecraft:entity.cow.hurt", "the cow's own hurt voice");
    assert_eq!(adult[0].1, 1.0, "LivingEntity.getSoundVolume is 1.0");
    assert!((0.8..=1.2).contains(&adult[0].2), "an adult's pitch is around 1.0, got {}", adult[0].2);
    h.world.entity_mut(e).insert(horizon_ecs_proto::cow::GrowAt(0));
    sound_slots::hurt(&mut h.world, e);
    let baby = heard(&mut h).into_iter().find_map(|ev| match ev {
        HostEvent::Sound { pitch, .. } => Some(pitch),
        _ => None,
    }).expect("a baby's hurt voice");
    assert!((1.3..=1.7).contains(&baby), "a baby's pitch is raised by half an octave, got {baby}");
    sound_slots::death(&mut h.world, e);
    assert!(heard(&mut h).iter().all(|ev| !matches!(ev, HostEvent::Sound { .. })), "the death voice of a vanilla cow is the client's: none from the server");
}

#[test]
fn rechargement_remplace_la_table() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = spawn(&mut h, "minecraft:hoglin");
    sound_slots::replace(&mut h.world, HashMap::from([("minecraft:hoglin", Overrides { hurt: SoundSlot::Id("bird:hurt"), ..Default::default() })]));
    assert_eq!(sound_slots::sound(&h.world, e, |o| o.hurt, None), Some("bird:hurt"), "the first load declares the hurt");
    sound_slots::replace(&mut h.world, HashMap::new());
    assert_eq!(sound_slots::sound(&h.world, e, |o| o.hurt, None), None, "the reload drops the removed declaration: nothing leaks");
}
