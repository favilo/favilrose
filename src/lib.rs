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

pub mod bar;
pub mod bindings;
pub mod config;
pub mod extensions;
pub mod hooks;
pub mod layouts;
pub mod mouse;

pub const STARTUP_SCRIPT: &str = "/usr/local/scripts/penrose-startup.sh";

const FONT: &str = "ProFontIIx Nerd Font";
const BLACK: u32 = 0x2828_28ff;
const WHITE: u32 = 0xebdb_b2ff;
const GREY: u32 = 0x3c38_36ff;
const BLUE: u32 = 0x4585_88ff;

pub const BAR_HEIGHT_PX: u32 = 28;
const POINT_SIZE: u8 = 10;

const MAX_ACTIVE_WINDOW_CHARS: usize = 50;
