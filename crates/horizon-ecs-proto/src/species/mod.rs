//! The species registry. A species is ONE file in this folder (`species/<name>.rs`, shared bases in
//! `species/templates/<template>.rs`) that holds nothing but its configuration: a `Def` (its `reference`, its sections,
//! its `changes`). `BUILTIN` inscribes them; `Registry` resolves every inheritance once, at startup, and is frozen:
//! `registry()` is read-only afterwards and nothing in the hot path looks anything up by name (an animal's
//! `SpeciesId` is its position in the registry, its modules are resolved at creation).
//!
//! Extending (a mod, or the next species): `install(|b| ...)` before the first `registry()` call; see `Builder`.
//! The framework assumes nothing about the entity: a `Def` with empty sections is a valid entity without goals.
use crate::behaviours::{self, Cap, Entry, Module};
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod chicken;
pub mod cow;
pub mod pig;
pub mod sheep;
pub mod templates;
pub mod zombie;

/// The inscription: the species of the base, in the order of their `SpeciesId`.
pub static BUILTIN: &[&Def] = &[&cow::DEF, &pig::DEF, &zombie::DEF, &sheep::DEF, &chicken::DEF];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Targets,
    Goals,
}
/// One modification of the inherited configuration, applied in order. `module` + `nth` name an entry (the `nth` entry
/// of that module in the resolved list, 0 when it appears once).
#[derive(Clone, Copy)]
pub enum Change {
    /// Appended at the end of the section.
    Add(Section, Entry),
    /// Inserted right after the `nth` entry of `module` (the order of the list is the order of the slots).
    InsertAfter { module: &'static Module, nth: u8, entry: Entry },
    /// Replaces the parameters of the entry (the new ones are complete: the module's `Params` with `..DEFAULT`).
    Set { module: &'static Module, nth: u8, params: &'static (dyn std::any::Any + Sync) },
    Priority { module: &'static Module, nth: u8, priority: u8 },
    Remove { module: &'static Module, nth: u8 },
}
/// One species or template: what it inherits, its own sections (used when it has no `reference`; with one they are
/// empty) and its changes.
pub struct Def {
    pub name: &'static str,
    pub reference: Option<&'static Def>,
    pub targets: &'static [Entry],
    pub goals: &'static [Entry],
    pub changes: &'static [Change],
    /// The capabilities of the entity (`None`: the reference's, or all of `Cap::ALL` at the root). A section the entity
    /// has no capability for is not read; a module that requires a missing capability is a startup error.
    pub caps: Option<&'static [Cap]>,
}
pub const ALL_CAPS: &[Cap] = &[Cap::Body, Cap::Living, Cap::Goals, Cap::Targets, Cap::Locomotion];
impl Def {
    pub const EMPTY: Def = Def { name: "", reference: None, targets: &[], goals: &[], changes: &[], caps: None };
    fn capabilities(&self) -> &'static [Cap] {
        self.caps.or_else(|| self.reference.map(|r| r.capabilities())).unwrap_or(ALL_CAPS)
    }
}

/// What the selectors of an animal are built from: the target entries then the goal entries (same model; a target goal
/// takes no flag), resolved once into the animal's `GoalSet`.
#[derive(Clone, Copy)]
pub struct Config {
    pub entries: &'static [Entry],
}
impl Config {
    /// The parameters of the `nth` entry of the module (for the tests and the tools that read the configuration).
    pub fn param<T: 'static>(&self, m: &'static Module, nth: usize) -> Option<&'static T> {
        let e = self.entries.iter().filter(|e| behaviours::same(e.module, m)).nth(nth)?;
        let p: &'static dyn std::any::Any = e.params;
        p.downcast_ref::<T>()
    }
    pub fn has(&self, m: &'static Module) -> bool {
        self.entries.iter().any(|e| behaviours::same(e.module, m))
    }
}
/// A species once resolved.
pub struct Resolved {
    pub name: &'static str,
    pub config: Config,
    pub caps: &'static [Cap],
}

