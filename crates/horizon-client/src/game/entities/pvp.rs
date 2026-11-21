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
    /// The custom crosshair, centred where vanilla's 15x15 sprite is (`Hud.extractCrosshair`):
    /// its outline in black, then the shape, inverting what is under it when its colour is
    /// `Inverted`. The runs are cached, so a frame only adds their quads.
    pub(in crate::game) fn draw_custom_crosshair(&mut self, batch: &mut HudBatch, w: i32, h: i32) {
        let mut c = self.settings.extras.pvp.crosshair;
        if let Some(hover) = c.hover
            && self.crosshair_living() == Some(true)
        {
            c.colour = hover;
        }
        let (cx, cy) = ((w - 15) / 2 + 7, (h - 15) / 2 + 7);
        let atlas = &self.gui.atlas;
        let (shape, rim) = self.pvp.crosshair.runs(&c);
        let fill = |batch: &mut HudBatch, &(x0, y, x1): &(i32, i32, i32), colour: u32| {
            let (x, y) = ((cx + x0) as f32, (cy + y) as f32);
            batch.fill(atlas, x, y, (x1 - x0) as f32, 1.0, colour);
        };
        for run in rim {
            fill(batch, run, rgba(0, 0, 0, 255));
        }
        let start = batch.vertices.len();
        let [a, r, g, b] = c.colour.argb().to_be_bytes();
        for run in shape {
            fill(batch, run, rgba(r, g, b, a));
        }
        if c.colour == Colour::Inverted {
            batch.invert_since(start);
        }
    }
    /// The block outline's fill: each box of the targeted block's shape (`getShape`, the boxes
    /// the outline's edges come from) as translucent faces drawn after translucent terrain,
    /// pushed out a little so they don't fight the block's own faces. Only while the outline
    /// shows (`outline` is its edges).
    pub(in crate::game) fn outline_fill(
        &self,
        outline: &[[f64; 6]],
        camera: glam::DVec3,
        out: &mut Vec<Vertex>,
    ) {
        let Some(argb) = self.settings.extras.pvp.outline.fill() else {
            return;
        };
        let Some(hit) = self.pick().filter(|_| !outline.is_empty()) else {
            return;
        };
        let [x, y, z] = hit.pos;
        let state = self.world.block(x, y, z);
        let facts = self.res.facts().get(state);
        let offset = match facts.outline_offset {
            true => self.pack.context.tables.offset(state, x, z),
            false => None,
        }
        .unwrap_or([0.0; 3]);
        let base = [f64::from(x), f64::from(y), f64::from(z)];
        let context = self.context_outline(hit.pos, state, true, &self.be_anims);
        let boxes = context.as_slice().iter().chain(match context {
            Some(_) => &[][..],
            None => &facts.outline[..],
        });
        let uv = horizon_client::gfx::lightning::white_uv(&self.gui.atlas.font.white());
        let [a, r, g, b] = argb.to_be_bytes();
        let color = u32::from_le_bytes([r, g, b, a]);
        const OUT: f64 = 0.002;
        for shape in boxes {
            let corner = |i: usize, high: bool| {
                let v = if high {
                    shape[i + 3] + OUT
                } else {
                    shape[i] - OUT
                };
                (base[i] + offset[i] + v - camera[i]) as f32
            };
            // Each face: the axis it faces along, its side, and its corners' other two axes.
            for axis in 0..3 {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                for high in [false, true] {
                    for (hu, hv) in [(false, false), (true, false), (true, true), (false, true)] {
                        let mut pos = [0.0; 3];
                        pos[axis] = corner(axis, high);
                        pos[u] = corner(u, hu);
                        pos[v] = corner(v, hv);
                        out.push(Vertex {
                            pos,
                            uv,
                            layer: horizon_client::gfx::TRANSLUCENT_PARTICLE_OVERLAY,
                            color,
                            light: 240 | 240 << 8,
                            normal: [0; 4],
                            glint_uv: [0.0; 2],
                        });
                    }
                }
            }
        }
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
    /// The keystrokes widget at `(x, y)`: a box per key, light while held, its name centred,
    /// and under each mouse button its clicks per second.
    fn paint_keys(&self, batch: &mut HudBatch, [x, y, sw, _]: [f32; 4], style: &Style) {
        let start = batch.vertices.len();
        let atlas = &self.gui.atlas;
        let k = &self.pvp.keys;
        let colour = |c: u32| {
            let [a, r, g, b] = c.to_be_bytes();
            rgba(r, g, b, a)
        };
        for (i, key) in Key::ALL.into_iter().enumerate() {
            let (kx, ky, w, h) = pvp_hud::key_rect(key);
            let (kx, ky) = (x + kx, y + ky);
            let held = k.held[i];
            if held {
                batch.fill(atlas, kx, ky, w, h, colour(KEY_HELD));
            } else if let Some(background) = style.background {
                batch.fill(atlas, kx, ky, w, h, colour(background));
            }
            let text_colour = colour(if held { KEY_HELD_TEXT } else { style.text });
            let Some((label, lw)) = k.labels.get(i) else {
                continue;
            };
            let cps = match key {
                Key::Attack => k.cps.lines.first(),
                Key::Use => k.cps.lines.get(1),
                _ => None,
            };
            let top = match cps {
                Some(_) => ky + 2.0,
                None => ky + (h - 8.0) / 2.0,
            };
            let shadow = style.shadow && !held;
            batch.text_styled(atlas, label, kx + (w - lw) / 2.0, top, text_colour, shadow);
            if let Some(line) = cps {
                // The count at half size under the name.
                let start = batch.vertices.len();
                let (cx, cy) = (kx + w / 2.0, ky + h - 6.0);
                let tx = cx - line.text_width / 2.0;
                batch.text_styled(atlas, &line.text, tx, cy, text_colour, false);
                batch.scale_since(start, cx, cy, 0.5, 0.5);
            }
        }
        let scale = sw / pvp_hud::keys_size().0;
        if scale != 1.0 {
            batch.scale_since(start, x, y, scale, scale);
        }
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
    /// One widget's box in `corner`: icons in a column, texts beside them.
    fn paint_widget(
        &self,
        batch: &mut HudBatch,
        [x, y, sw, _]: [f32; 4],
        lines: &[Line],
        (w, h): (f32, f32),
        style: &Style,
    ) {
        let start = batch.vertices.len();
        let atlas = &self.gui.atlas;
        if let Some(background) = style.background {
            let [a, r, g, b] = background.to_be_bytes();
            batch.fill(atlas, x, y, w, h, rgba(r, g, b, a));
        }
        let icons = pvp_hud::icon_column(lines);
        let mut top = y + pvp_hud::PAD;
        for line in lines {
            let inner = line.height() - 2.0;
            let size = line.icon.size();
            let (ix, iy) = (x + pvp_hud::PAD, top + (inner - size) / 2.0);
            match &line.icon {
                Icon::None => {}
                Icon::Armour(i) => {
                    if let Some(stack) = &self.armor[usize::from(*i)] {
                        self.draw_item(batch, stack, ix, iy);
                    }
                }
                Icon::Slot(slot) => {
                    if let Some(stack) = self.stack_in(*slot) {
                        self.draw_item_model(batch, stack, ix, iy);
                    }
                }
                Icon::Sprite(name) => {
                    if let Some(r) = atlas.sprite_or_missing(name) {
                        batch.rect_uv(ix, iy, size, size, r, u32::MAX);
                    }
                }
            }
            let text_h = if line.detail.is_some() { 18.0 } else { 8.0 };
            let tx = x + pvp_hud::PAD + line.text_x(icons);
            let ty = top + (inner - text_h) / 2.0;
            let argb = |c: u32| {
                let [a, r, g, b] = c.to_be_bytes();
                rgba(r, g, b, a)
            };
            // White lines take the widgets' text colour; durability keeps its own.
            let colour = match line.colour {
                0xFFFF_FFFF => style.text,
                c => c,
            };
            batch.text_styled(atlas, &line.text, tx, ty, argb(colour), style.shadow);
            if let Some(detail) = &line.detail {
                batch.text_styled(atlas, detail, tx, ty + 10.0, argb(DETAIL), style.shadow);
            }
            top += line.height();
        }
        if sw != w {
            batch.scale_since(start, x, y, sw / w, sw / w);
        }
    }
}
