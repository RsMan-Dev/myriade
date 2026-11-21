//! The bridge between the server's entities and the ECS (greffe L0): the table of network ids, the view of an entity as the
//! tracker reads it, and the drain of `HostOut` as a typed list. Nothing scans the entities: each piece runs at the act that
//! changes it (a mutator, a component hook), and the view is read on demand.
pub mod events;
pub mod ids;
pub mod view;
