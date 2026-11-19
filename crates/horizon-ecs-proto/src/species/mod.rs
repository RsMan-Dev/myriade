//! The species registry. A species is ONE file in this folder (`species/<name>.rs`, shared bases in
//! `species/templates/<template>.rs`) that holds nothing but its configuration: a `Def` (its `reference`, its sections,
//! its `changes`). `BUILTIN` inscribes them; `Registry` resolves every inheritance once, at startup, and is frozen:
//! `registry()` is read-only afterwards and nothing in the hot path looks anything up by name (an animal's
//! `SpeciesId` is its position in the registry, its modules are resolved at creation).
//!
//! Extending (a mod, or the next species): `install(|b| ...)` before the first `registry()` call; see `Builder`.
//! The framework assumes nothing about the entity: a `Def` with empty sections is a valid entity without goals.
use crate::behaviours::{self, Cap, Entry, Module, Pred};
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod chicken;
pub mod copper_golem;
pub mod cow;
pub mod pig;
pub mod predicates;
pub mod skeleton;
pub mod sheep;
pub mod templates;
pub mod horse;
pub mod blaze;
pub mod witch;
pub mod iron_golem;
pub mod magma_cube;
pub mod rabbit;
pub mod slime;
pub mod wolf;
pub mod cat;
pub mod ocelot;
pub mod llama;
pub mod zombie;
pub mod cod;
pub mod parrot;
pub mod ghast;
pub mod bat;
pub mod strider;
pub mod turtle;
pub mod vex;
pub mod squid;
pub mod bee;
pub mod dolphin;
pub mod axolotl;
pub mod drowned;
pub mod spider;
pub mod cave_spider;
pub mod glow_squid;
pub mod phantom;
pub mod armadillo;
pub mod creeper;
pub mod endermite;
pub mod enderman;
pub mod silverfish;
pub mod piglin_brute;
pub mod panda;
pub mod zoglin;
pub mod mooshroom;
pub mod husk;
pub mod zombie_villager;
pub mod zombified_piglin;
pub mod donkey;
pub mod mule;
pub mod skeleton_horse;
pub mod zombie_horse;
pub mod salmon;
pub mod tropical_fish;
pub mod pufferfish;
pub mod tadpole;
pub mod snow_golem;

