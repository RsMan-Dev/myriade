//! Beacons (`BeaconBlockEntity`, `BeaconMenu`, `BeaconScreen`,
//! `BeaconRenderer`): the beam sections tinted by stained glass, the pyramid
//! levels every 80 ticks, the primary/secondary effects for players in
//! range, the activate/ambient/deactivate sounds, the 230x219 menu with its
//! power buttons and payment slot, and the rotating, scrolling beam.
use super::{
    Game,
    stations::{INPUT, Kind},
};
use horizon_client::{
    containers::{Beacon, BlockEntity},
    effects::EffectInstance,
    mobs::DYE_NAMES,
};
use horizon_core::position::BlockPos;
use horizon_world::registry::BlockStateId;

/// `DyeColor.getTextureDiffuseColor` (RGB), by dye index.
pub(super) const SHIFT: f32 = -27.0;
const DYE: [u32; 16] = [
    0xF9FFFE, 0xF9801D, 0xC74EBD, 0x3AB3DA, 0xFED83D, 0x80C71F, 0xF38BAA, 0x474F52, 0x9D9D97,
    0x169C9C, 0x8932B8, 0x3C44AA, 0x835432, 0x5E7C16, 0xB02E26, 0x1D1D21,
];
/// `BEACON_EFFECTS` in order, with the pyramid level each needs.
pub(super) const EFFECTS: [(&str, i32); 6] = [
    ("speed", 1),
    ("haste", 1),
    ("resistance", 2),
    ("jump_boost", 2),
    ("strength", 3),
    ("regeneration", 4),
];
fn effect_id(name: &str) -> Option<&'static str> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    EFFECTS.iter().map(|(e, _)| *e).find(|e| *e == n)
}
/// `ARGB.average`.
fn average(a: u32, b: u32) -> u32 {
    let ch = |c: u32, s: u32| (c >> s) & 255;
    [16, 8, 0]
        .iter()
        .fold(0, |acc, &s| acc | ((ch(a, s) + ch(b, s)) / 2) << s)
}
impl Game<'_> {
    /// The beam color of a `BeaconBeamBlock` (stained glass and panes, the beacon).
    fn beam_color(&self, name: &str) -> Option<u32> {
        if name == "minecraft:beacon" {
            return Some(DYE[0]);
        }
        let n = name.strip_prefix("minecraft:")?;
        let dye = n
            .strip_suffix("_stained_glass")
            .or_else(|| n.strip_suffix("_stained_glass_pane"))?;
        DYE_NAMES.iter().position(|d| *d == dye).map(|i| DYE[i])
    }
    /// `IBlockStateExtension.getBeaconColorMultiplier` of `state` (named `name`) at `pos`
    /// for the beacon at `beacon`: the beam colour of a `BeaconBeamBlock`, unless a part of the
    /// state answers. The beam keeps the answer's red, green and blue.
    pub(super) fn beacon_color(
        &self,
        state: BlockStateId,
        name: &str,
        pos: [i32; 3],
        beacon: [i32; 3],
    ) -> Option<u32> {
        let own = self.beam_color(name);
        if !super::api_values::block_state::block_listens(
            mod_api::HookId::BeaconColorMultiplier,
            state,
        ) {
            return own;
        }
        let mut color = own;
        let [x, y, z] = pos;
        let [bx, by, bz] = beacon;
        self.native_beacon_color_multiplier(
            state,
            BlockPos::new(x, y, z),
            BlockPos::new(bx, by, bz),
            &mut color,
        );
        color.map(|c| c & 0xFF_FFFF)
    }
    /// `getHeight(WORLD_SURFACE)`: above the highest non-air block.
    pub(super) fn world_surface(&self, x: i32, z: i32) -> i32 {
        self.world
            .world_surface(x, z, |id| self.res.facts().is_air(id))
    }
    /// `BeaconBlockEntity.tick` of the beacon at `key`.
    pub(super) fn tick_beacon_at(&mut self, key: (i32, i32, i32)) {
        let Some((lease, BlockEntity::Beacon(mut b))) = self.world.block_entities.take(&key) else {
            return;
        };
        let previous = b.levels;
        self.tick_beacon(key, &mut b, false);
        let current = b.levels;
        self.world.block_entities.put(lease, BlockEntity::Beacon(b));
        if (previous == 0) != (current == 0) {
            self.native_beacon_state_change(self.dim.id(), [key.0, key.1, key.2], current > 0);
        }
    }
    fn tick_beacon(&mut self, (x, y, z): (i32, i32, i32), b: &mut Beacon, client: bool) {
        let mut check = if b.last_check_y < y {
            b.checking.clear();
            b.last_check_y = y - 1;
            y
        } else {
            b.last_check_y + 1
        };
        let surface = self.world_surface(x, z);
        let mut i = 0;
        while i < 10 && check <= surface {
            let state = self.world.block(x, check, z);
            let f = self.res.facts().get(state);
            if let Some(color) = self.beacon_color(state, &f.name, [x, check, z], [x, y, z]) {
                if b.checking.len() <= 1 {
                    b.checking.push((color, 1));
                } else if let Some(last) = b.checking.last_mut() {
                    if color == last.0 {
                        last.1 += 1;
                    } else {
                        let mixed = average(last.0, color);
                        b.checking.push((mixed, 1));
                    }
                }
            } else {
                let Some(last) = b
                    .checking
                    .last_mut()
                    .filter(|_| f.dampening < 15 || &*f.name == "minecraft:bedrock")
                else {
                    b.checking.clear();
                    b.last_check_y = surface;
                    break;
                };
                last.1 += 1;
            }
            check += 1;
            b.last_check_y += 1;
            i += 1;
        }
        let previous = b.levels;
        if self.world.game_time % 80 == 0 {
            if !b.sections.is_empty() {
                b.levels = self.beacon_base(x, y, z);
            }
            if !client && b.levels > 0 && !b.sections.is_empty() {
                self.beacon_effects([x, y, z], b);
                self.beacon_sound([x, y, z], "minecraft:block.beacon.ambient");
            }
        }
        if b.last_check_y >= surface {
            b.last_check_y = self.world.bottom_y() - 1;
            b.sections = std::mem::take(&mut b.checking);
            let (was, is) = (previous > 0, b.levels > 0);
            if !was && is {
                if !client {
                    self.beacon_sound([x, y, z], "minecraft:block.beacon.activate");
                    let levels = b.levels;
                    self.for_each_player(|g| g.beacon_constructed([x, y, z], levels));
                }
            } else if was && !is && !client {
                self.beacon_sound([x, y, z], "minecraft:block.beacon.deactivate");
            }
        }
    }
    /// `BeaconBlockEntity.tick` after activating: `CONSTRUCT_BEACON` for a
    /// player in the box `AABB(x, y, z, x, y - 4, z).inflate(10, 5, 10)`.
    fn beacon_constructed(&mut self, p: [i32; 3], levels: i32) {
        let Some(bb) = self
            .player
            .as_ref()
            .map(|pl| pl.dimensions.bounds(pl.position))
        else {
            return;
        };
        let (lo, hi) = (bb.min_position(), bb.max_position());
        let [x, y, z] = p.map(f64::from);
        if lo.x < x + 10.0
            && hi.x > x - 10.0
            && lo.y < y + 5.0
            && hi.y > y - 9.0
            && lo.z < z + 10.0
            && hi.z > z - 10.0
        {
            self.trigger_construct_beacon(levels);
        }
    }
    pub(super) fn beacon_sound(&mut self, p: [i32; 3], sound: &str) {
        let at = [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]) + 0.5,
            f64::from(p[2]) + 0.5,
        ];
        self.audio.play(sound, Some(at), 1.0, 1.0);
    }
    /// `updateBase`: complete layers of `beacon_base_blocks` below.
    fn beacon_base(&self, x: i32, y: i32, z: i32) -> i32 {
        let base = self
            .res
            .items()
            .block_tags
            .get("minecraft:beacon_base_blocks");
        let mut levels = 0;
        for step in 1..=4 {
            let ly = y - step;
            if ly < self.world.bottom_y() {
                break;
            }
            let ok = (x - step..=x + step).all(|lx| {
                (z - step..=z + step).all(|lz| {
                    let name = &self.res.facts().get(self.world.block(lx, ly, lz)).name;
                    base.is_some_and(|t| t.contains(&**name))
                })
            });
            if !ok {
                break;
            }
            levels = step;
        }
        levels
    }
    /// `applyEffects`: the powers for players within `levels * 10 + 10` (any height above).
    /// `BeaconBlockEntity.applyEffects`: every player in range.
    fn beacon_effects(&mut self, p: [i32; 3], b: &Beacon) {
        self.for_each_player(|g| g.beacon_effects_on_player(p, b));
    }
    fn beacon_effects_on_player(&mut self, p: [i32; 3], b: &Beacon) {
        let Some(primary) = b.primary.as_deref().and_then(effect_id) else {
            return;
        };
        let range = f64::from(b.levels * 10 + 10);
        let Some((pp, pl)) = self.player.as_ref().map(|p| (p.position, p.dimensions)) else {
            return;
        };
        let height = f64::from(self.world.top_y() - self.world.bottom_y());
        let bounds = pl.bounds(pp);
        if !effect_box_hits(
            p,
            range,
            height,
            [
                bounds.min_position().x,
                bounds.min_position().y,
                bounds.min_position().z,
            ],
            [
                bounds.max_position().x,
                bounds.max_position().y,
                bounds.max_position().z,
            ],
        ) {
            return;
        }
        let secondary = b.secondary.as_deref().and_then(effect_id);
        let amplifier = i32::from(b.levels >= 4 && secondary == Some(primary));
        let duration = (9 + b.levels * 2) * 20;
        let ambient = |id, amp| EffectInstance {
            ambient: true,
            ..EffectInstance::new(id, duration, amp)
        };
        self.potions.effects.add(ambient(primary, amplifier));
        if let Some(s) = secondary.filter(|&s| b.levels >= 4 && s != primary) {
            self.potions.effects.add(ambient(s, 0));
        }
    }
    /// Opening the menu shows the beacon's current powers.
    pub(super) fn beacon_opened(&mut self) {
        let Some(s) = self.station.as_ref().filter(|s| s.kind == Kind::Beacon) else {
            return;
        };
        let pos = s.pos;
        let (p, sec) = match self.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
            Some(BlockEntity::Beacon(b)) => (b.primary.clone(), b.secondary.clone()),
            _ => (None, None),
        };
        let index =
            |e: Option<String>| e.and_then(|e| EFFECTS.iter().position(|(n, _)| e.ends_with(n)));
        if let Some(s) = self.station.as_mut() {
            s.beacon = [index(p), index(sec)];
        }
    }
}
/// `BeaconBlockEntity.applyEffects` box: `AABB(pos).inflate(range).expandTowards(0, height, 0)`,
/// tested with `AABB.intersects` (strict) against the player's bounding box.
/// `BeaconMenu.PaymentSlot` and the player inventory, relative to the 176 origin.
pub(super) fn slot_position(i: usize) -> Option<(f32, f32)> {
    Some(match i {
        0..9 => (36.0 + SHIFT + i as f32 * 18.0, 195.0),
        9..36 => (
            36.0 + SHIFT + ((i - 9) % 9) as f32 * 18.0,
            137.0 + ((i - 9) / 9) as f32 * 18.0,
        ),
        INPUT => (136.0 + SHIFT, 110.0),
        _ => return None,
    })
}

