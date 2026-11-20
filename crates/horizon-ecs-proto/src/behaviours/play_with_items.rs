//! `Dolphin.PlayWithItemsGoal` (fiche 111, `dolphin.rs:478-495`, priorité 8 par `aquatic.rs:180`) et le jet `dropItem`
//! (`dolphin.rs:415-441`) : un dauphin qui tient un objet le jette devant lui, avec un délai de ramassage de 40 ticks. Pas de
//! drapeau. Réveil : l'objet entré dans la main (`Events.picked_up`, poussé par `item::pick_up`). La fonction `throw` est
//! publique : la fiche 110 (`move_to_item`) l'appelle.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::item::{self, Item};
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::jmath;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Le délai de ramassage de l'objet jeté (`dolphin.rs:437`).
    pub delay: i64,
    /// `eye_y - 0,3` : la hauteur du départ (`dolphin.rs:421`).
    pub hand_lower: f64,
    /// La poussée le long du regard (`dolphin.rs:422`).
    pub push: f64,
    /// L'ampleur du bruit latéral (`dolphin.rs:424`).
    pub noise: f64,
    /// La hausse par inclinaison (`dolphin.rs:429`, `* 1,5`).
    pub lift: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { delay: 40, hand_lower: 0.3, push: 0.3, noise: 0.02, lift: 1.5 };
}
/// L'état : le rang de l'entrée (le but n'a pas de marqueur : il tient en un jet).
#[derive(Component, Clone, Copy, Debug)]
pub struct Thrower {
    pub slot: u8,
    pub p: &'static Params,
}

pub static MODULE: Module = Module {
    name: "play_with_items",
    flags: 0,
    equip,
    can_use,
    start,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Thrower { slot: inst.slot, p });
}
/// `canUse` : la main principale n'est pas vide.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    main_hand(w, e).is_some()
}
/// `start` : le jet a lieu au tick de départ (le but tourne le tick où il démarre) ; la main est vide, le but finit.
fn start(w: &mut World, e: Entity, inst: Inst) {
    throw(w, e);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, pickup_run);
}
/// Un objet entré dans la main d'un dauphin qui porte le but le réveille (lu une fois, au tick suivant), sauf s'il va vers un objet :
/// `MoveToItemGoal.stop` jette d'abord ce qu'il tient et lance le repos (le sélecteur arrête avant de démarrer), `move_to_item::tick` le réveille si le but continue.
fn pickup_run(ev: Res<Events>, q: Query<&Thrower, Without<super::move_to_item::Fetching>>, mut reqs: ResMut<Requests>) {
    for e in ev.picked_up.iter() {
        if let Ok(t) = q.get(e) {
            reqs.0.push(Req::Start(e, t.slot));
        }
    }
}

