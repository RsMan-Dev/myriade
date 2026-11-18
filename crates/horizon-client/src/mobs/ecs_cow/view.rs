//! The view a system gets of one cow: every component of its entity at once (`Mut` so a write is a change).
use super::components::*;
use bevy_ecs::{change_detection::Mut, query::QueryData};

#[derive(QueryData)]
#[query_data(mutable)]
pub struct Cow {
    pub id: &'static mut Ident,
    pub motion: &'static mut Motion,
    pub vitals: &'static mut Vitals,
    pub env: &'static mut Env,
    pub clk: &'static mut Clk,
    pub random: &'static mut Rand,
    pub fluids: &'static mut FluidsC,
    pub ai: &'static mut AiC,
    pub extra: &'static mut ExtraC,
    pub attrs: &'static mut AttrsC,
    pub anim: &'static mut AnimC,
    pub goals: &'static mut GoalsC,
}
