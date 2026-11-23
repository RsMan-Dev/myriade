//! Règles du fermier, reprises de `villager_farming_tests.rs` (valeurs inchangées) : la récolte n'a lieu qu'à l'âge max du modèle
//! vanilla de la culture, classée par sa classe (mods compris), et la graine plantée est le premier objet taggé de l'inventaire.
use horizon_ecs_proto::rules::farm::{crop_max_age, first_seed, is_ripe};

#[test]
fn mod_crops_are_harvested_only_at_their_vanilla_templates_max_age() {
    // (cas, CropBlock, BeetrootBlock, TorchflowerCropBlock, âge, mûre) : blé 6 non, 7 oui ; betterave 3 oui ; torche 1 non, 2 oui ;
    // pitcher (pas un CropBlock) ; pierre.
    let cases = [
        ("wheat template at 6", true, false, false, Some(6), false),
        ("wheat template at 7", true, false, false, Some(7), true),
        ("beetroot template at 3", true, true, false, Some(3), true),
        ("torchflower template at 1", true, false, true, Some(1), false),
        ("torchflower template at 2", true, false, true, Some(2), true),
        ("pitcher crop, not a CropBlock", false, false, false, Some(4), false),
        ("stone", false, false, false, None, false),
    ];
    for (case, crop, beetroot, torchflower, age, ripe) in cases {
        assert_eq!(is_ripe(crop_max_age(crop, beetroot, torchflower), age), ripe, "{case}");
    }
}

#[test]
fn farmer_sows_the_first_tagged_block_item_and_consumes_one_seed() {
    let plants = |item: &str| matches!(item, "fieldtest:grain" | "fieldtest:roots" | "fieldtest:seeds");
    let stock = vec![
        ("minecraft:wheat_seeds".to_string(), 2),
        ("minecraft:stick".to_string(), 2),
        ("fieldtest:flower".to_string(), 2),
        ("fieldtest:roots".to_string(), 1),
        ("fieldtest:grain".to_string(), 2),
    ];
    assert_eq!(first_seed(&stock, plants), Some(3), "the first tagged block item: roots");
    let empty = vec![("fieldtest:roots".to_string(), 0)];
    assert_eq!(first_seed(&empty, plants), None, "an empty stack is no seed");
}
