//! The PvP & HUD page's options in the game (`horizon_client::pvp`, `docs/pvp.md`). Each
//! does nothing while it is off.
use crate::game::{Dim, Game};
use horizon_client::{
    extras::Corner,
    extras_hud::Corners,
    hud::{HudBatch, rgba},
    mesher::Vertex,
    particles::ParticleType,
    pvp::{ChatOptions, Colour, CrosshairCache, Frame},
    pvp_hud::{self, Combo, Durability, Icon, Key, Line, Widget},
    text::Text,
};
use std::sync::Arc;
use winit::event::MouseButton;

#[derive(Default)]
pub(in crate::game) struct PvpState {
    crosshair: CrosshairCache,
    /// Each armour slot's item, damage and maximum damage, and whether durability is in percent.
    armour: Widget<([Option<(Arc<str>, u32, u32)>; 4], bool)>,
    /// Each effect: id, amplifier, whole seconds left (-1 infinite).
    effects: Widget<Vec<(&'static str, i32, i32)>>,
    /// The counted item's slot and the total.
    held: Widget<Option<(usize, Arc<str>, u32)>>,
    ping: Widget<Option<i32>>,
    /// The block position and the direction faced.
    coords: Widget<Option<([i32; 3], &'static str)>>,
    server: Widget<Option<String>>,
    combo: Combo,
    combo_count: Widget<u32>,
    fps: Widget<u32>,
    /// Left and right presses, counted while the CPS or keystrokes widget is on.
    clicks: [pvp_hud::Clicks; 2],
    cps: Widget<(u32, u32)>,
    keys: Keystrokes,
    /// The local hour and minute, and when they were read.
    clock: Widget<(u8, u8)>,
    clock_read: f64,
    reach: pvp_hud::Reach,
    reach_shown: Widget<Option<u32>>,
    speed: pvp_hud::Speed,
    speed_shown: Widget<u32>,
    /// Each widget drawn last frame and its box on screen, for the HUD editor.
    pub(in crate::game) boxes: Vec<(pvp_hud::WidgetId, [f32; 4])>,
    /// The HUD editor, while it is open.
    pub(in crate::game) editor: Option<crate::game::hud_editor::HudEditor>,
    /// The open chat's box holds a chat search's query (Ctrl+F).
    pub(in crate::game) chat_search: bool,
    /// What the chat box held when the search borrowed it.
    pub(in crate::game) chat_search_aside: String,
}
/// `GuiGraphicsExtractor`'s grey for a detail line (an effect's time left).
const DETAIL: u32 = 0xFF7F_7F7F;
/// How the widgets look: their boxes' colour (none without boxes), text colour and shadow.
struct Style {
    background: Option<u32>,
    text: u32,
    shadow: bool,
}
/// A held key's box in the keystrokes widget, and its text.
const KEY_HELD: u32 = 0xB4FF_FFFF;
const KEY_HELD_TEXT: u32 = 0xFF00_0000;

/// The keystrokes widget: each key's label, made again only when its binding changes, which
/// keys are held, and the clicks per second under the mouse buttons.
#[derive(Default)]
struct Keystrokes {
    bound: [String; 7],
    labels: Vec<(Text, f32)>,
    held: [bool; 7],
    cps: Widget<(u32, u32)>,
}
const ARROWS: [&str; 3] = [
    "minecraft:arrow",
    "minecraft:spectral_arrow",
    "minecraft:tipped_arrow",
];

impl Game<'_> {
    /// Whether the custom crosshair replaces vanilla's sprite.
    pub(in crate::game) fn custom_crosshair(&self) -> bool {
        self.settings.extras.pvp.crosshair.custom
    }
    /// Whether this window may draw PvP & HUD's own time and weather: only a client of a server
    /// (single player's included) draws them, never the server that ticks the world.
    fn own_view(&self) -> bool {
        false && self.dim == Dim::Overworld
    }
    /// The day time the frame draws (sky, sun, moon, the sky's light): PvP & HUD's time of day,
    /// else the world's. The world's own `day_time` and everything that plays by it stay.
    pub(in crate::game) fn view_day_time(&self) -> i64 {
        let fixed = self.settings.extras.pvp.tweaks.time;
        horizon_client::pvp::view_day_time(self.world.day_time, fixed.filter(|_| self.own_view()))
    }
    /// The rain and thunder levels the frame draws (rain, the sky's colour and light, fog): PvP &
    /// HUD's weather, else the world's. Gameplay (riptide, mobs, crops) keeps the world's.
    pub(in crate::game) fn view_weather(&self, partial: f32) -> (f32, f32) {
        match self.settings.extras.pvp.tweaks.weather {
            Some(w) if self.own_view() && self.can_have_weather() => w.levels(),
            _ => self.weather_levels(partial),
        }
    }
    // What the engine reads from PvP & HUD. Every engine use goes through these, so each becomes
    // one engine knob or hook (vanilla's value when nothing is registered) when PvP & HUD leaves
    // the engine (docs/layers.md, plan M5). The HUD editor (`hud_editor.rs`) reads the widgets.

    /// Combat feel Legacy's pictures: 1.8.9's sword block, flat item pose and hand swing.
    pub(in crate::game) fn pvp_legacy_look(&self) -> bool {
        self.settings.extras.pvp.combat.legacy
    }
    /// Combat feel Legacy hides the attack indicator unless the player keeps it.
    pub(in crate::game) fn pvp_hides_attack_indicator(&self) -> bool {
        let combat = self.settings.extras.pvp.combat;
        combat.legacy && !combat.indicator
    }
    /// Legacy rules: the hits a server resolves follow 1.8.9 (`legacy_rules.rs`).
    pub(in crate::game) fn pvp_legacy_rules(&self) -> bool {
        self.settings.extras.pvp.combat.rules
    }
    /// The server's legacy rules, from server.toml or the window's options.
    pub(in crate::game) fn set_pvp_legacy_rules(&mut self, on: bool) {
        self.settings.extras.pvp.combat.rules = on;
    }
    /// The block outline, hurt overlay and glint colours the renderer draws with.
    pub(in crate::game) fn pvp_render_frame(&self) -> Frame {
        self.settings.extras.pvp.frame()
    }
    /// The enchantment glint's colour texel, 0 for vanilla's.
    pub(in crate::game) fn pvp_glint_texel(&self) -> u32 {
        self.settings.extras.pvp.tweaks.glint_texel()
    }
    /// The held shield's size in percent (100 is vanilla's).
    pub(in crate::game) fn pvp_shield_size(&self) -> u8 {
        self.settings.extras.pvp.tweaks.shield_size
    }
    /// The totem animation's scale (1 is vanilla's).
    pub(in crate::game) fn pvp_totem_scale(&self) -> f32 {
        f32::from(self.settings.extras.pvp.tweaks.totem_size) / 100.0
    }
    /// How far the first-person fire overlay is lowered, as a fraction (0 is vanilla's).
    pub(in crate::game) fn pvp_fire_lower(&self) -> f32 {
        f32::from(self.settings.extras.pvp.tweaks.fire_lower) / 100.0
    }
    /// Lava's fog as with fire resistance, and water's as fully adapted.
    pub(in crate::game) fn pvp_clear_fluids(&self) -> (bool, bool) {
        let tweaks = self.settings.extras.pvp.tweaks;
        (tweaks.clear_lava, tweaks.clear_water)
    }
    /// The chat's options: Ctrl+F search, copy on click, and the rest (`ChatOptions::qol`).
    pub(in crate::game) fn pvp_chat(&self) -> ChatOptions {
        self.settings.extras.pvp.chat
    }
    /// This player attacked entity `id`, `distance` from the eyes to where the crosshair's
    /// ray met its box (the combo counter waits for the server's report; the reach widget
    /// shows the distance).
    pub(in crate::game) fn pvp_attacked(&mut self, id: i32, distance: f64) {
        let now = self.started.elapsed().as_secs_f64();
        self.pvp.combo.attacked(id, now);
        self.pvp.reach.hit(distance, now);
    }
    /// Whether the key `mapping` is bound to is held: a keyboard key or a mouse button.
    fn binding_held(&self, mapping: &str) -> bool {
        let bound = self.settings.keys.get(mapping);
        if let Some(button) = horizon_client::keybinds::mouse_button(bound) {
            return self.physical_buttons.contains(&button);
        }
        self.mapped_key_down(mapping)
    }
    /// The server reports entity `id` hurt; `own` when it is this player,
    /// `by_me` whether this player was the damage's direct entity (if named).
    pub(in crate::game) fn pvp_hurt(&mut self, id: i32, own: bool, by_me: Option<bool>) {
        if own {
            self.pvp.combo.broken();
        } else {
            let now = self.started.elapsed().as_secs_f64();
            self.pvp.combo.hurt(id, by_me, now);
        }
    }
    /// Whether the effects widget replaces vanilla's effect icons (`Hud.extractEffects`).
    pub(in crate::game) fn pvp_effects_shown(&self) -> bool {
        self.settings.extras.pvp.hud.effects
    }
    /// Where a widget of `size` goes at its `place`'s scale: where the player put it, else in
    /// its corner (unless a mod hides that corner). `[x, y, w, h]`, scaled.
    fn widget_rect(
        &self,
        corners: &mut Corners,
        place: pvp_hud::Place,
        corner: Corner,
        (w, h): (f32, f32),
        screen: (f32, f32),
    ) -> Option<[f32; 4]> {
        let scale = f32::from(place.scale) / 100.0;
        let size = (w * scale, h * scale);
        let (x, y) = match place.at {
            Some(at) => pvp_hud::from_fractions(at, size, screen),
            None if self.native_corner_shown(corner) => corners.place(corner, size.0, size.1),
            None => return None,
        };
        Some([x, y, size.0, size.1])
    }
    /// The held item count: the first slot holding what is counted, its item, and the total in
    /// the inventory and off hand (arrows for a bow or crossbow). None for a single item.
    fn held_count(&self) -> Option<(usize, Arc<str>, u32)> {
        let held = self.held()?;
        let ranged = matches!(&*held.item, "minecraft:bow" | "minecraft:crossbow");
        let counts = |s: &horizon_client::items::ItemStack| match ranged {
            true => ARROWS.contains(&&*s.item),
            false => s.item == held.item,
        };
        let slots = self.inventory.iter().map(Option::as_ref).enumerate();
        let offhand = std::iter::once((crate::game::shield::OFFHAND_SLOT, self.offhand.as_ref()));
        let mut found = slots
            .chain(offhand)
            .filter_map(|(i, s)| Some((i, s?)))
            .filter(|(_, s)| counts(s));
        let (first, stack) = found.next()?;
        let total = found
            .map(|(_, s)| s.count)
            .fold(stack.count, u32::saturating_add);
        (ranged || total > 1).then(|| (first, stack.item.clone(), total))
    }
}
