//! penrose :: minimal configuration
//!
//! This file will give you a functional if incredibly minimal window manager that
//! has multiple workspaces and simple client / workspace movement.
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

use std::{collections::HashMap, env, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use penrose::{
    core::{Config, WindowManager},
    extensions::hooks::{add_ewmh_hooks, SpawnOnStartup},
    x11rb::RustConn,
};

use favilo_penrose::{
    config::{
        config_path, ensure_config_file, ConfigKeybindingHook, ConfigKeybindingStartup,
        ConfigWatcher,
    },
    extensions::directional_navigation_history::DirectionalNavigationHistoryHook,
    hooks::manage_hook,
    layouts::layouts,
    mouse::mouse_bindings,
    STARTUP_SCRIPT,
};

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{self, fmt, prelude::*, EnvFilter};

fn main() -> Result<()> {
    setup_logging()?;

    let path = config_path()?;
    ensure_config_file(&path)?;
    let keybinding_watcher = ConfigWatcher::new(path)?;

    let mut config = add_ewmh_hooks(Config {
        startup_hook: Some(SpawnOnStartup::boxed(STARTUP_SCRIPT)),
        event_hook: Some(Box::new(ConfigKeybindingHook)),
        default_layouts: layouts(),
        manage_hook: Some(manage_hook()),
        focus_follow_mouse: true,
        ..Config::default()
    });
    config.compose_or_set_startup_hook(ConfigKeybindingStartup);
    config.compose_or_set_refresh_hook(DirectionalNavigationHistoryHook);

    let conn = RustConn::new().context("X conn")?;

    // let bar = status_bar().context("Create status bar")?;

    let mouse_bindings = mouse_bindings();

    let mut wm = WindowManager::new(config, HashMap::new(), mouse_bindings, conn)
        .context("New window manager")?;
    wm.add_extension(keybinding_watcher);

    wm.run().context("Window manager run")?;
    Ok(())
}

fn setup_logging() -> Result<()> {
    let config_home = match env::var_os("XDG_CONFIG_HOME") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?)
            .join(".config"),
    };
    let penrose_home = config_home.join("penrose");
    let log_home = penrose_home.join("logs");
    std::fs::create_dir_all(&log_home)?;

    let log_file = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("penrose")
        .filename_suffix("log")
        .max_log_files(14)
        .build(log_home)?;

    let env_filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;
    let subsriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::Layer::new().with_ansi(false).with_writer(log_file))
        .with(fmt::Layer::new().pretty().with_writer(std::io::stdout));
    tracing::subscriber::set_global_default(subsriber)?;

    Ok(())
}

#[cfg(test)]
mod tests {}
