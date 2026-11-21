//! The one event channel: each lot's output resource is moved into the `HostEvent` list `step` returns, in lot order.
//! A resource that was never created, or is empty, costs one lookup and no allocation.
use crate::host_out::HostEvent;
use bevy_ecs::world::World;

macro_rules! lots {
    ($($res:ty => $variant:ident),* $(,)?) => {
        /// Moves every lot's queued events into `out`.
        pub fn drain_lots(world: &mut World, out: &mut Vec<HostEvent>) {
            $(
                if let Some(mut r) = world.get_resource_mut::<$res>() {
                    if !r.0.is_empty() {
                        out.extend(r.0.drain(..).map(HostEvent::$variant));
                    }
                }
            )*
        }
    };
}

lots! {
    crate::host_out_arrows::ArrowOut => Arrow,
    crate::boat_effects::BoatEffectEvents => BoatEffect,
    crate::boat_hurt::BoatHurtEvents => BoatHurt,
    crate::boat_tick::BoatEvents => Boat,
    crate::host_out_cart_move::CartMoveOut => CartMove,
    crate::host_out_cart_variants::CartOut => Cart,
    crate::host_out_equipment::EquipmentOut => Equipment,
    crate::firework_burst::FireworkOut => Firework,
    crate::general::GeneralEvents => General,
    crate::host_out_hook::HookOut => Hook,
    crate::host_out_inside::InsideOut => Inside,
    crate::host_out_items::ItemEvents => Item,
    crate::colony_register::MiscOut => Misc,
    crate::seat::SeatEvents => Seat,
    crate::tnt_tick::TntOut => Tnt,
    crate::host_out_trident::TridentOut => Trident,
    crate::host_out_using::UsingOut => Using,
    crate::host_out_vehicle_push::VehiclePushEvents => VehiclePush,
}
