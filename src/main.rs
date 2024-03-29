//! penrose :: minimal configuration
//!
//! This file will give you a functional if incredibly minimal window manager that
//! has multiple workspaces and simple client / workspace movement.
use color_eyre::eyre::{Context, Result};
use penrose::{
    core::{
        bindings::{parse_keybindings_with_xmodmap, MouseBindings},
        Config, WindowManager,
    },
    extensions::hooks::{add_ewmh_hooks, SpawnOnStartup},
    x11rb::RustConn,
};

use favilo_penrose::{
    bindings::{mouse_bindings, raw_key_bindings},
    hooks::manage_hook,
    layouts::layouts,
    STARTUP_SCRIPT,
};

use tracing_subscriber::{self, prelude::*};

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

    let mouse_bindings = mouse_bindings();
    let wm = WindowManager::new(config, key_bindings, mouse_bindings, conn)
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
