//! Recipe serializer resolution and the shared crafting lookup.
use super::*;
use std::result::Result;

fn source_path(name: &str) -> String {
    let (namespace, path) = name.split_once(':').unwrap_or(("minecraft", name));
    format!("data/{namespace}/recipe/{path}.json")
}
/// What a recipe file's `type` resolves to before the file is parsed.
/// `Debug` is for the tests' `unwrap_err`, which reads the reason a type is refused.
#[derive(Debug)]
pub enum Reading {
    /// A registered reader: the file is read as this recipe type.
    Type(String),
    /// Nothing loaded here reads it: a carried content mod's own type.
    Unread,
}
/// Resolve the file's serializer before parsing, after source conditions have run.
/// A `minecraft:` serializer must be a vanilla one; any other stays unread.
pub fn recipe_type(file: &str, serializer: &str) -> Result<Reading, String> {
    if super::VANILLA_RECIPE_SERIALIZERS.contains(&serializer) {
        return Ok(Reading::Type(
            if serializer.starts_with("minecraft:crafting_") {
                "minecraft:crafting"
            } else if serializer.starts_with("minecraft:smithing_") {
                "minecraft:smithing"
            } else {
                serializer
            }
            .into(),
        ));
    }
    let (namespace, _) = serializer
        .split_once(':')
        .unwrap_or(("minecraft", serializer));
    if namespace == "minecraft" {
        return Err(format!(
            "recipe {}, unregistered recipe serializer {serializer}",
            source_path(file)
        ));
    }
    Ok(Reading::Unread)
}

/// Resolve file serializers to recipe types at the content transport boundary.
/// A file nothing here reads is not sent.
pub fn file_has_type(file: &Value, types: &[String]) -> bool {
    matches!(reading(file), Some(Reading::Type(kind)) if types.contains(&kind))
}
/// Whether any reader here takes the file's `type`. A file none takes is dropped by the
/// loader, so it takes no display number either.
pub fn readable(file: &Value) -> bool {
    !matches!(reading(file), Some(Reading::Unread))
}
fn reading(file: &Value) -> Option<Reading> {
    file["type"]
        .as_str()
        .and_then(|id| recipe_type("sync", id).ok())
}

impl Recipes {
    /// The first recipe in order that matches the grid of `size` by side.
    pub fn find_with_dimensions(
        &self,
        context: &LoopContext,
        grid: &[Option<ItemStack>],
        size: usize,
        map_scale: &dyn Fn(i32) -> Option<u8>,
    ) -> Option<(&str, ItemStack)> {
        let vanilla = Input::of(grid, size)?;
        let ctx = Context {
            recipes: self,
            context,
            map_scale,
        };
        for (id, recipe) in &self.crafting {
            if let Some(output) = Self::assemble_if_matches(&ctx, &vanilla, recipe) {
                return output.map(|output| (id.as_str(), output));
            }
        }
        None
    }
}
