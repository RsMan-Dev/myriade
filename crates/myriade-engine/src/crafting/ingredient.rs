//! Engine recipe ingredients and stack matching.
#[path = "ingredient/components.rs"]
mod components;
#[path = "ingredient/display.rs"]
mod display;
#[path = "ingredient/matching.rs"]
mod matching;
use super::{ItemStack, Items};
use crate::native_stack_hooks::ingredient_test as native_ingredient_test;
use display::display_stacks;
use horizon_world::native_load_context::LoadContext;
pub use myriade_types::{IngredientRule, Shape};
use serde_json::Value;
use std::collections::HashSet;
/// Where `Ingredient::resolve` leaves the hook's answer inside the occurrence
/// it resolved. Written and read only here; the loads that own the sources
/// never send or save it.
const RESOLVED_ITEMS: &str = "horizon:resolved_items";

/// The kind an ingredient object names: `neoforge:ingredient_type` in
/// current NeoForge, `type` in older packs.
fn ingredient_kind(o: &serde_json::Map<String, Value>) -> Option<&str> {
    o.get("neoforge:ingredient_type")
        .or_else(|| o.get("type"))?
        .as_str()
}

/// A mod's own kind: a namespaced type that is not minecraft's.
fn is_custom(kind: &str) -> bool {
    kind.contains(':') && !kind.starts_with("minecraft:")
}

/// A set of accepted items (expanded from an item, list or tag), and the
/// items in its `HolderSet` order, with built-in component and display rules.
#[derive(Clone, Debug)]
// exhaustive: the game builds it with a literal; mods only read it
pub struct Ingredient {
    /// Full candidate set; a non-simple predicate tests each concrete stack.
    pub items: HashSet<String>,
    /// Default-stack display/simple-match order; may be empty for non-simple kinds.
    pub order: Vec<String>,
    /// A mod's own kind, for matching at craft time: the kind and its
    /// NBT compound, as the load-time hook saw them (`native_ingredient_test`).
    pub custom: Option<(String, horizon_core::nbt::Compound)>,
    /// NeoForge's shapes, with their children parsed: matching a concrete
    /// stack asks each child its own `test` (a mod's kind among them),
    /// as on NeoForge — the flat list above only names items.
    pub children: Option<(Shape, Vec<Self>)>,
    /// Built-in predicate evaluated against the concrete stack.
    pub rule: Option<IngredientRule>,
    /// Validated decoded component predicate, shared by all candidate checks.
    pub compiled: Option<std::sync::Arc<components::ComponentPredicate>>,
    /// Slot display override; membership continues to use `items`.
    pub display: Option<horizon_core::nbt::Compound>,
    /// Resolved slot display stacks; never used for placement or matching.
    pub shown: Option<Vec<ItemStack>>,
    /// Shared prototypes of the load that parsed this ingredient.
    pub component_defaults:
        std::sync::Arc<std::collections::HashMap<String, horizon_core::nbt::Compound>>,
}
impl Ingredient {
    /// Reject malformed component patches before any part of a recipe is published.
    pub fn validate_recipe(value: &Value, items: &Items, recipe: &str) -> bool {
        components::validate_recipe(value, items, recipe)
    }
    /// Decode built-in rules and materialize display stacks at a load boundary.
    pub fn prepare_components(&mut self, items: &Items) -> Option<()> {
        if let Some(rule) = &self.rule {
            let compiled = std::sync::Arc::new(
                components::ComponentPredicate::compile(rule, &self.items, items).ok()?,
            );
            if self.display.is_none() {
                self.shown = Some(compiled.shown(&self.order));
            }
            self.compiled = Some(compiled);
        }
        Some(())
    }
    fn default_matches(&self, stack: &ItemStack) -> bool {
        match &self.children {
            Some((Shape::Compound, kids)) => kids.iter().any(|kid| kid.default_matches(stack)),
            Some((Shape::Intersection, kids)) => kids.iter().all(|kid| kid.default_matches(stack)),
            Some((Shape::Difference, kids)) => {
                kids[0].default_matches(stack) && !kids[1].default_matches(stack)
            }
            Some((Shape::CustomDisplay, kids)) => kids[0].default_matches(stack),
            None => {
                self.items.contains(stack.item.as_ref())
                    && self
                        .compiled
                        .as_ref()
                        .is_none_or(|rule| rule.matches(stack))
            }
        }
    }

    /// Resolve a stored slot display at the owning resource boundary.
    pub fn resolve_display(
        display: &horizon_core::nbt::Compound,
        items: &Items,
    ) -> Option<Vec<ItemStack>> {
        display_stacks(display, items)
    }

    /// Whether item membership fully determines shapeless matching.
    pub fn is_simple(&self) -> bool {
        self.rule.is_none()
            && self.custom.is_none()
            && self
                .children
                .as_ref()
                .is_none_or(|(_, kids)| kids.iter().all(Self::is_simple))
    }

