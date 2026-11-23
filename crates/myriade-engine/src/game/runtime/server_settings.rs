//! What a server keeps of its operator's choices besides `server.properties`: the distances it
//! serves, the packs it runs, and its debug switches.
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `view-distance`, in chunks.
    pub view_distance: i32,
    /// `simulation-distance`, in chunks.
    pub simulation_distance: i32,
    /// `entity-broadcast-range-percentage` as a factor.
    pub entity_distance: f32,
    /// The resource packs served, in order.
    pub resource_packs: Vec<String>,
    /// Pregenerates the spawn area before the world opens.
    pub pregen: bool,
    /// Mobs stand still: no goals, no brains.
    pub no_ai: bool,
    /// The control socket is open.
    pub agent_control: bool,
    /// 1.8 combat: no cooldown, no sweep, the old weapon damage.
    pub legacy_combat: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            view_distance: 12,
            simulation_distance: 12,
            entity_distance: 1.0,
            resource_packs: Vec::new(),
            pregen: false,
            no_ai: false,
            agent_control: false,
            legacy_combat: false,
        }
    }
}

impl Settings {
    /// The defaults: the server has no mod list.
    pub fn load(_dir: &Path) -> Self {
        Self::default()
    }
}
