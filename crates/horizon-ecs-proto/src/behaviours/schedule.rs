//! Emploi du temps générique : deux tables (adulte, bébé) par espèce, heure → activité. Au réveil, chaque tranche active pose sa
//! mémoire d'activité jusqu'à sa fin (TTL) ; l'expiration réveille le cerveau, qui relance ce comportement : l'heure n'est jamais
//! relue entre deux tranches. Pas d'horloge du jour dans l'ECS (`Clock` mesure le temps d'entité) : `Times` est à brancher par le cadre.
use crate::brain::*;
use crate::cow::GrowAt;

#[derive(Resource, Default, Clone, Copy)]
pub struct Times {
    pub day: i64,
}

/// Une tranche `[from, to)` pendant laquelle la mémoire `key` (l'activité) est posée.
#[derive(Clone, Copy)]
pub struct Slice {
    pub key: &'static MemKey,
    pub from: i64,
    pub to: i64,
}

pub struct Params {
    pub adult: &'static [Slice],
    pub baby: &'static [Slice],
}

fn can_start(_: &mut World, _: Entity, _: &Ctx) -> bool {
    true
}
fn start(w: &mut World, e: Entity, c: &Ctx) {
    let t = w.resource::<Times>().day.rem_euclid(24000);
    let p = c.get::<Params>();
    let slices = if w.get::<GrowAt>(e).is_some() { p.baby } else { p.adult };
    let active = |s: &Slice| s.from <= t && t < s.to;
    for s in slices.iter().filter(|s| active(s)) {
        set(w, e, s.key, MemValue::Unit, Some((s.to - t) as u64));
    }
    // Une clé à deux tranches (la nuit passe minuit) reste posée tant qu'une de ses tranches est active.
    for s in slices.iter().filter(|s| !active(s) && !slices.iter().any(|a| active(a) && std::ptr::eq(a.key, s.key))) {
        erase(w, e, s.key);
    }
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "schedule:timeline", can_start, start, one_shot: true, passive: true, ..Behaviour::DEFAULT };
