//! The guard of "a component never knows the entity it is on": no species name (`minecraft:cow`, ...) in the framework. The
//! names are those of the registry (read from the configuration, not typed here); the folder `species/` (the configuration),
//! `bin/` and the test modules of a file (after its `#[cfg(test)]`) may name them. The ALLOWANCE below is the debt of the
//! conformity work: the count of names per file may only go DOWN (a new name in the framework fails; a file that cleans up
//! lowers its number here).
use std::{collections::BTreeMap, fs, path::Path};

/// File -> names still tolerated (the conformity work removes them: rules, then attributes; see AUDIT-ESPECES.md).
const ALLOWANCE: &[(&str, usize)] = &[];

fn scan(dir: &Path, names: &[&str], out: &mut BTreeMap<String, usize>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            let n = p.file_name().unwrap().to_string_lossy().to_string();
            if n != "species" && n != "bin" {
                scan(&p, names, out);
            }
            continue;
        }
        if p.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = fs::read_to_string(&p).unwrap();
        // The tests of a file come after its `#[cfg(test)]`.
        let body = text.split("#[cfg(test)]").next().unwrap();
        let mut count = 0;
        for line in body.lines() {
            let code = line.split("//").next().unwrap();
            count += names.iter().map(|n| code.matches(n).count()).sum::<usize>();
        }
        if count > 0 {
            out.insert(p.file_name().unwrap().to_string_lossy().to_string(), count);
        }
    }
}

#[test]
fn no_species_name_appears_in_the_framework_beyond_the_allowance() {
    let names: Vec<String> = horizon_ecs_proto::species::names().iter().map(|n| format!("\"{n}\"")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut found = BTreeMap::new();
    scan(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &names, &mut found);
    let allowed: BTreeMap<&str, usize> = ALLOWANCE.iter().copied().collect();
    let mut bad = Vec::new();
    for (file, n) in &found {
        let max = allowed.get(file.as_str()).copied().unwrap_or(0);
        if *n > max {
            bad.push(format!("{file}: {n} species names (allowed {max})"));
        }
    }
    eprintln!("species names in the framework, by file: {found:?}");
    assert!(bad.is_empty(), "a species name in the framework (the entity must not be known by its components): {bad:?}");
}
