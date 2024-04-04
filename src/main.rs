//! penrose :: minimal configuration
//!
//! This file will give you a functional if incredibly minimal window manager that
//! has multiple workspaces and simple client / workspace movement.
use std::{fs::File, path::PathBuf, sync::Arc};

use color_eyre::eyre::{Context, Result};
use penrose::{
    core::{bindings::parse_keybindings_with_xmodmap, Config, WindowManager},
    extensions::hooks::{add_ewmh_hooks, SpawnOnStartup},
    x11rb::RustConn,
};

use favilo_penrose::{
    bindings::raw_key_bindings,
    hooks::manage_hook,
    layouts::layouts,
    mouse::{mouse_bindings, MouseHandler},
    STARTUP_SCRIPT,
};

use tracing_subscriber::{self, prelude::*};

fn main() -> Result<()> {
    setup_logging()?;

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

    let mouse_handler = MouseHandler::new();
    let mouse_bindings = mouse_bindings();
    let mut wm = WindowManager::new(config, key_bindings, mouse_bindings, conn)
        .context("New window manager")?;

    wm.add_extension(mouse_handler);

    wm.run().context("Window manager run")?;
    Ok(())
}

fn setup_logging() -> Result<()> {
    color_eyre::install()?;

    // Create log directory if it doesn't exist
    let config_home = PathBuf::from(
        std::env::var("XDG_CONFIG_HOME")
            .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap())),
    );
    let penrose_home = config_home.join("penrose");
    let log_home = penrose_home.join("logs");
    std::fs::create_dir_all(&log_home)?;

    let log_file = File::create(&log_home.join("penrose.log"))?;

    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_writer(Arc::new(log_file))
        .finish()
        .init();
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
