//! penrose :: minimal configuration
//!
//! This file will give you a functional if incredibly minimal window manager that
//! has multiple workspaces and simple client / workspace movement.
use color_eyre::eyre::{Context, Result};
use penrose::{
    builtin::{
        actions::{exit, modify_with, send_layout_message, spawn},
        layout::{
            messages::{ExpandMain, IncMain, ShrinkMain},
            transformers::{Gaps, ReflectHorizontal, ReserveTop},
            MainAndStack, Monocle,
        },
    },
    core::{
        bindings::{parse_keybindings_with_xmodmap, KeyEventHandler},
        layout::Layout,
        Config, WindowManager,
    },
    extensions::hooks::{add_ewmh_hooks, SpawnOnStartup},
    map, stack,
    x11rb::RustConn,
};

use favilo_penrose::{
    bar::status_bar,
    hooks::{manage_hook, refresh_hooks},
    BAR_HEIGHT_PX, STARTUP_SCRIPT, raw_key_bindings,
};

use std::collections::HashMap;
use tracing_subscriber::{self, prelude::*};



fn layouts() -> penrose::pure::Stack<Box<dyn Layout>> {
    let max_main = 1;
    let ratio = 0.6;
    let ratio_step = 0.1;
    let outer_px = 0;
    let inner_px = 0;

    stack!(
        MainAndStack::side(max_main, ratio, ratio_step),
        ReflectHorizontal::wrap(MainAndStack::side(max_main, ratio, ratio_step)),
        MainAndStack::bottom(max_main, ratio, ratio_step),
        Monocle::boxed()
    )
    .map(|layout| ReserveTop::wrap(Gaps::wrap(layout, outer_px, inner_px), BAR_HEIGHT_PX))
}

fn main() -> Result<()> {
    color_eyre::install()?;

    tracing_subscriber::fmt()
        .with_env_filter("info")
        .finish()
        .init();

    let config = add_ewmh_hooks(Config {
        startup_hook: Some(SpawnOnStartup::boxed(STARTUP_SCRIPT)),
        default_layouts: layouts(),
        manage_hook: Some(manage_hook()),
        // refresh_hook: Some(refresh_hooks()),
        ..Config::default()
    });

    let conn = RustConn::new().context("X conn")?;
    let key_bindings =
        parse_keybindings_with_xmodmap(raw_key_bindings()).context("Parse keybindings")?;

    // let bar = status_bar().context("Create status bar")?;

    let wm = WindowManager::new(config, key_bindings, HashMap::new(), conn)
        .context("New window manager")?;

    wm.run().context("Window manager run")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_parse_correctly_with_xmodmap() {
        let res = parse_keybindings_with_xmodmap(raw_key_bindings());

        if let Err(e) = res {
            panic!("{e}");
        }
    }
}
