use super::*;

/// Helpers for native mods, kept as stable as the hook points.
// Native mods use these; nothing in Horizon does.
#[allow(dead_code)]
impl Game<'_> {
    /// A snapshot of the current graphics preferences.
    pub(in crate::game) fn graphics_options(&self) -> ClientGraphicsOptions {
        ClientGraphicsOptions {
            fov: 0.0,
            render_distance: 0,
            simulation_distance: 0,
            brightness: 0.0,
            max_fps: 0,
            vsync: false,
            fullscreen: false,
            view_bobbing: false,
            smooth_lighting: false,
            particles: 0,
            entity_distance: 0.0,
            biome_blend: 0,
        }
    }
    /// A snapshot of the current GUI and accessibility preferences.
    pub(in crate::game) fn gui_options(&self) -> ClientGuiOptions {
        ClientGuiOptions {
            gui_scale: 0,
            attack_indicator: 0,
            main_hand_left: false,
            reduced_debug_info: false,
            advanced_item_tooltips: false,
            high_contrast: false,
            show_subtitles: false,
            notification_display_time: 0.0,
            text_background_opacity: 0.0,
            menu_blur: 0,
        }
    }
    /// A snapshot of the current chat preferences.
    pub(in crate::game) fn chat_options(&self) -> ClientChatOptions {
        ClientChatOptions {
            visibility: 0,
            colors: false,
            links: false,
            opacity: 0.0,
            scale: 0.0,
            width: 0.0,
            height_focused: 0.0,
            height_unfocused: 0.0,
            line_spacing: 0.0,
            delay: 0.0,
        }
    }
    /// A snapshot of the current audio preferences.
    pub(in crate::game) fn audio_options(&self) -> ClientAudioOptions {
        ClientAudioOptions { master_volume: 0.0 }
    }
}
