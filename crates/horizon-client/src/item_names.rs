//! Display names of items and the name of a player head, for hover names and anvils.
use crate::items::ItemStack;
use std::collections::HashMap;

/// Display names from the pinned en_us language file.
#[derive(Default)]
pub struct Names {
    map: HashMap<String, String>,
}

impl Names {
    pub fn load(root: &horizon_assets::resource_source::ResourceSource) -> Self {
        let read = |namespace: &str| {
            root.read(&format!("assets/{namespace}/lang/en_us.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<HashMap<String, String>>(&b).ok())
                .unwrap_or_default()
        };
        let mut map = read("minecraft");
        // Content mods' names (docs/modpacks.md); none in a vanilla root.
        for namespace in root
            .namespaces("assets")
            .into_iter()
            .filter(|ns| ns != "minecraft")
        {
            map.extend(read(&namespace));
        }
        Self { map }
    }

    pub fn item(&self, id: &str) -> String {
        if let Some(name) = crate::item_data::default_names::of(id) {
            return name.plain();
        }
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        self.map
            .get(&format!("item.{namespace}.{path}"))
            .or_else(|| self.map.get(&format!("block.{namespace}.{path}")))
            .cloned()
            .unwrap_or_else(|| path.replace('_', " "))
    }

    /// An item's display name in the selected language: `item.*` then
    /// `block.*`, then the pinned English map (`Names::item`).
    pub fn display_item(&self, id: &str) -> String {
        if let Some(name) = crate::item_data::default_names::of(id) {
            return name.plain();
        }
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        if let Some(lang) = crate::lang::current() {
            for prefix in ["item", "block"] {
                if let Some(name) = lang.get(&format!("{prefix}.{namespace}.{path}")) {
                    return name.to_owned();
                }
            }
        }
        self.item(id)
    }

    /// A translation key's text in the selected language, else the pinned
    /// English map (`HoverName` and tooltip keys such as potion names).
    pub fn display_get(&self, key: &str) -> Option<String> {
        crate::lang::current()
            .and_then(|lang| lang.get(key).map(str::to_owned))
            .or_else(|| self.get(key).map(str::to_owned))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }
}

/// `PlayerHeadItem.getName`: a player head whose profile has a name is titled
/// after it.
pub struct PlayerHeadName(String);

impl PlayerHeadName {
    /// The title as `getName`'s `%s's Head` translation.
    pub fn title(&self, translate: &dyn Fn(&str) -> Option<String>) -> String {
        translate("block.minecraft.player_head.named")
            .unwrap_or_else(|| "%s's Head".to_owned())
            .replacen("%s", &self.0, 1)
    }
}

/// The name in a player head's `minecraft:profile`, if it has one.
pub fn player_head_name(stack: &ItemStack) -> Option<PlayerHeadName> {
    if &*stack.item != "minecraft:player_head" {
        return None;
    }
    Some(PlayerHeadName(stack.profile_name()?))
}
