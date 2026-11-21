use super::*;

#[test]
fn read_encoding_keeps_nonresident_stack_limits() {
    let items = super::stack_limit_tests::catalog();
    let key = mod_api::__private::item_key("minecraft:iron_sword").unwrap();
    let mut damaged = ApiStack::of(key);
    damaged
        .set_components([(JavaString::from("minecraft:max_damage"), Tag::Int(250))].into())
        .unwrap();
    damaged.set_damage(251);
    let mut removed = ApiStack::of(key);
    removed
        .set_components(
            [(
                JavaString::from("!minecraft:max_damage"),
                Tag::Compound(Default::default()),
            )]
            .into(),
        )
        .unwrap();
    removed.set_damage(1);
    let mut excess = ApiStack::of(mod_api::__private::item_key("minecraft:stick").unwrap());
    excess.set_count(65).unwrap();
    for api in [damaged, removed, excess] {
        assert!(api.try_into_engine(&items).is_err(), "write remains strict");
        let encoded = stack_value(&api).unwrap();
        assert_eq!(encoded.count, api.count());
        assert_eq!(encoded.damage, api.damage());
        assert_eq!(encoded.try_into_api().unwrap(), api);
    }
}
