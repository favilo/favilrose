#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

#[cfg(not(target_os = "macos"))]
mod x11 {
    #[path = "../bar.rs"]
    pub mod bar;
    pub type WmConn = penrose::x11rb::RustConn;

    pub const FONT: &str = "ProFontIIx Nerd Font";
    pub const BLACK: u32 = 0x2828_28ff;
    pub const WHITE: u32 = 0xebdb_b2ff;
    pub const GREY: u32 = 0x3c38_36ff;
    pub const BLUE: u32 = 0x4585_88ff;
    pub const POINT_SIZE: u8 = 10;
    pub const MAX_ACTIVE_WINDOW_CHARS: usize = 50;
}
#[cfg(not(target_os = "macos"))]
pub use x11::*;

#[cfg(target_os = "macos")]
mod macosx {
    pub type WmConn = penrosx::OsxConn;
}
#[cfg(target_os = "macos")]
pub use macosx::*;

pub mod bindings;
pub mod config;
pub mod extensions;
pub mod hooks;
pub mod layouts;
pub mod mouse;

pub const STARTUP_SCRIPT: &str = "/usr/local/scripts/penrose-startup.sh";
pub const BAR_HEIGHT_PX: u32 = 28;