fn effect_box_hits(p: [i32; 3], range: f64, height: f64, lo: [f64; 3], hi: [f64; 3]) -> bool {
    let [x, y, z] = p.map(f64::from);
    lo[0] < x + 1.0 + range
        && hi[0] > x - range
        && lo[1] < y + 1.0 + range + height
        && hi[1] > y - range
        && lo[2] < z + 1.0 + range
        && hi[2] > z - range
}
#[cfg(test)]
mod tests {
    use super::*;
    /// `BeaconScreen.extractBackground`: netherite 20, emerald 41, diamond
    /// 41+22, gold 42+44, iron 42+66.
    #[test]
    fn the_payment_row_reads_netherite_emerald_diamond_gold_iron() {
        assert_eq!(
            PAYMENT_ROW,
            [
                ("minecraft:netherite_ingot", 20.0),
                ("minecraft:emerald", 41.0),
                ("minecraft:diamond", 63.0),
                ("minecraft:gold_ingot", 86.0),
                ("minecraft:iron_ingot", 108.0),
            ]
        );
    }
    /// `BeaconUpgradePowerButton.createEffectDescription` appends " II"; the
    /// primary buttons' description is the bare effect.
    #[test]
    fn only_the_upgrade_button_names_the_second_level() {
        for e in 0..EFFECTS.len() {
            let primary = effect_tooltip(e, false).plain();
            let upgrade = effect_tooltip(e, true).plain();
            assert!(!primary.is_empty());
            assert!(!primary.ends_with(" II"));
            assert_eq!(upgrade, format!("{primary} II"));
        }
    }
    /// `BeaconScreenButton.setTooltip`: every button has one while hovered,
    /// and the sprite buttons' is their label whether or not they are active
    /// (the tick is disabled without a payment and a chosen power).
    #[test]
    fn every_hovered_beacon_button_shows_its_tooltip() {
        let at = |(x, y): (f32, f32)| (x + 11.0, y + 11.0);
        let name =
            |e: usize| horizon_client::lang::get(&format!("effect.minecraft.{}", EFFECTS[e].0));
        let buttons = buttons();
        for &(e, x, y, _) in &buttons {
            let tip = hovered_tooltip(at((x, y)), None, &buttons).unwrap_or_default();
            assert_eq!(tip.plain(), name(e), "button {e}");
        }
        let upgrade = at(upgrade_button());
        assert!(hovered_tooltip(upgrade, None, &buttons).is_none());
        assert_eq!(
            hovered_tooltip(upgrade, Some(0), &buttons).unwrap().plain(),
            format!("{} II", name(0))
        );
        assert_eq!(
            hovered_tooltip(at((164.0, 107.0)), None, &buttons)
                .unwrap_or_default()
                .plain(),
            horizon_client::lang::get("gui.done")
        );
        assert_eq!(
            hovered_tooltip(at((190.0, 107.0)), Some(0), &buttons)
                .unwrap_or_default()
                .plain(),
            horizon_client::lang::get("gui.cancel")
        );
        assert!(hovered_tooltip((0.0, 0.0), Some(0), &buttons).is_none());
    }
    #[test]
    fn only_active_beacons_have_beams() {
        let mut b = Beacon::default();
        assert!(beam(&BlockEntity::Beacon(Box::new(b.clone()))).is_none());
        b.levels = 1;
        b.sections.push((0xFFFFFF, 10));
        assert_eq!(
            beam(&BlockEntity::Beacon(Box::new(b))),
            Some(&[(0xFFFFFF, 10)][..])
        );
    }
    #[test]
    fn effect_box_uses_player_overlap() {
        let hit = |x: f64, y: f64| {
            effect_box_hits(
                [0, 100, 0],
                50.0,
                384.0,
                [x - 0.3, y, -0.3],
                [x + 0.3, y + 1.8, 0.3],
            )
        };
        assert!(hit(-50.2, 100.0));
        assert!(!hit(-50.4, 100.0));
        assert!(hit(51.2, 100.0));
        assert!(!hit(51.4, 100.0));
        assert!(hit(0.0, 48.3));
        assert!(!hit(0.0, 48.1));
        assert!(hit(0.0, 534.9));
        assert!(!hit(0.0, 535.0));
    }
}