/// `dropItem` du dauphin : la pile de la main part devant la tête (hauteur des yeux moins 0,3), poussée de 0,3 selon le lacet et
/// l'inclinaison, bruit de 0,02 dans une direction aléatoire (tirée d'abord la direction, puis l'ampleur), délai 40. Vrai si un
/// objet a été jeté. DOUTE : l'inclinaison lue est `Drive.pitch` (`m.head_pitch` côté `Mob`, `dolphin.rs:426`).
pub fn throw(w: &mut World, e: Entity) -> bool {
    let Some(p) = w.get::<Thrower>(e).map(|t| t.p) else { return false };
    let Some(stack) = main_hand(w, e).cloned() else { return false };
    let (Some(pos), Some(yaw), Some(eye)) = (
        w.get::<Pos>(e).map(|x| x.0),
        w.get::<Yaw>(e).map(|y| f64::from(y.0)),
        w.get::<FluidParams>(e).map(|f| f.eye_height),
    ) else {
        return false;
    };
    let pitch = w.get::<Drive>(e).map_or(0.0, |d| f64::from(d.pitch));
    let (dir, pow2) = with_rng(w, e, |r| {
        let dir = f64::from(r.next_float()) * std::f64::consts::TAU;
        (dir, p.noise * f64::from(r.next_float()))
    });
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let at = Vec3::new(pos.x, pos.y + eye - p.hand_lower, pos.z);
    let v = Vec3::new(
        p.push * -jmath::sin(yaw) * jmath::cos(pitch) + jmath::cos(dir) * pow2,
        p.push * jmath::sin(pitch) * p.lift,
        p.push * jmath::cos(yaw) * jmath::cos(pitch) + jmath::sin(dir) * pow2,
    );
    set_main_hand(w, e, None);
    item::drop_item(w, stack, at, v, p.delay);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use std::sync::Arc;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 8, &Params::DEFAULT)]));
        Config { entries }
    }
    fn stick() -> ItemStack {
        ItemStack { item: Arc::from("minecraft:stick"), count: 1, damage: 0, extra: None }
    }
    /// Un dauphin seul avec ce but, dans une mare (eau aux rangées -1 et 0), cap au sud, une pile en main.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        for x in -6..=6 {
            for z in -6..=6 {
                h.set_block(x, -1, z, "minecraft:water");
                h.set_block(x, 0, z, "minecraft:water");
            }
        }
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:dolphin").unwrap();
        sp.goals = config();
        let d = spawn_species(&mut h.world, &sp, 0.5, 0.5, 93);
        h.world.get_mut::<ActiveUntil>(d).unwrap().0 = u64::MAX;
        h.world.get_mut::<Yaw>(d).unwrap().0 = 0.0;
        h.world.resource_mut::<crate::goals::Entered>().0.push(d);
        h.tick();
        set_main_hand(&mut h.world, d, Some(stick()));
        Some((h, d))
    }
    /// Les objets au sol : leur vitesse.
    fn thrown(h: &mut Herd) -> Vec<Vec3> {
        h.world.query::<(&Item, &Vel)>().iter(&h.world).map(|(_, v)| v.0).collect()
    }
    /// La vitesse de la pile jetée en dernier (les objets sont listés dans l'ordre où ils sont créés, ici le dernier).
    fn thrown_newest(h: &mut Herd, before: usize) -> Vec3 {
        let v = thrown(h);
        assert_eq!(v.len(), before + 1, "une pile de plus");
        v[before]
    }

    /// La loi du jet (`dolphin.rs:423-431`), lue juste après `throw` : hors physique, pas de tick entre le jet et la lecture.
    #[test]
    fn the_throw_follows_the_mobs_law_for_yaw_and_pitch() {
        let Some((mut h, d)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        for (yaw, pitch) in [(0.0f32, 0.0f32), (0.0, 20.0)] {
            set_main_hand(&mut h.world, d, Some(stick()));
            h.world.get_mut::<Yaw>(d).unwrap().0 = yaw;
            h.world.get_mut::<Drive>(d).unwrap().pitch = pitch;
            let before = thrown(&mut h).len();
            assert!(throw(&mut h.world, d), "témoin : la pile part");
            let v = thrown_newest(&mut h, before);
            let p = f64::from(pitch).to_radians();
            // Bruit horizontal au plus 0,02 par axe ; poussée 0,3 le long du regard ; hausse 0,45 sin(inclinaison).
            assert!((v.z - 0.3 * p.cos()).abs() <= 0.02 + 1e-9, "poussée de 0,3 selon le regard (yaw {yaw}, pitch {pitch}) : {v:?}");
            assert!((v.y - 0.45 * p.sin()).abs() <= 0.02 + 1e-9, "hausse de 0,45 sin(inclinaison) : {v:?}");
            assert!(v.x.abs() <= 0.02 + 1e-9, "bruit latéral au plus 0,02 : {v:?}");
        }
    }

    /// Sans événement de ramassage, rien n'est jeté ; l'événement fait jeter la pile.
    #[test]
    fn a_pickup_makes_the_dolphin_throw_its_item_and_not_before() {
        let Some((mut h, d)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        for _ in 0..10 {
            h.tick();
        }
        assert!(main_hand(&h.world, d).is_some(), "sans ramassage, la pile reste en main");
        assert!(thrown(&mut h).is_empty(), "sans ramassage, rien n'est jeté");
        h.world.resource_mut::<Events>().picked_up.push(d);
        for _ in 0..4 {
            h.tick();
        }
        assert!(main_hand(&h.world, d).is_none(), "témoin : la main est vidée");
        assert_eq!(thrown(&mut h).len(), 1, "témoin : une pile jetée");
    }
}
