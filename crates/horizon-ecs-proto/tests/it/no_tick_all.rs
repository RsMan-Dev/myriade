//! A module's tick is its `Module::tick` field, called by `arbitrate` for the modules some animal carries: a free-standing `tick_all`
//! that something must remember to call is a module that does nothing when nobody calls it (the cube attack of lot 5 was one). The
//! source of the behaviours may not define one.
#[test]
fn no_behaviour_defines_a_tick_all() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/behaviours");
    let mut bad = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or("");
            if code.contains("fn tick_all") {
                bad.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    assert!(bad.is_empty(), "a `fn tick_all` not wired by the framework: use `Module {{ tick: Some(f), .. }}`: {bad:?}");
}
