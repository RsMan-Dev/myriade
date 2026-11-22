//! The game's side of the ECS bridge: the tick, the players' facts, the host events, the chunks, the spawns and the tracking.
mod tick;
mod facts;
mod host_events;
mod chunks;
mod spawn;
mod tracking;
mod passagers;
mod potions;
mod commandes_base;
mod commandes_donnees;
mod reseau_interactions;
mod laisses_montures;
mod distributeur_masse;
mod melee_explosions;
mod monde_divers;
mod vehicules;
mod fixes_generales;
mod mob_events;
mod mob_events_vivants;
mod mod_vivants;
mod hooks;
mod serveur_objets_tenus;
mod objets_orbes_tombants;
mod projectiles_feux;
mod mod_entites;
mod mod_mobs;
mod teams;
mod serveur_blocs;
mod arrows;
pub(in crate::game) mod inside;
mod carts;
mod falling;
mod firework;
mod frames;
mod general;
mod hook;
mod misc;
mod portal;
mod tnt;
mod trident;
mod modificateurs_objets;
mod boats;
mod sorties;
#[cfg(test)]
pub(in crate::game) mod test_support;
#[cfg(test)]
mod objets_sol_nbt_tests;
#[cfg(test)]
mod behaviour_measures;
#[cfg(test)]
mod behaviour_probe;