    /// The items of a list, in order.
    pub fn of(items: Vec<String>) -> Self {
        Self {
            items: items.iter().cloned().collect(),
            order: items,
            custom: None,
            children: None,
            rule: None,
            compiled: None,
            display: None,
            shown: None,
            component_defaults: Default::default(),
        }
    }
    /// Asks the `ingredient` hook once for each mod kind in `v`, nested ones
    /// included, and leaves each answer with its occurrence for `parse`: the
    /// one place the hook is called, so every table that reads the recipes
    /// shares the same answers. A file's own answer is dropped first, so data
    /// cannot forge one. `true`: `v` changed.
    // hidden: the load's half of the ingredient hook, not part of the mod surface
    #[doc(hidden)]
    pub fn resolve(v: &mut Value, context: &LoadContext) -> bool {
        match v {
            Value::Array(entries) => entries
                .iter_mut()
                .fold(false, |changed, e| Self::resolve(e, context) | changed),
            Value::Object(o) => {
                let mut changed = o.remove(RESOLVED_ITEMS).is_some();
                let kind = ingredient_kind(o).map(str::to_owned);
                match kind.as_deref() {
                    Some("neoforge:compound" | "neoforge:intersection") => {
                        for key in ["children", "ingredients"] {
                            if let Some(children) = o.get_mut(key) {
                                changed |= Self::resolve(children, context);
                            }
                        }
                    }
                    Some("neoforge:difference") => {
                        for key in ["base", "subtracted"] {
                            if let Some(part) = o.get_mut(key) {
                                changed |= Self::resolve(part, context);
                            }
                        }
                    }
                    Some("neoforge:custom_display") => {
                        if let Some(base) = o.get_mut("base") {
                            changed |= Self::resolve(base, context);
                        }
                    }
                    Some("neoforge:components" | "neoforge:block_tag") => {}
                    Some(k) if is_custom(k) => {
                        o.insert(RESOLVED_ITEMS.to_owned(), Value::Array(Vec::new()));
                        changed = true;
                    }
                    Some(_) => {}
                    // A brewing slot's item predicate wraps its ingredient.
                    None => {
                        if let Some(item) = o.get_mut("item") {
                            changed |= Self::resolve(item, context);
                        }
                    }
                }
                changed
            }
            _ => false,
        }
    }
    /// Every mod kind named in `v`, nested ones included, with whether
    /// `resolve` already left an empty answer for it. Walks the same shapes
    /// `resolve` does.
    pub fn custom_kinds(v: &Value) -> Vec<(String, bool)> {
        let mut found = Vec::new();
        Self::collect_custom_kinds(v, &mut found);
        found
    }
    fn collect_custom_kinds(v: &Value, found: &mut Vec<(String, bool)>) {
        match v {
            Value::Array(entries) => entries
                .iter()
                .for_each(|e| Self::collect_custom_kinds(e, found)),
            Value::Object(o) => {
                let parts: &[&str] = match ingredient_kind(o) {
                    Some("neoforge:compound" | "neoforge:intersection") => {
                        &["children", "ingredients"]
                    }
                    Some("neoforge:difference") => &["base", "subtracted"],
                    Some("neoforge:custom_display") => &["base"],
                    Some("neoforge:components" | "neoforge:block_tag") => &[],
                    Some(k) if is_custom(k) => {
                        let empty = o
                            .get(RESOLVED_ITEMS)
                            .and_then(Value::as_array)
                            .is_some_and(Vec::is_empty);
                        found.push((k.to_owned(), empty));
                        &[]
                    }
                    Some(_) => &[],
                    None => &["item"],
                };
                for key in parts {
                    if let Some(part) = o.get(*key) {
                        Self::collect_custom_kinds(part, found);
                    }
                }
            }
            _ => {}
        }
    }
    /// Drops every answer `resolve` left anywhere in `v`: what a load sends
    /// or shows is the files' own JSON. `true`: `v` changed.
    // hidden: the load's half of the ingredient hook, not part of the mod surface
    #[doc(hidden)]
    pub fn forget_answers(v: &mut Value) -> bool {
        match v {
            Value::Array(entries) => entries
                .iter_mut()
                .fold(false, |changed, e| Self::forget_answers(e) | changed),
            Value::Object(o) => {
                let mut changed = o.remove(RESOLVED_ITEMS).is_some();
                for part in o.values_mut() {
                    changed |= Self::forget_answers(part);
                }
                changed
            }
            _ => false,
        }
    }
    /// The vanilla ingredient: an item id, a `#tag`, a list of those, or an
    /// `{"item"}` / `{"tag"}` object.
    pub fn parse(v: &Value, items: &Items) -> Option<Self> {
        let order = Self::vanilla_order(v, items);
        let Some(order) = order else {
            log::error!("recipes: invalid ingredient (unresolved tag or malformed input): {v}");
            return None;
        };
        let mut ingredient = Self {
            items: order.iter().cloned().collect(),
            order,
            custom: None,
            children: None,
            rule: None,
            compiled: None,
            display: None,
            shown: None,
            component_defaults: items.component_defaults.clone(),
        };
        ingredient.prepare_components(items)?;
        if ingredient.order.is_empty() {
            log::warn!("recipes: ingredient has no placement candidates; recipe cannot be placed: {v}");
        }
        Some(ingredient)
    }

    fn vanilla_order(v: &Value, items: &Items) -> Option<Vec<String>> {
        match v {
            Value::String(id) => match id.strip_prefix('#') {
                Some(tag) => items.item_tag_order.get(tag).cloned(),
                None => Some(vec![id.clone()]),
            },
            Value::Array(entries) => {
                let mut order = Vec::new();
                for entry in entries {
                    order.extend(Self::vanilla_order(entry, items)?);
                }
                Some(order)
            }
            Value::Object(o) => match (o.get("item"), o.get("tag")) {
                (Some(Value::String(id)), _) => Some(vec![id.clone()]),
                (_, Some(Value::String(tag))) => items.item_tag_order.get(tag).cloned(),
                _ => None,
            },
            _ => None,
        }
    }
}


#[cfg(test)]
#[path = "ingredient/builtin_kinds_tests/mod.rs"]
mod builtin_kinds_tests;

#[cfg(test)]
#[path = "ingredient/review_tests.rs"]
mod review_tests;