/// Why the registry could not be built: it names the source, the species and the change.
#[derive(Debug)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The frozen registry.
pub struct Registry {
    pub modules: Vec<&'static Module>,
    pub species: Vec<Resolved>,
    by_name: HashMap<&'static str, usize>,
}
impl Registry {
    /// A module's id: its position (the slot of its systems).
    pub fn module_id(&self, m: &'static Module) -> Option<usize> {
        self.modules.iter().position(|x| behaviours::same(x, m))
    }
    pub fn id(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }
    pub fn by_name(&self, name: &str) -> Result<&Resolved, Error> {
        match self.id(name) {
            Some(i) => Ok(&self.species[i]),
            None => Err(Error(format!("unknown species `{name}` (known: {})", self.species.iter().map(|s| s.name).collect::<Vec<_>>().join(", ")))),
        }
    }
}

/// The building of the registry: the builtins, then what the mods add, in the order of the calls.
pub struct Builder {
    source: &'static str,
    modules: Vec<&'static Module>,
    defs: Vec<(&'static str, &'static Def)>,
    alters: Vec<(&'static str, &'static str, &'static [Change])>,
}
impl Builder {
    fn new() -> Self {
        let mut b = Builder { source: "base", modules: Vec::new(), defs: Vec::new(), alters: Vec::new() };
        for m in behaviours::TABLE {
            b.modules.push(m);
        }
        for d in BUILTIN {
            b.defs.push(("base", d));
        }
        b
    }
    /// The name of the mod that registers what follows (for the errors).
    pub fn source(&mut self, name: &'static str) -> &mut Self {
        self.source = name;
        self
    }
    /// A new behaviour (after the builtins: its systems run after theirs).
    pub fn module(&mut self, m: &'static Module) -> &mut Self {
        self.modules.push(m);
        self
    }
    /// A new species (or template: only species are looked up by name by the world, a template is only a `reference`).
    pub fn species(&mut self, d: &'static Def) -> &mut Self {
        self.defs.push((self.source, d));
        self
    }
    /// Changes applied to an existing species after its resolution.
    pub fn alter(&mut self, species: &'static str, changes: &'static [Change]) -> &mut Self {
        self.alters.push((self.source, species, changes));
        self
    }
    fn freeze(self) -> Result<Registry, Error> {
        let mut species = Vec::new();
        let mut by_name = HashMap::new();
        let mut lists: Vec<(Vec<Entry>, usize)> = Vec::new();
        for (src, d) in &self.defs {
            if by_name.contains_key(d.name) {
                return Err(Error(format!("{src}: species `{}` is declared twice", d.name)));
            }
            let (list, targets) = resolve(src, d, &self.modules)?;
            by_name.insert(d.name, species.len());
            lists.push((list, targets));
            species.push(Resolved { name: d.name, config: Config { entries: &[] }, caps: d.capabilities() });
        }
        for (src, name, changes) in &self.alters {
            let Some(&i) = by_name.get(name) else {
                return Err(Error(format!("{src}: alters `{name}`, which no source declares")));
            };
            let (list, targets) = &mut lists[i];
            apply(src, name, changes, list, targets, &self.modules)?;
        }
        for ((_, d), (list, _)) in self.defs.iter().zip(&lists) {
            let caps = d.capabilities();
            for e in list {
                if let Some(c) = e.module.requires.iter().find(|c| !caps.contains(c)) {
                    return Err(Error(format!("species `{}`: module `{}` requires the capability {c:?}, which the species does not have", d.name, e.module.name)));
                }
            }
        }
        for (r, (list, _)) in species.iter_mut().zip(lists) {
            r.config = Config { entries: Box::leak(list.into_boxed_slice()) };
        }
        Ok(Registry { modules: self.modules, species, by_name })
    }
}
/// The entries of a def (its reference first), the number of those in the targets section.
fn resolve(src: &str, d: &'static Def, modules: &[&'static Module]) -> Result<(Vec<Entry>, usize), Error> {
    let (mut list, mut targets) = match d.reference {
        Some(r) => resolve(src, r, modules)?,
        None => (Vec::new(), 0),
    };
    list.splice(targets..targets, d.targets.iter().copied());
    targets += d.targets.len();
    list.extend(d.goals.iter().copied());
    for e in d.targets.iter().chain(d.goals) {
        known(src, d.name, "section", e.module, modules)?;
    }
    apply(src, d.name, d.changes, &mut list, &mut targets, modules)?;
    Ok((list, targets))
}
fn known(src: &str, species: &str, what: &str, m: &'static Module, modules: &[&'static Module]) -> Result<(), Error> {
    if modules.iter().any(|x| behaviours::same(x, m)) {
        Ok(())
    } else {
        Err(Error(format!("{src}: species `{species}`, {what}: module `{}` is not registered", m.name)))
    }
}
fn apply(src: &str, species: &str, changes: &[Change], list: &mut Vec<Entry>, targets: &mut usize, modules: &[&'static Module]) -> Result<(), Error> {
    let find = |list: &Vec<Entry>, m: &'static Module, nth: u8| list.iter().enumerate().filter(|(_, e)| behaviours::same(e.module, m)).nth(nth as usize).map(|(i, _)| i);
    for (k, c) in changes.iter().enumerate() {
        let fail = |what: &str, m: &'static Module, nth: u8| Error(format!("{src}: species `{species}`, change {k}: {what} `{}` #{nth}", m.name));
        match *c {
            Change::Add(sec, e) => {
                known(src, species, &format!("change {k}"), e.module, modules)?;
                match sec {
                    Section::Goals => list.push(e),
                    Section::Targets => {
                        list.insert(*targets, e);
                        *targets += 1;
                    }
                }
            }
            Change::InsertAfter { module, nth, entry } => {
                known(src, species, &format!("change {k}"), entry.module, modules)?;
                let i = find(list, module, nth).ok_or_else(|| fail("no entry to insert after:", module, nth))?;
                list.insert(i + 1, entry);
                if i < *targets {
                    *targets += 1;
                }
            }
            Change::Set { module, nth, params } => {
                let i = find(list, module, nth).ok_or_else(|| fail("no entry to set:", module, nth))?;
                list[i].params = params;
            }
            Change::Priority { module, nth, priority } => {
                let i = find(list, module, nth).ok_or_else(|| fail("no entry to reprioritise:", module, nth))?;
                list[i].priority = priority;
            }
            Change::Remove { module, nth } => {
                let i = find(list, module, nth).ok_or_else(|| fail("no entry to remove:", module, nth))?;
                list.remove(i);
                if i < *targets {
                    *targets -= 1;
                }
            }
        }
    }
    Ok(())
}

static EXTENSIONS: std::sync::Mutex<Vec<fn(&mut Builder)>> = std::sync::Mutex::new(Vec::new());
static REGISTRY: OnceLock<Registry> = OnceLock::new();
/// Adds what a mod brings; must be called before the first `registry()` (the registry is frozen then).
pub fn install(f: fn(&mut Builder)) -> Result<(), Error> {
    if REGISTRY.get().is_some() {
        return Err(Error("the species registry is frozen".into()));
    }
    EXTENSIONS.lock().unwrap().push(f);
    Ok(())
}
/// Builds (the builtins, then each extension in the order of `install`) without freezing: a startup check.
pub fn build() -> Result<Registry, Error> {
    let mut b = Builder::new();
    for f in EXTENSIONS.lock().unwrap().iter() {
        f(&mut b);
    }
    b.freeze()
}
/// The frozen registry of the process.
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| build().unwrap_or_else(|e| panic!("species registry: {e}")))
}
/// The configuration of a species by its type name; an unknown name is an error, not a default.
pub fn config(name: &str) -> Config {
    registry().by_name(name).unwrap_or_else(|e| panic!("{e}")).config
}
/// Every species of the process, by `SpeciesId`.
pub fn names() -> Vec<&'static str> {
    registry().species.iter().map(|s| s.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::*;

    /// The inherited configurations are the flat lists they replaced (module, priority and the parameters that differ).
    #[test]
    fn the_resolved_species_equal_their_flat_configuration() {
        type Flat = Vec<(&'static Module, u8)>;
        let flat = |name: &str| -> Flat {
            let f = |m: &'static Module, p: u8| (m, p);
            match name {
                "minecraft:cow" => vec![f(&float::MODULE, 0), f(&panic::MODULE, 1), f(&breed::MODULE, 2), f(&tempt::MODULE, 3), f(&follow_parent::MODULE, 4), f(&stroll::MODULE, 5), f(&look_at_player::MODULE, 6), f(&random_look::MODULE, 7)],
                "minecraft:pig" => vec![f(&float::MODULE, 0), f(&panic::MODULE, 1), f(&breed::MODULE, 3), f(&tempt::MODULE, 4), f(&tempt::MODULE, 4), f(&follow_parent::MODULE, 5), f(&stroll::MODULE, 6), f(&look_at_player::MODULE, 7), f(&random_look::MODULE, 8)],
                "minecraft:sheep" => vec![f(&float::MODULE, 0), f(&panic::MODULE, 1), f(&breed::MODULE, 2), f(&tempt::MODULE, 3), f(&follow_parent::MODULE, 4), f(&eat_block::MODULE, 5), f(&stroll::MODULE, 6), f(&look_at_player::MODULE, 7), f(&random_look::MODULE, 8)],
                "minecraft:chicken" => vec![f(&float::MODULE, 0), f(&panic::MODULE, 1), f(&breed::MODULE, 2), f(&tempt::MODULE, 3), f(&follow_parent::MODULE, 4), f(&stroll::MODULE, 5), f(&look_at_player::MODULE, 6), f(&random_look::MODULE, 7)],
                _ => vec![f(&hurt_by::MODULE, 1), f(&nearest_target::MODULE, 2), f(&melee::MODULE, 3), f(&stroll::MODULE, 7), f(&look_at_player::MODULE, 8), f(&random_look::MODULE, 8)],
            }
        };
        for name in names() {
            let got: Flat = config(name).entries.iter().map(|e| (e.module, e.priority)).collect();
            let want = flat(name);
            assert_eq!(got.len(), want.len(), "{name}");
            for (g, w) in got.iter().zip(&want) {
                assert!(same(g.0, w.0) && g.1 == w.1, "{name}: {} {} instead of {} {}", g.0.name, g.1, w.0.name, w.1);
            }
        }
        let c = |n| config(n);
        assert_eq!(c("minecraft:pig").param::<tempt::Params>(&tempt::MODULE, 1).unwrap().items, &["minecraft:carrot", "minecraft:potato", "minecraft:beetroot"]);
        assert_eq!(c("minecraft:pig").param::<tempt::Params>(&tempt::MODULE, 0).unwrap().items, &["minecraft:carrot_on_a_stick"]);
        assert_eq!(c("minecraft:chicken").param::<panic::Params>(&panic::MODULE, 0).unwrap().speed, 1.4);
        assert_eq!(c("minecraft:sheep").param::<follow_parent::Params>(&follow_parent::MODULE, 0).unwrap().speed, 1.1);
        assert_eq!(c("minecraft:cow").param::<tempt::Params>(&tempt::MODULE, 0).unwrap().speed, 1.25);
    }
    #[test]
    fn a_module_without_its_capability_is_a_startup_error() {
        static NO_LOCOMOTION: Def = Def { name: "mod:post", reference: Some(&cow::DEF), caps: Some(&[Cap::Body, Cap::Living, Cap::Goals]), ..Def::EMPTY };
        let mut b = Builder::new();
        b.species(&NO_LOCOMOTION);
        let e = b.freeze().err().expect("an error").to_string();
        assert!(e.contains("mod:post") && e.contains("Locomotion"), "{e}");
    }
    #[test]
    fn an_unknown_species_and_a_bad_change_are_errors_that_name_their_source() {
        assert!(registry().by_name("minecraft:nope").is_err());
        static BAD: Def = Def { name: "mod:bad", reference: Some(&cow::DEF), changes: &[super::Change::Remove { module: &melee::MODULE, nth: 0 }], ..Def::EMPTY };
        let mut b = Builder::new();
        b.source("a-mod").species(&BAD);
        let e = b.freeze().err().expect("an error").to_string();
        assert!(e.contains("a-mod") && e.contains("mod:bad") && e.contains("change 0"), "{e}");
    }
}
