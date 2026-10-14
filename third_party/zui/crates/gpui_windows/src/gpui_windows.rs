#![cfg(target_os = "windows")]

mod clipboard;
mod destination_list;
mod direct_manipulation;
mod direct_write;
mod directx_atlas;
mod directx_devices;
mod directx_renderer;
mod dispatcher;
mod display;
mod events;
mod keyboard;
mod platform;
mod system_notifications;
mod system_settings;
mod util;
mod vsync;
mod window;
mod wrapper;

pub(crate) use clipboard::*;
pub(crate) use destination_list::*;
pub(crate) use direct_write::*;
pub(crate) use directx_atlas::*;
pub(crate) use directx_devices::*;
pub(crate) use directx_renderer::*;
pub(crate) use dispatcher::*;
pub(crate) use display::*;
pub(crate) use events::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
pub(crate) use system_notifications::*;
pub(crate) use system_settings::*;
pub(crate) use util::*;
pub(crate) use vsync::*;
pub(crate) use window::*;
pub(crate) use wrapper::*;

pub use platform::WindowsPlatform;

pub(crate) use windows::Win32::Foundation::HWND;

#[cfg(test)]
mod tests {
    use gpui::{AtlasTile, EdgeFadeParams, ImageAlphaMaskParams, PolychromeSprite, Quad};
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn edge_fade_host_layout_is_fourteen_packed_floats() {
        assert_eq!(align_of::<EdgeFadeParams>(), align_of::<f32>());
        assert_eq!(size_of::<EdgeFadeParams>(), 56);
    }

    #[test]
    fn quad_host_layout_includes_trailing_edge_fade() {
        assert_eq!(offset_of!(Quad, fade), 160);
        assert_eq!(size_of::<Quad>(), 216);
    }

    #[test]
    fn polychrome_host_layout_keeps_fade_before_atlas_tile() {
        assert_eq!(offset_of!(PolychromeSprite, fade), 64);
        assert_eq!(offset_of!(PolychromeSprite, alpha_mask), 120);
        assert_eq!(offset_of!(PolychromeSprite, tile), 160);
        assert_eq!(size_of::<AtlasTile>(), 32);
        assert_eq!(size_of::<PolychromeSprite>(), 192);
    }

    #[test]
    fn image_alpha_mask_host_layout() {
        assert_eq!(size_of::<ImageAlphaMaskParams>(), 40);
        assert_eq!(offset_of!(ImageAlphaMaskParams, radius), 16);
        assert_eq!(offset_of!(ImageAlphaMaskParams, bottom_feather), 32);
    }
}
