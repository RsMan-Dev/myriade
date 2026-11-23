//! `villager_witness`: the murder witnesses of VanillaVillagerWitnessOracle, on the dedicated server's ECS.
use crate::game::Game;
use horizon_client::Result;
use horizon_ecs_proto::{behaviours::gossip, effects::Invisible};
use serde_json::{Value, json};

/// The oracle's candidates: its id, position and whether it is invisible.
const CANDIDATES: [(u32, [f64; 3], bool); 5] = [
    (2, [10.0, 1.0, 0.0], false),
    (3, [10.0, 1.0, 0.0], true),
    (4, [15.0, 1.0, 15.0], false),
    (6, [1.5, 1.0, 0.0], true),
    (7, [20.0, 1.0, 0.0], false),
];

impl Game<'_> {
    /// `villager_witness`: a murder of the victim at the origin by one player, three times. `witnesses` and `fresh` are
    /// the oracle's ids of the candidates that take its gossip; `cached` repeats with the first two candidates swapped
    /// in visibility. The ECS has no scan cache, so `cached` is its instant answer.
    pub(super) fn op_villager_witness(&mut self, _op: &Value) -> Result<Value> {
        let victim = self.ecs_spawn("minecraft:villager", [0.0, 1.0, 0.0], None).ok_or("villager_witness: no villager")?;
        self.ecs.set_no_ai(victim, true);
        let mut spawned = Vec::new();
        for (oracle, at, hidden) in CANDIDATES {
            let id = self.ecs_spawn("minecraft:villager", at, None).ok_or("villager_witness: no villager")?;
            self.ecs.set_no_ai(id, true);
            self.set_invisible(id, hidden);
            spawned.push((id, oracle));
        }
        let [near, hidden] = [spawned[0].0, spawned[1].0];
        let witnesses = self.villager_witnesses(victim, 1, &spawned)?;
        self.set_invisible(near, true);
        self.set_invisible(hidden, false);
        let cached = self.villager_witnesses(victim, 2, &spawned)?;
        let fresh = self.villager_witnesses(victim, 3, &spawned)?;
        for id in spawned.iter().map(|&(id, _)| id).chain([victim]) {
            if let Some(e) = self.ecs.entity_of(id) {
                self.ecs.despawn(e);
            }
        }
        Ok(json!({"witnesses": witnesses, "cached": cached, "fresh": fresh}))
    }

    /// `Villager.tellWitnessesThatIWasMurdered` of `victim` by `player`; the oracle ids of the candidates whose
    /// reputation of `player` is now set.
    fn villager_witnesses(&mut self, victim: u32, player: u32, candidates: &[(u32, u32)]) -> Result<Vec<u32>> {
        let dead = self.ecs.entity_of(victim).ok_or("villager_witness: no victim entity")?;
        let w = self.ecs.ecs_mut().ok_or("villager_witness: no ecs")?.world_mut();
        gossip::witness_murder(w, dead, player);
        let mut out = Vec::new();
        for &(id, oracle) in candidates {
            if self.ecs.villager_reputation_of(id, player) != 0 {
                out.push(oracle);
            }
        }
        Ok(out)
    }

    /// Sets or clears the invisibility of entity `id` (the `Invisible` marker that `targeting` reads).
    fn set_invisible(&mut self, id: u32, hidden: bool) {
        let Some(e) = self.ecs.entity_of(id) else { return };
        let Some(w) = self.ecs.ecs_mut() else { return };
        let mut entity = w.world_mut().entity_mut(e);
        if hidden {
            entity.insert(Invisible);
        } else {
            entity.remove::<Invisible>();
        }
    }
}