/// The inscription: the species of the base, in the order of their `SpeciesId`.
/// The species a process builds its level for when `HORIZON_SPECIES` is not set.
pub const PROCESS_DEFAULT: &str = "minecraft:cow";
pub static BUILTIN: &[&Def] = &[&cow::DEF, &pig::DEF, &zombie::DEF, &sheep::DEF, &chicken::DEF, &skeleton::DEF, &copper_golem::DEF, &cod::DEF, &parrot::DEF, &ghast::DEF, &wolf::DEF,
    &cat::DEF,
    &ocelot::DEF, &llama::DEF,
    &armadillo::DEF,
    &creeper::DEF,
    &endermite::DEF,
    &enderman::DEF,
    &silverfish::DEF,
    &piglin_brute::DEF,
    &panda::DEF,
    &zoglin::DEF,
    &mooshroom::DEF,
    &husk::DEF,
    &zombie_villager::DEF,
    &zombified_piglin::DEF,
    &horse::DEF,
    &donkey::DEF,
    &mule::DEF,
    &skeleton_horse::DEF,
    &zombie_horse::DEF,
    &salmon::DEF,
    &tropical_fish::DEF,
    &pufferfish::DEF,
    &tadpole::DEF,
    &snow_golem::DEF,
    &rabbit::DEF,
    &turtle::DEF,
    &squid::DEF,
    &bee::DEF,
    &dolphin::DEF,
    &axolotl::DEF,
    &drowned::DEF,
    &spider::DEF,
    &cave_spider::DEF,
    &glow_squid::DEF,
    &vex::DEF,
    &phantom::DEF,
    &slime::DEF,
    &magma_cube::DEF,
    &bat::DEF,
    &strider::DEF,
    &iron_golem::DEF,
    &blaze::DEF,
    &witch::DEF,
];

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
    /// The rules (not goals) of the species: `rules.rs`. With a `reference`, its rules come first.
    pub rules: &'static [crate::rules::RuleEntry],
    /// The capabilities of the entity (`None`: the reference's, or all of `Cap::ALL` at the root). A section the entity
    /// has no capability for is not read; a module that requires a missing capability is a startup error.
    pub caps: Option<&'static [Cap]>,
    /// The brain (own, or the reference's when empty).
    pub brain: crate::brain::BrainDef,
    /// The modes of locomotion by medium (`locomotion::Medium`): empty is the reference's, or the walk at the root.
    pub locomotion: &'static [crate::locomotion::ModeEntry],
}
pub const ALL_CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion];
/// A capability that supposes others (declared, checked when the registry is frozen): the dependency, not a guess.
const NEEDS: &[(Cap, Cap)] = &[(Cap::Body, Cap::Transform), (Cap::Body, Cap::Solid), (Cap::Living, Cap::Body), (Cap::Locomotion, Cap::Body), (Cap::Tamable, Cap::Transform), (Cap::Rideable, Cap::Body)];
impl Def {
    pub const EMPTY: Def = Def { name: "", reference: None, targets: &[], goals: &[], changes: &[], rules: &[], caps: None, brain: crate::brain::BrainDef::EMPTY, locomotion: &[] };
    fn all_rules(&self) -> Vec<crate::rules::RuleEntry> {
        let mut v = self.reference.map_or_else(Vec::new, Def::all_rules);
        v.extend_from_slice(self.rules);
        v
    }
    /// The modes of locomotion: its own, else its reference's, else the walk.
    fn modes(&self) -> &'static [crate::locomotion::ModeEntry] {
        if !self.locomotion.is_empty() {
            self.locomotion
        } else if !self.capabilities().contains(&Cap::Locomotion) {
            // An entity that does not move by itself (a mannequin, a display): nothing is imposed, not even the walk.
            &[]
        } else {
            self.reference.map_or(crate::locomotion::DEFAULT_MODES, |r| r.modes())
        }
    }
    fn brain_def(&self) -> crate::brain::BrainDef {
        if self.brain.is_empty() { self.reference.map_or(crate::brain::BrainDef::EMPTY, |r| r.brain_def()) } else { self.brain }
    }
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
    pub rules: &'static [crate::rules::RuleEntry],
    pub caps: &'static [Cap],
    pub brain: Option<&'static crate::brain::Plan>,
    /// The modes of locomotion, inherited like the sections.
    pub modes: &'static [crate::locomotion::ModeEntry],
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
    predicates: HashMap<&'static str, Pred>,
}
impl Registry {
    /// A module's id: its position (the slot of its systems).
    pub fn module_id(&self, m: &'static Module) -> Option<usize> {
        self.modules.iter().position(|x| behaviours::same(x, m))
    }
    /// Whether some resolved species has the module of rank `module`: its systems and resources exist only then (the cost of a
    /// module nobody carries is zero). `REGISTER_ALL=1` registers every module (to measure what the guarantee saves).
    pub fn carried(&self, module: usize) -> bool {
        // The unit tests of a module inject their own configuration into the library under test: every module is inscribed there.
        cfg!(test)
            || EVERY_MODULE.load(std::sync::atomic::Ordering::Relaxed)
            || std::env::var_os("REGISTER_ALL").is_some()
            || self.species.iter().any(|s| s.config.entries.iter().chain(s.modes.iter().map(|m| &m.1)).any(|e| self.module_id(e.module) == Some(module)))
    }
    /// The named predicate (`Cond::Is`).
    pub fn predicate(&self, name: &str) -> Option<Pred> {
        self.predicates.get(name).copied()
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
    predicates: Vec<(&'static str, &'static str, Pred)>,
}
impl Builder {
    fn new() -> Self {
        let mut b = Builder { source: "base", modules: Vec::new(), defs: Vec::new(), alters: Vec::new(), predicates: Vec::new() };
        for (n, f) in predicates::BUILTIN {
            b.predicates.push(("base", n, *f));
        }
        for m in behaviours::TABLE.iter().chain(crate::locomotion::TABLE) {
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
    /// A named predicate for `Cond::Is` (name it `yourmod:what`).
    pub fn predicate(&mut self, name: &'static str, f: Pred) -> &mut Self {
        self.predicates.push((self.source, name, f));
        self
    }
    fn freeze(self) -> Result<Registry, Error> {
        let mut preds: HashMap<&'static str, Pred> = HashMap::new();
        for (src, name, f) in &self.predicates {
            if preds.insert(name, *f).is_some() {
                return Err(Error(format!("{src}: predicate `{name}` is declared twice")));
            }
        }
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
            species.push(Resolved { name: d.name, config: Config { entries: &[] }, rules: Box::leak(d.all_rules().into_boxed_slice()), caps: d.capabilities(), brain: None, modes: d.modes() });
        }
        for (src, name, changes) in &self.alters {
            let Some(&i) = by_name.get(name) else {
                return Err(Error(format!("{src}: alters `{name}`, which no source declares")));
            };
            let (list, targets) = &mut lists[i];
            apply(src, name, changes, list, targets, &self.modules)?;
        }
        for (src, d) in &self.defs {
            for (_, e) in d.modes() {
                known(src, d.name, "locomotion", e.module, &self.modules)?;
                if let Some(c) = e.module.requires.iter().find(|c| !d.capabilities().contains(c)) {
                    return Err(Error(format!("species `{}`: mode `{}` requires the capability {c:?}, which the species does not have", d.name, e.module.name)));
                }
            }
        }
        for ((_, d), (list, _)) in self.defs.iter().zip(&lists) {
            let caps = d.capabilities();
            if let Some((a, b)) = NEEDS.iter().find(|(a, b)| caps.contains(a) && !caps.contains(b)) {
                return Err(Error(format!("species `{}`: the capability {a:?} needs {b:?}, which it does not declare", d.name)));
            }
            if list.len() > crate::goals::MAX_ENTRIES {
                return Err(Error(format!("species `{}`: {} entries (goals and targets), the selector holds {} at most", d.name, list.len(), crate::goals::MAX_ENTRIES)));
            }
            for e in list {
                if let Some(c) = e.module.requires.iter().find(|c| !caps.contains(c)) {
                    return Err(Error(format!("species `{}`: module `{}` requires the capability {c:?}, which the species does not have", d.name, e.module.name)));
                }
            }
        }
        for ((src, d), (list, _)) in self.defs.iter().zip(&lists) {
            for e in list {
                let mut names = Vec::new();
                if let Some(c) = e.when {
                    c.names(&mut names);
                }
                if let Some(n) = names.iter().find(|n| !preds.contains_key(*n)) {
                    return Err(Error(format!("{src}: species `{}`, module `{}`: unknown predicate `{n}`", d.name, e.module.name)));
                }
            }
        }
        for ((_, d), r) in self.defs.iter().zip(species.iter_mut()) {
            let def = d.brain_def();
            if !def.is_empty() {
                let plan = crate::brain::Plan::build(&def, r.caps).map_err(|m| Error(format!("species `{}`: brain: {m}", d.name)))?;
                r.brain = Some(Box::leak(Box::new(plan)));
            }
        }
        for (r, (list, _)) in species.iter_mut().zip(lists) {
            r.config = Config { entries: Box::leak(list.into_boxed_slice()) };
        }
        Ok(Registry { modules: self.modules, species, by_name, predicates: preds })
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

static EVERY_MODULE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Inscribes the systems of EVERY registered module, carried or not (before the first `registry()`): what the tests of a module
/// that inject their own configuration need; `tests/common` does it for every integration test.
pub fn inscribe_every_module() {
    EVERY_MODULE.store(true, std::sync::atomic::Ordering::Relaxed);
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

    /// What every species resolves to (module, priority, in order), written out by hand when the species was verified: the
    /// inheritance (`reference`, `changes`) must give exactly these lists. ALL the species are checked and ALL the differences
    /// are listed; a species of the registry that is not here is a difference too (add its line when you add the species).
    const RESOLVED: &[(&str, &[(&str, u8)])] = &[
        ("minecraft:cow", &[("float", 0), ("panic", 1), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("stroll", 5), ("look_at_player", 6), ("random_look", 7)]),
        ("minecraft:pig", &[("float", 0), ("panic", 1), ("breed", 3), ("tempt", 4), ("tempt", 4), ("follow_parent", 5), ("stroll", 6), ("look_at_player", 7), ("random_look", 8)]),
        ("minecraft:zombie", &[("hurt_by", 1), ("nearest_target", 2), ("melee", 3), ("attack_turtle_egg", 4), ("stroll", 7), ("look_at_player", 8), ("random_look", 8)]),
        ("minecraft:sheep", &[("float", 0), ("panic", 1), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("eat_block", 5), ("stroll", 6), ("look_at_player", 7), ("random_look", 8)]),
        ("minecraft:chicken", &[("float", 0), ("panic", 1), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("stroll", 5), ("look_at_player", 6), ("random_look", 7)]),
        ("minecraft:skeleton", &[("hurt_by", 1), ("nearest_target", 2), ("nearest_target", 3), ("nearest_target", 3), ("stroll", 5), ("look_at_player", 6), ("random_look", 6), ("restrict_sun", 2), ("ranged_bow", 4), ("flee_sun", 3), ("avoid_entity", 3)]),
        ("minecraft:copper_golem", &[]),
        ("minecraft:cod", &[("panic", 0), ("avoid_entity", 2), ("fish_swim", 4), ("follow_flock_leader", 5)]),
        ("minecraft:salmon", &[("panic", 0), ("avoid_entity", 2), ("fish_swim", 4), ("follow_flock_leader", 5)]),
        ("minecraft:tropical_fish", &[("panic", 0), ("avoid_entity", 2), ("fish_swim", 4), ("follow_flock_leader", 5)]),
        ("minecraft:pufferfish", &[("panic", 0), ("avoid_entity", 2), ("fish_swim", 4), ("pufferfish_puff", 1)]),
        ("minecraft:tadpole", &[("panic", 0), ("avoid_entity", 2), ("fish_swim", 4)]),
        ("minecraft:snow_golem", &[("nearest_target", 1), ("ranged_attack", 1), ("stroll", 2), ("snow_trail", 255), ("look_at_player", 3), ("random_look", 4)]),
        ("minecraft:rabbit", &[("float", 1), ("climb_on_powder_snow", 1), ("panic", 1), ("breed", 2), ("tempt", 3), ("avoid_entity", 4), ("avoid_entity", 4), ("avoid_entity", 4), ("raid_garden", 5), ("stroll", 6), ("look_at_player", 11)]),
        ("minecraft:spider", &[("hurt_by", 1), ("nearest_target", 2), ("float", 1), ("leap_at_target", 3), ("melee", 4), ("stroll", 5), ("look_at_player", 6), ("random_look", 6)]),
        ("minecraft:cave_spider", &[("hurt_by", 1), ("nearest_target", 2), ("float", 1), ("leap_at_target", 3), ("melee", 4), ("stroll", 5), ("look_at_player", 6), ("random_look", 6)]),
        ("minecraft:dolphin", &[]),
        ("minecraft:axolotl", &[]),
        ("minecraft:drowned", &[]),
        ("minecraft:bee", &[("hurt_by", 1), ("nearest_target", 2), ("neutral_anger", 255), ("melee", 0), ("bee_enter_hive", 1), ("breed", 2), ("tempt", 3), ("bee_validate_hive", 3), ("bee_validate_flower", 3), ("bee_pollinate", 4), ("follow_parent", 5), ("bee_locate_hive", 5), ("bee_go_to_hive", 5), ("bee_go_to_known_flower", 6), ("bee_grow_crop", 7), ("fly_stroll", 8)]),
        ("minecraft:squid", &[("squid_random_movement", 0), ("squid_flee", 1)]),
        ("minecraft:glow_squid", &[("squid_random_movement", 0), ("squid_flee", 1)]),
        ("minecraft:vex", &[("hurt_by", 1), ("vex_copy_owner_target", 2), ("nearest_target", 3), ("float", 0), ("vex_charge", 4), ("vex_random_move", 8)]),
        ("minecraft:phantom", &[("phantom_attack_player_target", 1), ("phantom_attack_strategy", 1), ("phantom_sweep_attack", 2), ("phantom_circle_around_anchor", 3)]),
        ("minecraft:turtle", &[("panic", 0), ("turtle_lay_egg", 1), ("tempt", 2), ("turtle_go_to_water", 3), ("turtle_go_home", 4), ("turtle_travel", 7), ("look_at_player", 8), ("turtle_stroll", 9)]),
        ("minecraft:parrot", &[("panic", 0), ("float", 0), ("look_at_player", 1), ("sit_when_ordered", 2), ("follow_owner", 2), ("fly_stroll", 2), ("follow_mob", 3)]),
        ("minecraft:ghast", &[("float_around", 5), ("ghast_look", 7), ("charged_shot", 7)]),
        ("minecraft:bat", &[("bat_flutter", 0)]),
        ("minecraft:strider", &[("panic", 1), ("breed", 2), ("tempt", 3), ("strider_go_to_lava", 4), ("follow_parent", 5), ("stroll", 7), ("look_at_player", 8), ("random_look", 8)]),
        ("minecraft:turtle", &[("turtle_go_to_water", 3), ("turtle_travel", 7), ("turtle_stroll", 9)]),
        ("minecraft:llama", &[("float", 0), ("panic", 3), ("breed", 4), ("tempt", 5), ("follow_parent", 6), ("stroll", 7), ("look_at_player", 8), ("random_look", 9)]),
        ("minecraft:cat", &[("nearest_target", 1), ("nearest_target", 1), ("float", 1), ("panic", 1), ("sit_when_ordered", 2), ("cat_relax_on_owner", 3), ("tempt", 4), ("cat_lie_on_block", 5), ("follow_owner", 6), ("cat_sit_on_block", 7), ("leap_at_target", 8), ("ocelot_attack", 9), ("breed", 10), ("stroll", 11), ("look_at_player", 12), ("avoid_entity", 4)]),
        ("minecraft:ocelot", &[("nearest_target", 1), ("nearest_target", 1), ("float", 1), ("tempt", 3), ("leap_at_target", 7), ("ocelot_attack", 8), ("breed", 9), ("stroll", 10), ("look_at_player", 11), ("avoid_entity", 4)]),
        ("minecraft:wolf", &[("owner_target", 1), ("owner_target", 2), ("hurt_by", 3), ("nearest_target", 4), ("nearest_target", 5), ("nearest_target", 6), ("nearest_target", 7), ("float", 1), ("panic", 1), ("sit_when_ordered", 2), ("avoid_entity", 3), ("forget_target", 3), ("leap_at_target", 4), ("melee", 5), ("follow_owner", 6), ("breed", 7), ("stroll", 8), ("beg", 9), ("look_at_player", 10), ("random_look", 10)]),
        ("minecraft:armadillo", &[("float", 0), ("panic", 1), ("rolled", 2), ("breed", 3), ("tempt", 4), ("baby_follow_adult", 4), ("stroll", 5), ("look_at_player", 6), ("random_look", 7)]),
        ("minecraft:creeper", &[("nearest_target", 1), ("hurt_by", 2), ("float", 1), ("swell", 2), ("avoid_entity", 3), ("avoid_entity", 3), ("melee", 4), ("stroll", 5), ("look_at_player", 6), ("random_look", 6)]),
        ("minecraft:endermite", &[("hurt_by", 1), ("nearest_target", 2), ("float", 1), ("climb_on_powder_snow", 1), ("melee", 2), ("stroll", 3), ("look_at_player", 7), ("random_look", 8)]),
        ("minecraft:enderman", &[("look_for_player", 1), ("hurt_by", 2), ("float", 0), ("freeze_when_looked_at", 1), ("melee", 2), ("stroll", 7), ("look_at_player", 8), ("random_look", 8), ("leave_block", 10), ("take_block", 11), ("ender_teleport", 255)]),
        ("minecraft:silverfish", &[("hurt_by", 1), ("nearest_target", 2), ("float", 1), ("climb_on_powder_snow", 1), ("silverfish_wake_up_friends", 3), ("melee", 4), ("silverfish_merge", 5)]),
        ("minecraft:piglin_brute", &[("hurt_by", 1), ("nearest_target", 2), ("nearest_target", 3), ("float", 0), ("melee", 2), ("brute_stroll_home", 5), ("stroll", 5), ("look_at_player", 6)]),
        ("minecraft:panda", &[("hurt_by", 1), ("float", 0), ("panic", 2), ("melee", 3), ("tempt", 4), ("avoid_entity", 6), ("avoid_entity", 6), ("panda_sit", 7), ("panda_lie_on_back", 8), ("panda_sneeze", 8), ("random_look", 10), ("panda_roll", 12), ("follow_parent", 13), ("stroll", 14)]),
        ("minecraft:zoglin", &[("hurt_by", 1), ("nearest_target", 2), ("nearest_target", 3), ("float", 0), ("melee", 2), ("stroll", 5), ("look_at_player", 6)]),
        ("minecraft:mooshroom", &[("float", 0), ("panic", 1), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("stroll", 5), ("look_at_player", 6), ("random_look", 7)]),
        ("minecraft:husk", &[("hurt_by", 1), ("nearest_target", 2), ("melee", 3), ("attack_turtle_egg", 4), ("stroll", 7), ("look_at_player", 8), ("random_look", 8)]),
        ("minecraft:zombie_villager", &[("hurt_by", 1), ("nearest_target", 2), ("melee", 3), ("attack_turtle_egg", 4), ("stroll", 7), ("look_at_player", 8), ("random_look", 8)]),
        ("minecraft:zombified_piglin", &[("hurt_by", 1), ("nearest_target", 2), ("neutral_anger", 255), ("melee", 2), ("attack_turtle_egg", 4), ("stroll", 7), ("look_at_player", 8), ("random_look", 8)]),
        ("minecraft:horse", &[("float", 0), ("panic", 1), ("run_around_like_crazy", 1), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("stroll", 6), ("look_at_player", 7), ("random_look", 8), ("random_stand", 9)]),
        ("minecraft:donkey", &[("float", 0), ("panic", 1), ("run_around_like_crazy", 1), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("stroll", 6), ("look_at_player", 7), ("random_look", 8), ("random_stand", 9)]),
        ("minecraft:mule", &[("float", 0), ("panic", 1), ("run_around_like_crazy", 1), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("stroll", 6), ("look_at_player", 7), ("random_look", 8), ("random_stand", 9)]),
        ("minecraft:skeleton_horse", &[("run_around_like_crazy", 1), ("breed", 2), ("follow_parent", 4), ("stroll", 6), ("look_at_player", 7), ("random_look", 8), ("random_stand", 9)]),
        ("minecraft:slime", &[("float", 1)]),
        ("minecraft:magma_cube", &[("float", 1)]),
        ("minecraft:zombie_horse", &[("run_around_like_crazy", 1), ("float", 0), ("breed", 2), ("tempt", 3), ("follow_parent", 4), ("stroll", 6), ("look_at_player", 7), ("random_look", 8), ("random_stand", 9)]),
        ("minecraft:witch", &[("hurt_by", 1), ("nearest_target", 3), ("float", 1), ("ranged_attack", 2), ("stroll", 2), ("look_at_player", 3), ("random_look", 3)]),
        ("minecraft:blaze", &[("hurt_by", 1), ("nearest_target", 2), ("burst_shot", 4), ("stroll", 7), ("look_at_player", 8), ("random_look", 8)]),
        ("minecraft:iron_golem", &[("hurt_by", 2), ("melee", 1), ("move_towards_target", 2), ("village_walk", 2), ("village_walk", 4), ("look_at_player", 7), ("random_look", 8)]),
    ];
    #[test]
    fn the_resolved_species_equal_their_expected_configuration() {
        let mut diffs: Vec<String> = Vec::new();
        for name in names() {
            let got: Vec<(&str, u8)> = config(name).entries.iter().map(|e| (e.module.name, e.priority)).collect();
            if got.len() > crate::goals::MAX_ENTRIES {
                diffs.push(format!("{name}: {} entries, the selector holds {}", got.len(), crate::goals::MAX_ENTRIES));
            }
            match RESOLVED.iter().find(|r| r.0 == name) {
                None => diffs.push(format!("{name}: not in the expected table (resolves to {got:?})")),
                Some((_, want)) if got != *want => diffs.push(format!("{name}: expected {want:?}, resolved {got:?}")),
                _ => {}
            }
        }
        for (name, _) in RESOLVED {
            if !names().contains(name) {
                diffs.push(format!("{name}: in the expected table, not in the registry"));
            }
        }
        let c = |n| config(n);
        let check = |ok: bool, what: &str, diffs: &mut Vec<String>| {
            if !ok {
                diffs.push(what.to_string());
            }
        };
        check(c("minecraft:pig").param::<tempt::Params>(&tempt::MODULE, 1).is_some_and(|p| p.items == ["minecraft:carrot", "minecraft:potato", "minecraft:beetroot"]), "pig: second tempt goal's foods", &mut diffs);
        check(c("minecraft:pig").param::<tempt::Params>(&tempt::MODULE, 0).is_some_and(|p| p.items == ["minecraft:carrot_on_a_stick"]), "pig: first tempt goal's item", &mut diffs);
        check(c("minecraft:chicken").param::<panic::Params>(&panic::MODULE, 0).is_some_and(|p| p.speed == 1.4), "chicken: panic speed", &mut diffs);
        check(c("minecraft:sheep").param::<follow_parent::Params>(&follow_parent::MODULE, 0).is_some_and(|p| p.speed == 1.1), "sheep: follow speed", &mut diffs);
        check(c("minecraft:cow").param::<tempt::Params>(&tempt::MODULE, 0).is_some_and(|p| p.speed == 1.25), "cow: tempt speed", &mut diffs);
        assert!(diffs.is_empty(), "{} differences:\n{}", diffs.len(), diffs.join("\n"));
    }
    #[test]
    fn a_module_without_its_capability_is_a_startup_error() {
        static NO_LOCOMOTION: Def = Def { name: "mod:post", reference: Some(&cow::DEF), caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals]), ..Def::EMPTY };
        let mut b = Builder::new();
        b.species(&NO_LOCOMOTION);
        let e = b.freeze().err().expect("an error").to_string();
        assert!(e.contains("mod:post") && e.contains("Locomotion"), "{e}");
    }
    #[test]
    fn an_unknown_predicate_is_a_startup_error_naming_species_and_module() {
        use crate::behaviours::Cond;
        static ENTRY: [Entry; 1] = [Entry::new(&melee::MODULE, 3, &melee::Params::DEFAULT).when(&Cond::Not(&Cond::Is("mod:nope")))];
        static BAD: Def = Def { name: "mod:cond", reference: None, goals: &ENTRY, ..Def::EMPTY };
        let mut b = Builder::new();
        b.source("a-mod").species(&BAD);
        let e = b.freeze().err().expect("an error").to_string();
        assert!(e.contains("a-mod") && e.contains("mod:cond") && e.contains("melee") && e.contains("mod:nope"), "{e}");
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
    /// A species with more entries than the selector holds is an error of the registry at the freeze, not a crash at the first tick.
    #[test]
    fn a_species_with_too_many_entries_is_an_error_at_the_freeze() {
        use crate::behaviours::{float, Entry};
        static MANY: [Entry; 33] = [Entry::new(&float::MODULE, 1, &float::Params::DEFAULT); 33];
        static BIG: Def = Def { name: "mod:big", goals: &MANY, ..Def::EMPTY };
        let mut b = Builder::new();
        b.source("a-mod").species(&BIG);
        let e = b.freeze().err().expect("an error").to_string();
        assert!(e.contains("mod:big") && e.contains("33") && e.contains("32"), "{e}");
    }
}
